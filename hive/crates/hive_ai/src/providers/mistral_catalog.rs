//! Fetch and cache the live Mistral model catalog.
//!
//! Mistral exposes an OpenAI-compatible models endpoint at
//! `https://api.mistral.ai/v1/models`. The catalog is cached for 5 minutes.

use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::types::{ModelCapabilities, ModelCapability, ModelInfo, ModelTier, ProviderType};

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
    data: Vec<MistralModel>,
}

#[derive(Debug, Deserialize)]
struct MistralModel {
    id: String,
    #[serde(default)]
    max_context_length: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    owned_by: Option<String>,
}

// ---------------------------------------------------------------------------
// Pricing / tier helpers
// ---------------------------------------------------------------------------

/// Assign a pricing tier and per-million-token prices based on the model id.
fn classify_model(model_id: &str) -> (ModelTier, f64, f64) {
    let id_lower = model_id.to_ascii_lowercase();

    if id_lower.contains("codestral") {
        (ModelTier::Mid, 0.3, 0.9)
    } else if id_lower.contains("large") {
        (ModelTier::Mid, 2.0, 6.0)
    } else if id_lower.contains("medium") {
        (ModelTier::Mid, 2.7, 8.1)
    } else if id_lower.contains("small") {
        (ModelTier::Budget, 0.1, 0.3)
    } else if id_lower.contains("pixtral") {
        (ModelTier::Mid, 2.0, 6.0)
    } else {
        (ModelTier::Mid, 1.0, 3.0)
    }
}

/// Derive a human-friendly display name from the raw model id.
fn display_name_from_id(id: &str) -> String {
    // Clean up common prefixes/patterns
    let name = id
        .replace("mistral-", "Mistral ")
        .replace("codestral-", "Codestral ")
        .replace("pixtral-", "Pixtral ")
        .replace("-latest", " (Latest)");
    // Capitalize first letter of each word
    name.split('-')
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

/// Determine capabilities based on model id.
fn model_capabilities(model_id: &str) -> ModelCapabilities {
    let id_lower = model_id.to_ascii_lowercase();

    let mut caps = vec![ModelCapability::ToolUse, ModelCapability::StructuredOutput];

    if id_lower.contains("large") || id_lower.contains("pixtral") {
        caps.push(ModelCapability::Vision);
    }

    if id_lower.contains("large") || id_lower.contains("codestral") {
        caps.push(ModelCapability::LongContext);
    }

    ModelCapabilities::new(&caps)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch the Mistral model catalog, returning cached results if fresh.
pub async fn fetch_mistral_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
    // Check cache first
    {
        let cache = CACHE.lock();
        if let Some(fetched_at) = cache.fetched_at
            && fetched_at.elapsed() < CACHE_TTL
            && !cache.models.is_empty()
        {
            return Ok(cache.models.clone());
        }
    }

    // Fetch from API
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.mistral.ai/v1/models")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Mistral API returned {}", resp.status()));
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
            let capabilities = model_capabilities(&m.id);

            let mut info = ModelInfo {
                id: m.id,
                name,
                provider: "mistral".into(),
                provider_type: ProviderType::Mistral,
                tier,
                context_window: m.max_context_length.unwrap_or(32_000),
                input_price_per_mtok: input_price,
                output_price_per_mtok: output_price,
                capabilities,
                release_date: None,
            };

            // Enrich from static registry if available
            crate::model_registry::enrich_from_registry(&mut info);
            info
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
    fn classify_large_model() {
        let (tier, input, output) = classify_model("mistral-large-latest");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 2.0).abs() < f64::EPSILON);
        assert!((output - 6.0).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_small_model() {
        let (tier, input, output) = classify_model("mistral-small-latest");
        assert_eq!(tier, ModelTier::Budget);
        assert!((input - 0.1).abs() < f64::EPSILON);
        assert!((output - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_codestral_model() {
        let (tier, input, output) = classify_model("codestral-latest");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 0.3).abs() < f64::EPSILON);
        assert!((output - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_unknown_model() {
        let (tier, input, output) = classify_model("some-new-mistral-model");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 1.0).abs() < f64::EPSILON);
        assert!((output - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn display_name_large() {
        let name = display_name_from_id("mistral-large-latest");
        assert!(name.contains("Mistral") || name.contains("Large"));
    }

    #[test]
    fn capabilities_large() {
        let caps = model_capabilities("mistral-large-latest");
        assert!(caps.has(ModelCapability::ToolUse));
        assert!(caps.has(ModelCapability::Vision));
        assert!(caps.has(ModelCapability::LongContext));
    }

    #[test]
    fn capabilities_small() {
        let caps = model_capabilities("mistral-small-latest");
        assert!(caps.has(ModelCapability::ToolUse));
        assert!(!caps.has(ModelCapability::Vision));
    }

    #[test]
    fn invalidate_cache_clears() {
        {
            let mut cache = CACHE.lock();
            cache.models = vec![ModelInfo {
                id: "test".into(),
                name: "Test".into(),
                provider: "mistral".into(),
                provider_type: ProviderType::Mistral,
                tier: ModelTier::Mid,
                context_window: 32_000,
                input_price_per_mtok: 1.0,
                output_price_per_mtok: 3.0,
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
