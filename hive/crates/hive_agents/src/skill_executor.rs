//! Capability-aware skill executor.
//!
//! Loads a skill, checks whether the active model satisfies the skill's
//! requirements, enhances the prompt based on the model's preferred
//! capabilities, and builds a ready-to-send [`ChatRequest`].

use anyhow::{Result, bail};

use hive_ai::types::{
    ChatMessage, ChatRequest, MessageRole, ModelCapability, ModelInfo, ModelTier, ToolDefinition,
};

use crate::skill_format::SkillFile;

// ---------------------------------------------------------------------------
// Tier ordering helper
// ---------------------------------------------------------------------------

/// Returns a numeric rank for a model tier (higher = more capable).
fn tier_rank(tier: ModelTier) -> u8 {
    match tier {
        ModelTier::Free => 0,
        ModelTier::Budget => 1,
        ModelTier::Mid => 2,
        ModelTier::Premium => 3,
    }
}

// ---------------------------------------------------------------------------
// SkillExecutor
// ---------------------------------------------------------------------------

/// Adapts a skill's prompt to the active model and builds a `ChatRequest`.
pub struct SkillExecutor;

impl SkillExecutor {
    /// Validate that `model` satisfies the skill's requirements.
    ///
    /// Returns a human-readable error if the model is incompatible.
    pub fn validate(skill: &SkillFile, model: &ModelInfo) -> Result<()> {
        // Check required capabilities
        for cap in &skill.requirements.capabilities {
            if !model.capabilities.has(*cap) {
                bail!(
                    "Skill '{}' requires {:?} but model '{}' does not support it. \
                     Switch to a model with this capability.",
                    skill.skill.name,
                    cap,
                    model.name,
                );
            }
        }

        // Check minimum tier
        if tier_rank(model.tier) < tier_rank(skill.requirements.min_tier) {
            bail!(
                "Skill '{}' requires at least {:?} tier but model '{}' is {:?}. \
                 Switch to a higher-tier model.",
                skill.skill.name,
                skill.requirements.min_tier,
                model.name,
                model.tier,
            );
        }

        Ok(())
    }

    /// Build an enhanced prompt from the skill template, adapting to the
    /// model's capabilities.
    pub fn build_prompt(skill: &SkillFile, model: &ModelInfo, user_context: &str) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Prepend thinking instruction if model supports it
        if model.capabilities.has(ModelCapability::ExtendedThinking)
            && skill
                .requirements
                .preferred
                .contains(&ModelCapability::ExtendedThinking)
        {
            parts.push("Think step by step before answering. Show your reasoning.".into());
        }

        // Main template
        parts.push(skill.prompt.template.clone());

        // Tool use hint
        if model.capabilities.has(ModelCapability::ToolUse) {
            if let Some(hint) = &skill.prompt.tool_use_hint {
                parts.push(hint.clone());
            }
        }

        // Structured output hint
        if model.capabilities.has(ModelCapability::StructuredOutput) {
            if let Some(hint) = &skill.prompt.structured_output_hint {
                parts.push(hint.clone());
            }
        }

        // Append user context
        if !user_context.is_empty() {
            parts.push(format!("\n---\n{user_context}"));
        }

