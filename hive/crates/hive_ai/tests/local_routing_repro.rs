//! Reproduction: local-only Ollama setup where the configured default model
//! (`llama3.2`) is NOT installed, but another model (`tinyllama`) IS.
//!
//! Mirrors how `hive_app` builds the `AiService` for a privacy-mode, local-only
//! user (see `hive_app/src/main.rs`). It prints exactly what routing resolves
//! and whether a real stream succeeds, so we can see whether Hive gracefully
//! uses a *detected* model instead of dying on the configured-but-absent one.
//!
//! Ignored by default — it requires a running Ollama on 127.0.0.1:11434.
//! Run with:
//!   cargo test -p hive_ai --test local_routing_repro -- --ignored --nocapture

use hive_ai::types::{ChatMessage, MessageRole};
use hive_ai::{AiService, AiServiceConfig};

fn user_like_config() -> AiServiceConfig {
    AiServiceConfig {
        privacy_mode: true,
        ollama_url: "http://127.0.0.1:11434".into(),
        lmstudio_url: "http://localhost:1234".into(),
        default_model: "llama3.2".into(), // NOT installed locally
        auto_routing: true,
        ..Default::default()
    }
}

async fn try_route(svc: &AiService, label: &str, model: &str) {
    let msgs = vec![ChatMessage::text(
        MessageRole::User,
        "Reply with exactly: PONG",
    )];
    match svc.prepare_stream(msgs, model, None, None) {
        None => {
            eprintln!("[{label}] prepare_stream -> None  (UI would show 'No AI providers configured')");
        }
        Some((provider, request)) => {
            eprintln!(
                "[{label}] routed -> provider={} model={}",
                provider.name(),
                request.model
            );
            match provider.stream_chat(&request).await {
                Err(e) => eprintln!("[{label}] stream_chat ERR: {e}"),
                Ok(mut rx) => {
                    let mut out = String::new();
                    while let Some(chunk) = rx.recv().await {
                        out.push_str(&chunk.content);
                        if chunk.done {
                            break;
                        }
                    }
                    eprintln!("[{label}] stream_chat OK, reply={:?}", out.trim());
                }
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires a running Ollama on 127.0.0.1:11434"]
async fn local_only_missing_default_model_repro() {
    let mut svc = AiService::new(user_like_config());
    let discovery = svc.start_discovery();
    discovery.scan_all().await;

    let detected: Vec<String> = svc
        .all_available_models()
        .into_iter()
        .map(|m| m.id)
        .collect();
    eprintln!("detected models: {detected:?}");

    // The configured default model, which is NOT installed in Ollama.
    try_route(&svc, "default(llama3.2)", "llama3.2").await;
    // Explicit auto-routing.
    try_route(&svc, "auto", "auto").await;
}
