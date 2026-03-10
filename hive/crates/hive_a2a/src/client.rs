//! Agent Card discovery and caching for remote A2A agents.
//!
//! Provides the client-side discovery flow:
//! 1. Build the well-known Agent Card URL from a base URL
//! 2. Cache discovered cards with a configurable TTL
//! 3. Fetch and validate agent cards from remote endpoints

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use a2a_rs::AgentCard;

use crate::auth::validate_outbound_url;
use crate::error::A2aError;

// ---------------------------------------------------------------------------
// URL builder
// ---------------------------------------------------------------------------

/// Build the well-known Agent Card URL from a base URL.
///
/// Strips trailing slashes from `base_url` and appends
/// `/.well-known/agent-card.json`.
pub fn agent_card_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    format!("{trimmed}/.well-known/agent-card.json")
}

// ---------------------------------------------------------------------------
// Discovery cache
// ---------------------------------------------------------------------------

/// A thread-safe cache for discovered Agent Cards with TTL-based expiry.
pub struct DiscoveryCache {
    entries: Mutex<HashMap<String, (AgentCard, Instant)>>,
    ttl: Duration,
}

impl DiscoveryCache {
    /// Create a new cache with the given time-to-live for entries.
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Retrieve a cached agent card if it exists and has not expired.
    ///
    /// Returns `None` if the entry is missing or has exceeded the TTL.
    pub fn get(&self, url: &str) -> Option<AgentCard> {
        let entries = self.entries.lock().ok()?;
        let (card, inserted_at) = entries.get(url)?;
        if inserted_at.elapsed() < self.ttl {
            Some(card.clone())
        } else {
            None
        }
    }

    /// Insert (or overwrite) a cached agent card.
    pub fn insert(&self, url: &str, card: AgentCard) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(url.to_string(), (card, Instant::now()));
        }
    }

    /// Remove a cached entry, forcing re-discovery on the next lookup.
    pub fn invalidate(&self, url: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(url);
        }
    }
}

// ---------------------------------------------------------------------------
// Discovery function
// ---------------------------------------------------------------------------

/// Discover a remote agent by fetching its Agent Card.
///
/// Flow:
/// 1. Check the cache for a valid (non-expired) entry
/// 2. Validate the outbound URL (SSRF / scheme checks)
/// 3. HTTP GET the well-known agent card endpoint
/// 4. Parse the response as an [`AgentCard`]
/// 5. Cache the result and return it
pub async fn discover_agent(base_url: &str, cache: &DiscoveryCache) -> Result<AgentCard, A2aError> {
    // 1. Check cache
    if let Some(card) = cache.get(base_url) {
        return Ok(card);
    }

    // 2. Build and validate URL
    let card_url = agent_card_url(base_url);
    validate_outbound_url(&card_url)?;

    // 3. Fetch
    let response = reqwest::get(&card_url).await.map_err(|e| {
        A2aError::Network(format!("Failed to fetch agent card from {card_url}: {e}"))
    })?;

    if !response.status().is_success() {
        return Err(A2aError::Network(format!(
            "Agent card request to {card_url} returned status {}",
            response.status()
        )));
    }

    // 4. Parse
    let card: AgentCard = response.json().await.map_err(|e| {
        A2aError::Network(format!("Failed to parse agent card from {card_url}: {e}"))
    })?;

    // 5. Cache and return
    cache.insert(base_url, card.clone());
    Ok(card)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_url() {
        assert_eq!(
            agent_card_url("https://agent.example.com"),
            "https://agent.example.com/.well-known/agent-card.json"
        );
    }

    #[test]
    fn test_agent_card_url_trailing_slash() {
        assert_eq!(
            agent_card_url("https://agent.example.com/"),
            "https://agent.example.com/.well-known/agent-card.json"
        );
    }

    #[test]
    fn test_agent_card_url_multiple_trailing_slashes() {
        assert_eq!(
            agent_card_url("https://agent.example.com///"),
            "https://agent.example.com/.well-known/agent-card.json"
        );
    }

    #[test]
    fn test_discovery_cache_miss() {
        let cache = DiscoveryCache::new(Duration::from_secs(300));
        assert!(cache.get("https://example.com").is_none());
    }

    #[test]
    fn test_discovery_cache_hit_and_invalidate() {
        let cache = DiscoveryCache::new(Duration::from_secs(300));
        let card = AgentCard {
            name: "Test Agent".into(),
            description: "A test agent".into(),
            url: "https://example.com".into(),
            provider: None,
            version: "1.0.0".into(),
            documentation_url: None,
            capabilities: a2a_rs::AgentCapabilities::default(),
            security_schemes: None,
            security: None,
            default_input_modes: vec!["text".into()],
            default_output_modes: vec!["text".into()],
            skills: vec![],
            supports_authenticated_extended_card: None,
        };

        cache.insert("https://example.com", card.clone());

        // Should hit
        let cached = cache.get("https://example.com");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().name, "Test Agent");

        // Invalidate
        cache.invalidate("https://example.com");
        assert!(cache.get("https://example.com").is_none());
    }

    #[test]
    fn test_discovery_cache_expiry() {
        // Zero TTL means everything expires immediately
        let cache = DiscoveryCache::new(Duration::from_secs(0));
        let card = AgentCard {
            name: "Ephemeral".into(),
            description: "Gone in a blink".into(),
            url: "https://ephemeral.example.com".into(),
            provider: None,
            version: "0.0.1".into(),
            documentation_url: None,
            capabilities: a2a_rs::AgentCapabilities::default(),
            security_schemes: None,
            security: None,
            default_input_modes: vec!["text".into()],
            default_output_modes: vec!["text".into()],
            skills: vec![],
            supports_authenticated_extended_card: None,
        };

        cache.insert("https://ephemeral.example.com", card);

        // With 0-second TTL the entry is already expired
        assert!(cache.get("https://ephemeral.example.com").is_none());
    }
}
