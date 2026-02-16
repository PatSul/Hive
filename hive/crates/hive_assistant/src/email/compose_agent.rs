use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

use hive_ai::service::AiService;
use hive_ai::types::{ChatMessage, MessageRole};

use crate::email::UnifiedEmail;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A drafted email ready for review before sending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftedEmail {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// How confident the agent is in this draft (0.0 - 1.0).
    pub confidence: f64,
}

// ---------------------------------------------------------------------------
// ComposeAgent
// ---------------------------------------------------------------------------

/// Agent that drafts emails from natural-language instructions or in reply
/// to existing emails using AI generation.
///
/// When an `AiService` is provided, drafts are generated via the configured
/// LLM. Without one, the agent falls back to template-based drafting.
pub struct ComposeAgent {
    ai_service: Option<Arc<Mutex<AiService>>>,
}

impl ComposeAgent {
    /// Create a compose agent without AI (template-based fallback).
    pub fn new() -> Self {
        Self { ai_service: None }
    }

    /// Create a compose agent backed by an AI service for real generation.
    pub fn with_ai(ai_service: Arc<Mutex<AiService>>) -> Self {
        Self {
            ai_service: Some(ai_service),
        }
    }

    /// Draft an email from a natural-language instruction.
    ///
    /// For example: "Send a follow-up to Alice about the Q1 report."
    pub fn draft_from_instruction(&self, instruction: &str) -> Result<DraftedEmail, String> {
        if let Some(ref ai) = self.ai_service {
            return self.draft_with_ai(instruction, None, ai);
        }

        // Fallback: template-based drafting.
        Ok(DraftedEmail {
            to: String::new(),
            subject: format!("Re: {instruction}"),
            body: format!("Draft based on instruction: {instruction}"),
            confidence: 0.0,
        })
    }

    /// Draft a reply to an existing email.
    pub fn draft_reply(
        &self,
        original: &UnifiedEmail,
        instruction: &str,
    ) -> Result<DraftedEmail, String> {
        if let Some(ref ai) = self.ai_service {
            return self.draft_with_ai(instruction, Some(original), ai);
        }

        // Fallback: template-based drafting.
        Ok(DraftedEmail {
            to: original.from.clone(),
            subject: format!("Re: {}", original.subject),
            body: format!(
                "Reply to '{}' based on instruction: {instruction}",
                original.subject
            ),
            confidence: 0.0,
        })
    }

    /// Use the AI service to generate a draft email.
    fn draft_with_ai(
        &self,
        instruction: &str,
        original: Option<&UnifiedEmail>,
        ai_service: &Arc<Mutex<AiService>>,
    ) -> Result<DraftedEmail, String> {
        let system_prompt = "You are an expert email composer. Given an instruction, \
            draft a professional email. Respond in JSON format with the fields: \
            \"to\" (email address or empty if unknown), \"subject\", and \"body\". \
            Be concise, professional, and match the requested tone.";

        let user_content = if let Some(orig) = original {
            format!(
                "Reply to this email:\nFrom: {}\nSubject: {}\nBody: {}\n\nInstruction: {}",
                orig.from, orig.subject, orig.body, instruction
            )
        } else {
            format!("Instruction: {instruction}")
        };

        let messages = vec![
            ChatMessage::text(MessageRole::System, system_prompt),
            ChatMessage::text(MessageRole::User, &user_content),
        ];

        let handle = Handle::try_current().map_err(|e| format!("No tokio runtime: {e}"))?;
        let model = {
            let svc = ai_service
                .lock()
                .map_err(|e| format!("Lock error: {e}"))?;
            svc.default_model().to_string()
        };

        let response = handle.block_on(async {
            let mut svc = ai_service
                .lock()
                .map_err(|e| format!("Lock error: {e}"))?;
            svc.chat(messages, &model, None)
                .await
                .map_err(|e| format!("AI chat error: {e}"))
        })?;

        // Try to parse the AI response as JSON.
        let content = &response.content;
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
            Ok(DraftedEmail {
                to: parsed
                    .get("to")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                subject: parsed
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                body: parsed
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or(content)
                    .to_string(),
                confidence: 0.85,
            })
        } else {
            // If JSON parsing fails, use the raw response as the body.
            let to = original.map(|o| o.from.clone()).unwrap_or_default();
            let subject = original
                .map(|o| format!("Re: {}", o.subject))
                .unwrap_or_else(|| format!("Re: {instruction}"));

            Ok(DraftedEmail {
                to,
                subject,
                body: content.clone(),
                confidence: 0.7,
            })
        }
    }
}

impl Default for ComposeAgent {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::email::compose_agent::{ComposeAgent, DraftedEmail};
    use crate::email::{EmailProvider, UnifiedEmail};

    fn make_original_email() -> UnifiedEmail {
        UnifiedEmail {
            id: "orig-1".to_string(),
            from: "alice@example.com".to_string(),
            to: "me@example.com".to_string(),
            subject: "Q1 Report".to_string(),
            body: "Please review the attached Q1 report.".to_string(),
            timestamp: "2026-02-10T10:00:00Z".to_string(),
            provider: EmailProvider::Gmail,
            read: true,
            important: true,
        }
    }

    #[test]
    fn test_draft_from_instruction() {
        let agent = ComposeAgent::new();
        let draft = agent
            .draft_from_instruction("Follow up with Bob about the meeting")
            .unwrap();

        assert!(draft.subject.contains("Follow up with Bob"));
        assert!(draft.body.contains("Follow up with Bob"));
        assert!((draft.confidence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_draft_reply() {
        let agent = ComposeAgent::new();
        let original = make_original_email();

        let draft = agent
            .draft_reply(&original, "Acknowledge and confirm review by Friday")
            .unwrap();

        assert_eq!(draft.to, "alice@example.com");
        assert_eq!(draft.subject, "Re: Q1 Report");
        assert!(draft.body.contains("Q1 Report"));
        assert!(draft.body.contains("Acknowledge"));
    }

    #[test]
    fn test_draft_reply_preserves_sender() {
        let agent = ComposeAgent::new();
        let original = make_original_email();

        let draft = agent.draft_reply(&original, "Thanks!").unwrap();
        assert_eq!(draft.to, original.from);
    }

    #[test]
    fn test_drafted_email_serialization() {
        let draft = DraftedEmail {
            to: "test@example.com".to_string(),
            subject: "Test".to_string(),
            body: "Body".to_string(),
            confidence: 0.85,
        };
        let json = serde_json::to_string(&draft).unwrap();
        let deserialized: DraftedEmail = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.to, "test@example.com");
        assert!((deserialized.confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_compose_agent() {
        let agent = ComposeAgent::default();
        let draft = agent.draft_from_instruction("test").unwrap();
        assert!(draft.subject.contains("test"));
    }

    #[test]
    fn test_new_without_ai() {
        let agent = ComposeAgent::new();
        assert!(agent.ai_service.is_none());
    }
}
