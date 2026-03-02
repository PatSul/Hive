use crate::types::{ModelCapabilities, ModelCapability, ModelInfo, ModelTier, ProviderType};

pub(crate) fn builtin_models() -> Vec<ModelInfo> {
    let mut models = Vec::new();

    let mut push_model = |id: &str, name: &str, tier: ModelTier, ctx: u32, in_price: f64, out_price: f64, caps: &[ModelCapability]| {
        models.push(ModelInfo {
            id: id.to_string(),
            name: name.to_string(),
            provider: "venice".to_string(),
            provider_type: ProviderType::Venice,
            tier,
            context_window: ctx,
            input_price_per_mtok: in_price,
            output_price_per_mtok: out_price,
            capabilities: ModelCapabilities::new(caps),
            release_date: None,
        });
    };

    // Note: Venice API is compatible with OpenAI but has different model names. 
    // We'll populate some known ones here.

    push_model(
        "llama-3.3-70b",
        "Llama 3.3 70B",
        ModelTier::Premium,
        131072,
        0.0, // Venice doesn't strictly charge by token in the same way, but keeping structure
        0.0,
        &[ModelCapability::ToolUse],
    );

    push_model(
        "qwen32b",
        "Qwen 2.5 32B Config",
        ModelTier::Mid,
        32768,
        0.0,
        0.0,
        &[ModelCapability::ToolUse],
    );
    
    push_model(
        "deepseek-r1-llama-70b",
        "DeepSeek R1 (Llama 70B)",
        ModelTier::Premium,
        131072,
        0.0,
        0.0,
        &[ModelCapability::ToolUse, ModelCapability::ExtendedThinking],
    );

    models
}
