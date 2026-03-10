//! Agent Card builder for Hive's A2A identity.
//!
//! Constructs an [`a2a_rs::AgentCard`] (via [`SimpleAgentInfo`]) that describes
//! Hive's multi-agent capabilities and is served at
//! `/.well-known/agent-card.json`.

use a2a_rs::{AgentCapabilities, AgentCard, AgentProvider, AgentSkill, SimpleAgentInfo};

use crate::config::A2aConfig;

// ---------------------------------------------------------------------------
// Skill ID constants
// ---------------------------------------------------------------------------

/// HiveMind multi-agent pipeline (9-role orchestration).
pub const SKILL_HIVEMIND: &str = "hivemind";

/// Task Coordinator (dependency-ordered parallel tasks).
pub const SKILL_COORDINATOR: &str = "coordinator";

/// Queen Swarm Orchestration (multi-team swarm).
pub const SKILL_QUEEN: &str = "queen";

/// Single Agent (one-shot persona call).
pub const SKILL_SINGLE: &str = "single";

/// All supported skill IDs in declaration order.
pub const SUPPORTED_SKILLS: [&str; 4] =
    [SKILL_HIVEMIND, SKILL_COORDINATOR, SKILL_QUEEN, SKILL_SINGLE];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `skill_id` is one of Hive's supported A2A skills.
pub fn is_supported_skill(skill_id: &str) -> bool {
    SUPPORTED_SKILLS.contains(&skill_id)
}

// ---------------------------------------------------------------------------
// Agent Card builder
// ---------------------------------------------------------------------------

/// Build the four [`AgentSkill`] definitions that Hive exposes over A2A.
fn build_skills() -> Vec<AgentSkill> {
    vec![
        AgentSkill::new(
            SKILL_HIVEMIND.to_string(),
            "HiveMind Multi-Agent Pipeline".to_string(),
            "9-role orchestration pipeline: Architect, Planner, Researcher, \
             Developer, Reviewer, Tester, DevOps, Writer, and Coordinator \
             collaborate to solve complex tasks end-to-end."
                .to_string(),
            vec![
                "multi-agent".to_string(),
                "orchestration".to_string(),
                "pipeline".to_string(),
            ],
        )
        .with_examples(vec![
            "Build a REST API with tests and documentation".to_string(),
            "Refactor this module and update all callers".to_string(),
        ])
        .with_input_modes(vec!["text".to_string()])
        .with_output_modes(vec!["text".to_string()]),
        AgentSkill::new(
            SKILL_COORDINATOR.to_string(),
            "Task Coordinator".to_string(),
            "Breaks a goal into dependency-ordered sub-tasks and executes \
             them in parallel where possible, merging results."
                .to_string(),
            vec![
                "coordinator".to_string(),
                "parallel".to_string(),
                "task-graph".to_string(),
            ],
        )
        .with_examples(vec![
            "Run linting, tests, and type-checking in parallel".to_string(),
            "Migrate database then update API and docs".to_string(),
        ])
        .with_input_modes(vec!["text".to_string()])
        .with_output_modes(vec!["text".to_string()]),
        AgentSkill::new(
            SKILL_QUEEN.to_string(),
            "Queen Swarm Orchestration".to_string(),
            "Spawns multiple agent teams (swarms) that work concurrently, \
             each with their own coordinator, merging results at the end."
                .to_string(),
            vec![
                "swarm".to_string(),
                "multi-team".to_string(),
                "orchestration".to_string(),
            ],
        )
        .with_examples(vec![
            "Implement frontend and backend features simultaneously".to_string(),
            "Audit security across all microservices at once".to_string(),
        ])
        .with_input_modes(vec!["text".to_string()])
        .with_output_modes(vec!["text".to_string()]),
        AgentSkill::new(
            SKILL_SINGLE.to_string(),
            "Single Agent".to_string(),
            "Executes a one-shot task with a single AI persona — fast and \
             lightweight for simple requests."
                .to_string(),
            vec![
                "single".to_string(),
                "one-shot".to_string(),
                "lightweight".to_string(),
            ],
        )
        .with_examples(vec![
            "Explain this error message".to_string(),
            "Write a unit test for this function".to_string(),
        ])
        .with_input_modes(vec!["text".to_string()])
        .with_output_modes(vec!["text".to_string()]),
    ]
}

/// Build a [`SimpleAgentInfo`] describing Hive's A2A capabilities.
///
/// The returned value implements `AgentInfoProvider` and can be passed
/// directly to [`a2a_rs::HttpServer`] when starting the A2A endpoint.
///
/// The `base_url` is derived from the provided [`A2aConfig`] as
/// `http://{bind}:{port}`.
pub fn build_hive_agent_info(config: &A2aConfig) -> SimpleAgentInfo {
    let base_url = format!("http://{}:{}", config.server.bind, config.server.port);

    SimpleAgentInfo::new("Hive".to_string(), base_url)
        .with_description(
            "Hive is a multi-agent AI coding assistant exposing HiveMind, \
             Coordinator, Queen, and Single-Agent orchestration over A2A."
                .to_string(),
        )
        .with_provider(
            "AIrglow Studio".to_string(),
            "https://hivecode.app".to_string(),
        )
        .with_version(env!("CARGO_PKG_VERSION").to_string())
        .with_streaming()
        .with_state_transition_history()
        .with_skills(build_skills())
}