        parts.join("\n\n")
    }

    /// Build a complete `ChatRequest` ready to send to `AiService`.
    ///
    /// - Validates model compatibility
    /// - Enhances the prompt based on model capabilities
    /// - Injects required tool definitions (caller provides the full defs)
    pub fn build_request(
        skill: &SkillFile,
        model: &ModelInfo,
        user_context: &str,
        available_tools: &[ToolDefinition],
    ) -> Result<ChatRequest> {
        Self::validate(skill, model)?;

        let prompt = Self::build_prompt(skill, model, user_context);

        // Filter available tools to those required/optional by the skill
        let skill_tools: Vec<ToolDefinition> = available_tools
            .iter()
            .filter(|t| {
                skill.tools.required.contains(&t.name) || skill.tools.optional.contains(&t.name)
            })
            .cloned()
            .collect();

        let tools = if skill_tools.is_empty() {
            None
        } else {
            Some(skill_tools)
        };

        Ok(ChatRequest {
            messages: vec![ChatMessage::text(MessageRole::User, prompt)],
            model: model.id.clone(),
            max_tokens: 4096,
            temperature: None,
            system_prompt: Some(format!(
                "You are executing the '{}' skill. {}",
                skill.skill.name, skill.skill.description
            )),
            tools,
            cache_system_prompt: true,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hive_ai::types::{ModelCapabilities, ProviderType};

    use crate::skill_format::{
        SkillFileSource, SkillMeta, SkillMetadata, SkillPrompt, SkillRequirements, SkillTools,
    };
    use crate::skill_marketplace::SkillCategory;

    fn test_skill() -> SkillFile {
        SkillFile {
            skill: SkillMeta {
                name: "code-review".into(),
                description: "Review code".into(),
                version: "1.0.0".into(),
                category: SkillCategory::CodeGeneration,
                author: "test".into(),
                source: SkillFileSource::Builtin,
                enabled: true,
            },
            requirements: SkillRequirements {
                capabilities: vec![ModelCapability::ToolUse],
                preferred: vec![ModelCapability::ExtendedThinking],
                min_tier: ModelTier::Mid,
            },
            prompt: SkillPrompt {
                template: "Review the following code.".into(),
                tool_use_hint: Some("Use read_file to examine code.".into()),
                structured_output_hint: Some("Return JSON.".into()),
            },
            tools: SkillTools {
                required: vec!["read_file".into()],
                optional: vec!["list_directory".into()],
            },
            metadata: SkillMetadata::default(),
        }
    }

    fn premium_model() -> ModelInfo {
        ModelInfo {
            id: "claude-opus-4".into(),
            name: "Claude Opus 4".into(),
            provider: "anthropic".into(),
            provider_type: ProviderType::Anthropic,
            tier: ModelTier::Premium,
            context_window: 200_000,
            input_price_per_mtok: 15.0,
            output_price_per_mtok: 75.0,
            capabilities: ModelCapabilities::new(&[
                ModelCapability::ToolUse,
                ModelCapability::ExtendedThinking,
                ModelCapability::StructuredOutput,
                ModelCapability::LongContext,
            ]),
            release_date: Some("2025-05-22".into()),
        }
    }

    fn budget_model() -> ModelInfo {
        ModelInfo {
            id: "llama-3".into(),
            name: "Llama 3".into(),
            provider: "ollama".into(),
            provider_type: ProviderType::Ollama,
            tier: ModelTier::Budget,
            context_window: 8192,
            input_price_per_mtok: 0.0,
            output_price_per_mtok: 0.0,
            capabilities: ModelCapabilities::new(&[]),
            release_date: None,
        }
    }

    fn mid_model_with_tools() -> ModelInfo {
        ModelInfo {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            provider: "openai".into(),
            provider_type: ProviderType::OpenAI,
            tier: ModelTier::Mid,
            context_window: 128_000,
            input_price_per_mtok: 2.5,
            output_price_per_mtok: 10.0,
            capabilities: ModelCapabilities::new(&[
                ModelCapability::ToolUse,
                ModelCapability::StructuredOutput,
            ]),
            release_date: Some("2024-05-13".into()),
        }
    }

    #[test]
    fn validate_compatible_model() {
        let skill = test_skill();
        let model = premium_model();
        assert!(SkillExecutor::validate(&skill, &model).is_ok());
    }

    #[test]
    fn validate_missing_capability() {
        let skill = test_skill();
        let model = budget_model();
        let err = SkillExecutor::validate(&skill, &model).unwrap_err();
        assert!(err.to_string().contains("ToolUse"));
    }

    #[test]
    fn validate_insufficient_tier() {
        let skill = test_skill();
        let mut model = mid_model_with_tools();
        model.tier = ModelTier::Budget; // skill requires Mid
        let err = SkillExecutor::validate(&skill, &model).unwrap_err();
        assert!(err.to_string().contains("tier"));
    }

    #[test]
    fn prompt_enhanced_for_premium_model() {
        let skill = test_skill();
        let model = premium_model();
        let prompt = SkillExecutor::build_prompt(&skill, &model, "some code here");
        assert!(prompt.contains("Think step by step"));
        assert!(prompt.contains("Review the following code"));
        assert!(prompt.contains("Use read_file"));
        assert!(prompt.contains("Return JSON"));
        assert!(prompt.contains("some code here"));
    }

    #[test]
    fn prompt_minimal_for_basic_model() {
        let skill = test_skill();
        let model = mid_model_with_tools();
        let prompt = SkillExecutor::build_prompt(&skill, &model, "");
        // No extended thinking (mid model doesn't have it)
        assert!(!prompt.contains("Think step by step"));
        // But does have tool use and structured output
        assert!(prompt.contains("Use read_file"));
        assert!(prompt.contains("Return JSON"));
    }

    #[test]
    fn build_request_filters_tools() {
        let skill = test_skill();
        let model = premium_model();
        let all_tools = vec![
            ToolDefinition {
                name: "read_file".into(),
                description: "Read a file".into(),
                input_schema: serde_json::json!({}),
            },
            ToolDefinition {
                name: "write_file".into(),
                description: "Write a file".into(),
                input_schema: serde_json::json!({}),
            },
            ToolDefinition {
                name: "list_directory".into(),
                description: "List dir".into(),
                input_schema: serde_json::json!({}),
            },
        ];

        let request = SkillExecutor::build_request(&skill, &model, "code", &all_tools).unwrap();
        let tools = request.tools.unwrap();
        assert_eq!(tools.len(), 2); // read_file + list_directory, NOT write_file
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"list_directory"));
        assert!(!names.contains(&"write_file"));
    }

    #[test]
    fn build_request_fails_for_incompatible_model() {
        let skill = test_skill();
        let model = budget_model();
        assert!(SkillExecutor::build_request(&skill, &model, "", &[]).is_err());
    }
}
