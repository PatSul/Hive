use hive_ai::types::{ChatRequest, ChatResponse};

/// Async AI execution trait for the autoresearch engine.
///
/// This mirrors `hive_agents::AiExecutor` but lives in `hive_learn` to avoid
/// a circular dependency (`hive_agents` already depends on `hive_learn`).
/// Callers in `hive_agents` can provide a thin adapter that delegates to their
/// `AiExecutor` implementation.
pub trait AutoResearchExecutor: Send + Sync {
    /// Execute a chat request and return the response.
    fn execute(
        &self,
        request: &ChatRequest,
    ) -> impl std::future::Future<Output = Result<ChatResponse, String>> + Send;
}