/// Build a raw [`AgentCard`] directly (useful for tests and serialisation
/// without pulling in the full `SimpleAgentInfo` wrapper).
pub fn build_hive_agent_card(config: &A2aConfig) -> AgentCard {
    let base_url = format!("http://{}:{}", config.server.bind, config.server.port);

    AgentCard {
        name: "Hive".to_string(),
        description: "Hive is a multi-agent AI coding assistant exposing HiveMind, \
                      Coordinator, Queen, and Single-Agent orchestration over A2A."
            .to_string(),
        url: base_url,
        provider: Some(AgentProvider {
            organization: "AIrglow Studio".to_string(),
            url: "https://hivecode.app".to_string(),
        }),
        version: env!("CARGO_PKG_VERSION").to_string(),
        documentation_url: Some("https://hivecode.app/docs/a2a".to_string()),
        capabilities: AgentCapabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
        },
        security_schemes: None,
        security: None,
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
        skills: build_skills(),
        supports_authenticated_extended_card: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_constants() {
        assert_eq!(SKILL_HIVEMIND, "hivemind");
        assert_eq!(SKILL_COORDINATOR, "coordinator");
        assert_eq!(SKILL_QUEEN, "queen");
        assert_eq!(SKILL_SINGLE, "single");
        assert_eq!(SUPPORTED_SKILLS.len(), 4);
    }

    #[test]
    fn test_is_supported_skill() {
        assert!(is_supported_skill("hivemind"));
        assert!(is_supported_skill("coordinator"));
        assert!(is_supported_skill("queen"));
        assert!(is_supported_skill("single"));
        assert!(!is_supported_skill("unknown"));
        assert!(!is_supported_skill(""));
    }

    #[test]
    fn test_build_hive_agent_card() {
        let config = A2aConfig::default();
        let card = build_hive_agent_card(&config);

        // Name
        assert_eq!(card.name, "Hive");

        // Description mentions multi-agent
        assert!(
            card.description.contains("multi-agent"),
            "description should mention multi-agent: {}",
            card.description
        );

        // Capabilities
        assert!(card.capabilities.streaming);
        assert!(card.capabilities.state_transition_history);
        assert!(!card.capabilities.push_notifications);

        // Skills
        assert_eq!(card.skills.len(), 4);
        let skill_ids: Vec<&str> = card.skills.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            skill_ids,
            vec!["hivemind", "coordinator", "queen", "single"]
        );

        // Each skill has tags and examples
        for skill in &card.skills {
            assert!(!skill.tags.is_empty(), "skill {} has no tags", skill.id);
            assert!(
                skill.examples.is_some(),
                "skill {} has no examples",
                skill.id
            );
        }
    }

    #[test]
    fn test_build_hive_agent_info() {
        let config = A2aConfig::default();
        let info = build_hive_agent_info(&config);

        // Verify skills through the SimpleAgentInfo getter
        let skills = info.get_skills();
        assert_eq!(skills.len(), 4);
        assert_eq!(skills[0].id, SKILL_HIVEMIND);
        assert_eq!(skills[1].id, SKILL_COORDINATOR);
        assert_eq!(skills[2].id, SKILL_QUEEN);
        assert_eq!(skills[3].id, SKILL_SINGLE);

        // Lookup by ID
        assert!(info.get_skill_by_id("hivemind").is_some());
        assert!(info.get_skill_by_id("nonexistent").is_none());
    }

    #[test]
    fn test_agent_card_url_from_config() {
        let mut config = A2aConfig::default();
        config.server.bind = "0.0.0.0".to_string();
        config.server.port = 9000;

        let card = build_hive_agent_card(&config);
        assert_eq!(card.url, "http://0.0.0.0:9000");
    }

    #[test]
    fn test_agent_card_provider() {
        let config = A2aConfig::default();
        let card = build_hive_agent_card(&config);

        let provider = card.provider.as_ref().expect("provider should be set");
        assert_eq!(provider.organization, "AIrglow Studio");
        assert_eq!(provider.url, "https://hivecode.app");
    }

    #[test]
    fn test_agent_card_serialises_to_json() {
        let config = A2aConfig::default();
        let card = build_hive_agent_card(&config);

        let json = serde_json::to_string_pretty(&card).expect("serialisation failed");
        assert!(json.contains("\"name\": \"Hive\""));
        assert!(json.contains("\"streaming\": true"));
        assert!(json.contains("\"hivemind\""));
        assert!(json.contains("\"coordinator\""));
        assert!(json.contains("\"queen\""));
        assert!(json.contains("\"single\""));
    }

    #[test]
    fn test_supported_skills_matches_built_skills() {
        let config = A2aConfig::default();
        let card = build_hive_agent_card(&config);

        // Every skill in the card must be in SUPPORTED_SKILLS
        for skill in &card.skills {
            assert!(
                is_supported_skill(&skill.id),
                "card skill '{}' not in SUPPORTED_SKILLS",
                skill.id
            );
        }

        // Every entry in SUPPORTED_SKILLS must appear in the card
        for &id in &SUPPORTED_SKILLS {
            assert!(
                card.skills.iter().any(|s| s.id == id),
                "SUPPORTED_SKILLS entry '{}' missing from card",
                id
            );
        }
    }
}
