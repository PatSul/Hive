//! Remote A2A agent wrapper for outbound task delegation.
//!
//! [`RemoteAgent`] encapsulates the connection details and agent card for
//! an external A2A agent. It provides methods to send tasks via JSON-RPC
//! and extract outputs from completed tasks.

use a2a_rs::{
    AgentCard, Message, MessageSendConfiguration, MessageSendParams, Part, Role, Task,
};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::bridge::artifact_to_text;
use crate::config::RemoteAgentConfig;
use crate::error::A2aError;

// ---------------------------------------------------------------------------
// RemoteAgent
// ---------------------------------------------------------------------------

/// A wrapper around a remote A2A agent, holding connection info and
/// an optional cached [`AgentCard`].
pub struct RemoteAgent {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub card: Option<AgentCard>,
    client: reqwest::Client,
}

impl RemoteAgent {
    /// Create a `RemoteAgent` from a [`RemoteAgentConfig`].
    pub fn from_config(config: RemoteAgentConfig) -> Self {
        Self {
            name: config.name,
            base_url: config.url,
            api_key: config.api_key,
            card: None,
            client: reqwest::Client::new(),
        }
    }

    /// Create a `RemoteAgent` with a pre-fetched agent card.
    pub fn with_card(
        name: String,
        base_url: String,
        card: AgentCard,
        api_key: Option<String>,
    ) -> Self {
        Self {
            name,
            base_url,
            api_key,
            card: Some(card),
            client: reqwest::Client::new(),
        }
    }

