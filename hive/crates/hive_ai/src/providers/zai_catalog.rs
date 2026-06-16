//! Fetch and cache the live Z.AI model catalog.
//!
//! Z.AI exposes an OpenAI-compatible models endpoint at
//! `https://api.z.ai/api/paas/v4/models`. The catalog is cached for 5 minutes.

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
    data: Vec<ZaiModel>,
}

#[derive(Debug, Deserialize)]
struct ZaiModel {
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

    if id_lower.contains("glm-5.2") {
        (ModelTier::Premium, 1.4, 4.4)
    } else if id_lower.contains("flash") {
        (ModelTier::Free, 0.0, 0.0)
    } else if id_lower.contains("air") {
        (ModelTier::Budget, 0.2, 1.1)
    } else {
        // glm-4.6 and any other GLM model default to mid tier.
        (ModelTier::Mid, 0.6, 2.2)
    }
}

/// Derive a human-friendly display name from the raw model id.
fn display_name_from_id(id: &str) -> String {
    // Clean up common prefixes/patterns
    let name = id.replace("glm-", "GLM-").replace("-latest", " (Latest)");
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
        .join("-")
}

/// Determine capabilities based on model id.
fn model_capabilities(model_id: &str) -> ModelCapabilities {
    let id_lower = model_id.to_ascii_lowercase();

    let mut caps = vec![ModelCapability::ToolUse, ModelCapability::StructuredOutput];

    if id_lower.contains("glm-5.2") || id_lower.contains("glm-4.6") {
        caps.push(ModelCapability::LongContext);
    }

    ModelCapabilities::new(&caps)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch the Z.AI model catalog, returning cached results if fresh.
pub async fn fetch_zai_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
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
        .get("https://api.z.ai/api/paas/v4/models")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Z.AI API returned {}", resp.status()));
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
                provider: "zai".into(),
                provider_type: ProviderType::Zai,
                tier,
                context_window: m.max_context_length.unwrap_or(128_000),
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
    fn classify_glm_52_model() {
        let (tier, input, output) = classify_model("glm-5.2");
        assert_eq!(tier, ModelTier::Premium);
        assert!((input - 1.4).abs() < f64::EPSILON);
        assert!((output - 4.4).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_glm_air_model() {
        let (tier, input, output) = classify_model("glm-4.5-air");
        assert_eq!(tier, ModelTier::Budget);
        assert!((input - 0.2).abs() < f64::EPSILON);
        assert!((output - 1.1).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_glm_flash_model() {
        let (tier, input, output) = classify_model("glm-4.5-flash");
        assert_eq!(tier, ModelTier::Free);
        assert!((input - 0.0).abs() < f64::EPSILON);
        assert!((output - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_glm_46_model() {
        let (tier, input, output) = classify_model("glm-4.6");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 0.6).abs() < f64::EPSILON);
        assert!((output - 2.2).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_unknown_model() {
        let (tier, input, output) = classify_model("some-new-glm-model");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 0.6).abs() < f64::EPSILON);
        assert!((output - 2.2).abs() < f64::EPSILON);
    }

    #[test]
    fn display_name_glm() {
        let name = display_name_from_id("glm-4.6");
        assert!(name.contains("GLM") || name.contains("4.6"));
    }

    #[test]
    fn capabilities_glm_52() {
        let caps = model_capabilities("glm-5.2");
        assert!(caps.has(ModelCapability::ToolUse));
        assert!(caps.has(ModelCapability::StructuredOutput));
        assert!(caps.has(ModelCapability::LongContext));
    }

    #[test]
    fn capabilities_glm_flash() {
        let caps = model_capabilities("glm-4.5-flash");
        assert!(caps.has(ModelCapability::ToolUse));
        assert!(!caps.has(ModelCapability::LongContext));
    }

    #[test]
    fn invalidate_cache_clears() {
        {
            let mut cache = CACHE.lock();
            cache.models = vec![ModelInfo {
                id: "test".into(),
                name: "Test".into(),
                provider: "zai".into(),
                provider_type: ProviderType::Zai,
                tier: ModelTier::Mid,
                context_window: 128_000,
                input_price_per_mtok: 0.6,
                output_price_per_mtok: 2.2,
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
