//! Fetch and cache the live Doubao / BytePlus model catalog.
//!
//! BytePlus (Volcano Engine) exposes an OpenAI-compatible models endpoint at
//! `https://ark.ap-southeast.bytepluses.com/api/v3/models`. The catalog is
//! cached for 5 minutes.

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
    data: Vec<DoubaoModel>,
}

#[derive(Debug, Deserialize)]
struct DoubaoModel {
    id: String,
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

    if id_lower.contains("pro") {
        // Doubao Pro — flagship model
        (ModelTier::Mid, 0.8, 2.0)
    } else if id_lower.contains("lite") {
        // Doubao Lite — budget model
        (ModelTier::Budget, 0.3, 0.6)
    } else {
        // Unknown tier — default to mid
        (ModelTier::Mid, 0.5, 1.5)
    }
}

/// Derive a human-friendly display name from the raw model id.
fn display_name_from_id(id: &str) -> String {
    let name = id
        .replace("doubao-", "Doubao ")
        .replace("-256k", " 256K")
        .replace("-128k", " 128K")
        .replace("-32k", " 32K");
    // Capitalize first letter of remaining parts.
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

    if id_lower.contains("pro") {
        caps.push(ModelCapability::LongContext);
    }

    ModelCapabilities::new(&caps)
}

/// Infer context window from model id suffix.
fn infer_context_window(model_id: &str) -> u32 {
    let id_lower = model_id.to_ascii_lowercase();
    if id_lower.contains("256k") {
        256_000
    } else if id_lower.contains("128k") {
        128_000
    } else if id_lower.contains("32k") {
        32_000
    } else {
        128_000 // default
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch the Doubao model catalog, returning cached results if fresh.
pub async fn fetch_doubao_models(api_key: &str) -> Result<Vec<ModelInfo>, String> {
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
        .get("https://ark.ap-southeast.bytepluses.com/api/v3/models")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Doubao API returned {}", resp.status()));
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
            let context_window = infer_context_window(&m.id);

            let mut info = ModelInfo {
                id: m.id,
                name,
                provider: "doubao".into(),
                provider_type: ProviderType::Doubao,
                tier,
                context_window,
                input_price_per_mtok: input_price,
                output_price_per_mtok: output_price,
                capabilities,
                release_date: None,
            };

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
    fn classify_pro_model() {
        let (tier, input, output) = classify_model("doubao-pro-256k");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 0.8).abs() < f64::EPSILON);
        assert!((output - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_lite_model() {
        let (tier, input, output) = classify_model("doubao-lite-128k");
        assert_eq!(tier, ModelTier::Budget);
        assert!((input - 0.3).abs() < f64::EPSILON);
        assert!((output - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn classify_unknown_model() {
        let (tier, input, output) = classify_model("doubao-new-model");
        assert_eq!(tier, ModelTier::Mid);
        assert!((input - 0.5).abs() < f64::EPSILON);
        assert!((output - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn display_name_pro() {
        let name = display_name_from_id("doubao-pro-256k");
        assert!(name.contains("Doubao") || name.contains("Pro"));
    }

    #[test]
    fn display_name_lite() {
        let name = display_name_from_id("doubao-lite-128k");
        assert!(name.contains("Doubao") || name.contains("Lite"));
    }

    #[test]
    fn capabilities_pro() {
        let caps = model_capabilities("doubao-pro-256k");
        assert!(caps.has(ModelCapability::ToolUse));
        assert!(caps.has(ModelCapability::LongContext));
    }

    #[test]
    fn capabilities_lite() {
        let caps = model_capabilities("doubao-lite-128k");
        assert!(caps.has(ModelCapability::ToolUse));
        assert!(!caps.has(ModelCapability::LongContext));
    }

    #[test]
    fn infer_context_256k() {
        assert_eq!(infer_context_window("doubao-pro-256k"), 256_000);
    }

    #[test]
    fn infer_context_128k() {
        assert_eq!(infer_context_window("doubao-lite-128k"), 128_000);
    }

    #[test]
    fn infer_context_default() {
        assert_eq!(infer_context_window("doubao-unknown"), 128_000);
    }

    #[test]
    fn invalidate_cache_clears() {
        {
            let mut cache = CACHE.lock();
            cache.models = vec![ModelInfo {
                id: "test".into(),
                name: "Test".into(),
                provider: "doubao".into(),
                provider_type: ProviderType::Doubao,
                tier: ModelTier::Mid,
                context_window: 128_000,
                input_price_per_mtok: 0.8,
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