    /// Send a task message to the remote agent via JSON-RPC `message/send`.
    ///
    /// Constructs a [`MessageSendParams`] payload wrapping the given text,
    /// optional `skill_id` in metadata, and the provided `task_id`. The
    /// request is sent as a blocking JSON-RPC call (`blocking: true`) so
    /// the response contains the completed [`Task`].
    pub async fn send_task(
        &self,
        message_text: &str,
        skill_id: Option<&str>,
        task_id: &str,
    ) -> Result<Task, A2aError> {
        // Build metadata with optional skill_id
        let metadata = skill_id.map(|sid| {
            let mut meta = Map::new();
            meta.insert("skill_id".into(), Value::String(sid.into()));
            meta
        });

        let message = Message {
            role: Role::User,
            parts: vec![Part::text(message_text.into())],
            metadata: metadata.clone(),
            reference_task_ids: None,
            message_id: Uuid::new_v4().to_string(),
            task_id: Some(task_id.into()),
            context_id: Some(Uuid::new_v4().to_string()),
            kind: "message".into(),
        };

        let params = MessageSendParams {
            message,
            configuration: Some(MessageSendConfiguration {
                accepted_output_modes: vec!["text".into()],
                history_length: None,
                push_notification_config: None,
                blocking: Some(true),
            }),
            metadata,
        };

        // Build JSON-RPC request
        let rpc_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": Uuid::new_v4().to_string(),
            "method": "message/send",
            "params": params,
        });

        // Build the HTTP request
        let url = format!("{}/", self.base_url.trim_end_matches('/'));
        let mut request = self.client.post(&url).json(&rpc_request);

        if let Some(ref key) = self.api_key {
            request = request.header("X-Hive-Key", key);
        }

        // Send
        let response = request
            .send()
            .await
            .map_err(|e| A2aError::Network(format!("Failed to send task to {}: {e}", self.name)))?;

        if !response.status().is_success() {
            return Err(A2aError::Network(format!(
                "Remote agent {} returned status {}",
                self.name,
                response.status()
            )));
        }

        // Parse the JSON-RPC response
        let rpc_response: Value = response
            .json()
            .await
            .map_err(|e| A2aError::Network(format!("Failed to parse response from {}: {e}", self.name)))?;

        // Check for JSON-RPC error
        if let Some(err) = rpc_response.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown remote error");
            return Err(A2aError::Provider(format!(
                "Remote agent {} error: {msg}",
                self.name
            )));
        }

        // Extract the result as a Task
        let result = rpc_response
            .get("result")
            .ok_or_else(|| {
                A2aError::Network(format!(
                    "Remote agent {} returned no result in JSON-RPC response",
                    self.name
                ))
            })?;

        let task: Task = serde_json::from_value(result.clone()).map_err(|e| {
            A2aError::Bridge(format!(
                "Failed to deserialize Task from {} response: {e}",
                self.name
            ))
        })?;

        Ok(task)
    }

    /// Extract concatenated text output from a completed task's artifacts.
    ///
    /// Returns an empty string if the task has no artifacts.
    pub fn extract_output(task: &Task) -> String {
        match &task.artifacts {
            Some(artifacts) => artifacts
                .iter()
                .map(|a| artifact_to_text(a))
                .collect::<Vec<_>>()
                .join("\n"),
            None => String::new(),
        }
    }

    /// List skill IDs from the cached agent card.
    ///
    /// Returns an empty vector if no agent card is available.
    pub fn available_skills(&self) -> Vec<String> {
        match &self.card {
            Some(card) => card.skills.iter().map(|s| s.id.clone()).collect(),
            None => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use a2a_rs::{AgentCapabilities, AgentSkill, Artifact, TaskState, TaskStatus};

    #[test]
    fn test_from_config() {
        let config = RemoteAgentConfig {
            name: "Test".into(),
            url: "https://test.com".into(),
            api_key: Some("key".into()),
        };
        let agent = RemoteAgent::from_config(config);
        assert_eq!(agent.name, "Test");
        assert_eq!(agent.base_url, "https://test.com");
        assert!(agent.api_key.is_some());
        assert_eq!(agent.api_key.as_deref(), Some("key"));
        assert!(agent.card.is_none());
    }

    #[test]
    fn test_with_card() {
        let card = AgentCard {
            name: "Remote".into(),
            description: "A remote agent".into(),
            url: "https://remote.example.com".into(),
            provider: None,
            version: "1.0.0".into(),
            documentation_url: None,
            capabilities: AgentCapabilities::default(),
            security_schemes: None,
            security: None,
            default_input_modes: vec!["text".into()],
            default_output_modes: vec!["text".into()],
            skills: vec![AgentSkill::new(
                "code-review".into(),
                "Code Review".into(),
                "Review code".into(),
                vec!["review".into()],
            )],
            supports_authenticated_extended_card: None,
        };

        let agent = RemoteAgent::with_card(
            "Remote".into(),
            "https://remote.example.com".into(),
            card,
            Some("secret".into()),
        );

        assert_eq!(agent.name, "Remote");
        assert!(agent.card.is_some());
        assert_eq!(agent.api_key.as_deref(), Some("secret"));
    }

    #[test]
    fn test_available_skills_without_card() {
        let config = RemoteAgentConfig {
            name: "NoCard".into(),
            url: "https://nocard.example.com".into(),
            api_key: None,
        };
        let agent = RemoteAgent::from_config(config);
        assert!(agent.available_skills().is_empty());
    }

    #[test]
    fn test_available_skills_with_card() {
        let card = AgentCard {
            name: "Skilled".into(),
            description: "A skilled agent".into(),
            url: "https://skilled.example.com".into(),
            provider: None,
            version: "1.0.0".into(),
            documentation_url: None,
            capabilities: AgentCapabilities::default(),
            security_schemes: None,
            security: None,
            default_input_modes: vec!["text".into()],
            default_output_modes: vec!["text".into()],
            skills: vec![
                AgentSkill::new(
                    "skill-a".into(),
                    "Skill A".into(),
                    "First skill".into(),
                    vec![],
                ),
                AgentSkill::new(
                    "skill-b".into(),
                    "Skill B".into(),
                    "Second skill".into(),
                    vec![],
                ),
            ],
            supports_authenticated_extended_card: None,
        };

        let agent = RemoteAgent::with_card(
            "Skilled".into(),
            "https://skilled.example.com".into(),
            card,
            None,
        );

        let skills = agent.available_skills();
        assert_eq!(skills, vec!["skill-a", "skill-b"]);
    }

    #[test]
    fn test_extract_output_empty() {
        let task = Task {
            id: "task-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
            kind: "task".into(),
        };
        assert_eq!(RemoteAgent::extract_output(&task), "");
    }

    #[test]
    fn test_extract_output_with_artifacts() {
        let task = Task {
            id: "task-2".into(),
            context_id: "ctx-2".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: Some(vec![
                Artifact {
                    artifact_id: "art-1".into(),
                    name: None,
                    description: None,
                    parts: vec![Part::text("Result from first artifact".into())],
                    metadata: None,
                },
                Artifact {
                    artifact_id: "art-2".into(),
                    name: None,
                    description: None,
                    parts: vec![Part::text("Result from second artifact".into())],
                    metadata: None,
                },
            ]),
            history: None,
            metadata: None,
            kind: "task".into(),
        };

        let output = RemoteAgent::extract_output(&task);
        assert!(output.contains("Result from first artifact"));
        assert!(output.contains("Result from second artifact"));
    }

    #[test]
    fn test_extract_output_empty_artifacts_vec() {
        let task = Task {
            id: "task-3".into(),
            context_id: "ctx-3".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: Some(vec![]),
            history: None,
            metadata: None,
            kind: "task".into(),
        };
        assert_eq!(RemoteAgent::extract_output(&task), "");
    }
}
