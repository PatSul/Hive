//! Fetch and cache the live xAI model catalog.
//!
//! xAI exposes an OpenAI-compatible models endpoint at `https://api.x.ai/v1/models`.
//! The catalog is cached for 5 minutes to avoid excessive API calls.

use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::types::{ModelInfo, ModelTier, ProviderType};

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

const CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

struct CatalogCache {
    models: Vec<ModelInfo>,
    fetched_at: Option<Instant>,
}

static CACHE: Mutex<CatalogCache> = Mutex::new(CatalogCache {
    models: Vec::new(),
    fetched_at: None,
});

/// Clear the cached catalog (e.g. when the API key changes).
pub fn invalidate_cache() {
    let mut cache = CACHE.lock();
    cache.models.clear();
    cache.fetched_at = None;
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<XaiModel>,
}

#[derive(Debug, Deserialize)]
struct XaiModel {
    id: String,
    #[serde(default)]
    context_window: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    owned_by: Option<String>,
}

// ---------------------------------------------------------------------------
// Pricing / tier helpers
// ---------------------------------------------------------------------------

/// Assign a pricing tier and per-million-token prices based on the model id.
///
/// xAI pricing (as of 2025): Grok-3 is premium-tier, Grok-3 Mini is mid-tier,
/// Grok-2 variants are mid/budget.
fn classify_model(model_id: &str) -> (ModelTier, f64, f64) {
    let id_lower = model_id.to_ascii_lowercase();

    if id_lower.contains("grok-3-mini") {
        (ModelTier::Mid, 0.30, 0.50)
    } else if id_lower.contains("grok-3") {
        (ModelTier::Premium, 3.00, 15.00)
    } else if id_lower.contains("grok-2") {
        (ModelTier::Mid, 2.00, 10.00)
    } else {
        (ModelTier::Mid, 1.00, 2.00)
    }
}

/// Derive a human-friendly display name from the raw model id.
fn display_name_from_id(id: &str) -> String {
    id.split('-')
        .map(|part| {
            if part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                part.to_string()
            } else {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        let mut s = first.to_uppercase().to_string();
                        s.extend(chars);
                        s
                    }
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch the xAI model catalog, returning cached results if fresh.
pub async fn fetch_xai_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    // Check cache first
    {
        let cache = CACHE.lock();
        if let Some(fetched_at) = cache.fetched_at
            && fetched_at.elapsed() < CACHE_TTL && !cache.models.is_empty() {
                return Ok(cache.models.clone());
            }
    }

    // Fetch from API
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.x.ai/v1/models")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("xAI API returned {}", resp.status()));
    }

    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {e}"))?;

    let models: Vec<ModelInfo> = body
        .data
        .into_iter()
        .map(|m| {
            let (tier, input_price, output_price) = classify_model(&m.id);
            let name = display_name_from_id(&m.id);

            ModelInfo {
                id: m.id,
                name,
                provider: "xai".into(),
                provider_type: ProviderType::XAI,
                tier,
                context_window: m.context_window.unwrap_or(131072),
                input_price_per_mtok: input_price,
                output_price_per_mtok: output_price,
                capabilities: Default::default(),
                release_date: None,
            }
        })
        .collect();

    // Update cache
    {
        let mut cache = CACHE.lock();
        cache.models = models.clone();
        cache.fetched_at = Some(Instant::now());
    }

    Ok(models)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_grok3_model() {
        let (tier, input, output) = classify_model("grok-3");
        assert_eq!(tier, ModelTier::Premium);
        assert!((input - 3.0).abs() < f64::EPSILON);
        assert!((output - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_grok3_mini_model() {
        let (tier, input, output) = classify_model("grok-3-mini");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 0.30).abs() < f64::EPSILON);
        assert!((output - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_grok2_model() {
        let (tier, input, output) = classify_model("grok-2-1212");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 2.0).abs() < f64::EPSILON);
        assert!((output - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_unknown_model() {
        let (tier, input, output) = classify_model("some-new-xai-model");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 1.0).abs() < f64::EPSILON);
        assert!((output - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn display_name_grok3() {
        assert_eq!(display_name_from_id("grok-3"), "Grok 3");
    }

    #[test]
    fn display_name_grok3_mini() {
        assert_eq!(display_name_from_id("grok-3-mini"), "Grok 3 Mini");
    }

    #[test]
    fn invalidate_cache_clears() {
        {
            let mut cache = CACHE.lock();
            cache.models = vec![ModelInfo {
                id: "test".into(),
                name: "Test".into(),
                provider: "xai".into(),
                provider_type: ProviderType::XAI,
                tier: ModelTier::Mid,
                context_window: 131072,
                input_price_per_mtok: 1.0,
                output_price_per_mtok: 2.0,
                capabilities: Default::default(),
                release_date: None,
            }];
            cache.fetched_at = Some(Instant::now());
        }

        invalidate_cache();

        let cache = CACHE.lock();
        assert!(cache.models.is_empty());
        assert!(cache.fetched_at.is_none());
    }
}
