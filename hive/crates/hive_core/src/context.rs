//! Context window management — tracks token usage, prunes messages
//! to stay within model-specific context limits, and proactively compacts
//! conversations to prevent dead sessions.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Rough token estimate: ~4 characters per token for English text.
/// This is intentionally conservative (overestimates) to avoid truncation.
const CHARS_PER_TOKEN: usize = 4;

/// Default proactive compaction threshold (80% of context budget).
const DEFAULT_COMPACTION_THRESHOLD: f64 = 0.80;
const PROACTIVE_COMPACTION_RATIO: f64 = 0.50;

/// Aggressive compaction threshold used in reactive (post-overflow) path.
const REACTIVE_COMPACTION_RATIO: f64 = 0.70;

/// Estimate token count for a string.
pub fn estimate_tokens(text: &str) -> usize {
    // Use character count / 4 as a rough approximation.
    // More accurate would be tiktoken, but this avoids a heavy dependency.
    text.len().div_ceil(CHARS_PER_TOKEN)
}

// ---------------------------------------------------------------------------
// Context window
// ---------------------------------------------------------------------------

/// A message in the context window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    pub tokens: usize,
    pub pinned: bool,
    /// Whether this message is a compaction summary replacing older messages.
    #[serde(default)]
    pub is_compacted: bool,
    /// How many original messages this compaction summary replaced.
    #[serde(default)]
    pub original_count: Option<u32>,
}

impl ContextMessage {
    /// Creates a new context message with an automatically estimated token count.
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let tokens = estimate_tokens(&content);
        Self {
            role: role.into(),
            content,
            tokens,
            pinned: false,
            is_compacted: false,
            original_count: None,
        }
    }

    /// Marks this message as pinned so it survives context pruning.
    pub fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }
}

/// Result of a context compaction operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Number of messages that were compacted into a summary.
    pub messages_compacted: usize,
    /// Tokens freed by compaction.
    pub tokens_freed: usize,
    /// Tokens used by the summary message.
    pub summary_tokens: usize,
    /// Context usage percentage after compaction.
    pub usage_after: f64,
}

/// Manages the context window for a conversation.
///
/// Keeps track of messages and their estimated token counts,
/// pruning oldest non-pinned messages when the limit is reached.
/// Supports proactive compaction (at configurable threshold) and
/// reactive compaction (after context overflow).
pub struct ContextWindow {
    messages: Vec<ContextMessage>,
    max_tokens: usize,
    system_prompt_tokens: usize,
    /// Proactive compaction threshold (0.0 to 1.0). When `usage_pct()`
    /// exceeds this value, `needs_compaction()` returns true.
    compaction_threshold: f64,
}

impl ContextWindow {
    /// Creates a new context window with the given maximum token budget.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_tokens,
            system_prompt_tokens: 0,
            compaction_threshold: DEFAULT_COMPACTION_THRESHOLD,
        }
    }

    /// Get the maximum token budget.
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// Set the proactive compaction threshold (0.0 to 1.0).
    ///
    /// When `usage_pct()` exceeds this value, `needs_compaction()` returns true.
    /// Default is 0.80 (80%).
    pub fn set_compaction_threshold(&mut self, threshold: f64) {
        self.compaction_threshold = threshold.clamp(0.0, 1.0);
    }

    /// Get the current compaction threshold.
    pub fn compaction_threshold(&self) -> f64 {
        self.compaction_threshold
    }

    /// Set the system prompt (counts toward the token budget).
    pub fn set_system_prompt(&mut self, prompt: &str) {
        self.system_prompt_tokens = estimate_tokens(prompt);
    }

    /// Add a message to the context. May trigger pruning.
    pub fn push(&mut self, message: ContextMessage) {
        self.messages.push(message);
        self.prune();
    }

    /// Total estimated tokens across all messages + system prompt.
    pub fn total_tokens(&self) -> usize {
        self.system_prompt_tokens + self.messages.iter().map(|m| m.tokens).sum::<usize>()
    }

    /// Available tokens remaining.
    pub fn available_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.total_tokens())
    }

    /// Number of messages in the window.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get all messages in order.
    pub fn messages(&self) -> &[ContextMessage] {
        &self.messages
    }

    /// Usage as a percentage (0.0 to 1.0+).
    pub fn usage_pct(&self) -> f64 {
        if self.max_tokens == 0 {
            return 1.0;
        }
        self.total_tokens() as f64 / self.max_tokens as f64
    }

    /// Whether the context is over budget.
    pub fn is_over_budget(&self) -> bool {
        self.total_tokens() > self.max_tokens
    }

    // -----------------------------------------------------------------------
    // Compaction
    // -----------------------------------------------------------------------

    /// Whether the context window has grown enough to trigger proactive
    /// compaction. Returns true when usage exceeds the compaction threshold
    /// AND there are enough non-pinned messages to compact (at least 3).
    pub fn needs_compaction(&self) -> bool {
        if self.max_tokens == 0 {
            return false;
        }
        let compactable = self
            .messages
            .iter()
            .filter(|m| !m.pinned && !m.is_compacted)
            .count();
        self.usage_pct() >= self.compaction_threshold && compactable >= 3
    }

    /// Select the oldest non-pinned, non-compacted messages for compaction.
    ///
    /// Selects up to `ratio` (0.0-1.0) of total message tokens, ensuring
    /// we don't compact pinned or already-compacted messages.
    fn select_for_compaction(&self, ratio: f64) -> Vec<usize> {
        let message_tokens: usize = self.messages.iter().map(|m| m.tokens).sum();
        let target_tokens = (message_tokens as f64 * ratio) as usize;

        let mut selected = Vec::new();
        let mut accumulated = 0usize;

        for (idx, msg) in self.messages.iter().enumerate() {
            if accumulated >= target_tokens {
                break;
            }
            if msg.pinned || msg.is_compacted {
                continue;
            }
            accumulated += msg.tokens;
            selected.push(idx);
        }

        selected
    }

    fn selected_messages_for_compaction(&self, ratio: f64) -> Vec<ContextMessage> {
        self.select_for_compaction(ratio)
            .into_iter()
            .map(|idx| self.messages[idx].clone())
            .collect()
    }

    fn find_message_indices(&self, messages: &[ContextMessage]) -> Result<Vec<usize>, String> {
        if messages.is_empty() {
            return Err("No compactable messages found".into());
        }

        let mut indices = Vec::with_capacity(messages.len());
        let mut search_start = 0usize;

        for expected in messages {
            let Some((idx, _)) = self
                .messages
                .iter()
                .enumerate()
                .skip(search_start)
                .find(|(_, actual)| *actual == expected)
            else {
                return Err("Compaction candidates no longer match the current context".into());
            };
            indices.push(idx);
            search_start = idx + 1;
        }

        Ok(indices)
    }

    fn apply_compaction_summary_internal(
        &mut self,
        indices: &[usize],
        summary_text: String,
    ) -> Result<CompactionResult, String> {
        if indices.is_empty() {
            return Err("No compactable messages found".into());
        }

        let to_compact: Vec<ContextMessage> =
            indices.iter().map(|&i| self.messages[i].clone()).collect();
        let tokens_before: usize = to_compact.iter().map(|m| m.tokens).sum();
        let count = to_compact.len() as u32;
        let summary_tokens = estimate_tokens(&summary_text);
        let insert_at = indices[0];

        for &idx in indices.iter().rev() {
            self.messages.remove(idx);
        }

        let summary_msg = ContextMessage {
            role: "system".into(),
            content: summary_text,
            tokens: summary_tokens,
            pinned: true,
            is_compacted: true,
            original_count: Some(count),
        };
        self.messages.insert(insert_at, summary_msg);

        let tokens_freed = tokens_before.saturating_sub(summary_tokens);

        Ok(CompactionResult {
            messages_compacted: count as usize,
            tokens_freed,
            summary_tokens,
            usage_after: self.usage_pct(),
        })
    }

    pub fn compaction_candidates(&self) -> Vec<ContextMessage> {
        if !self.needs_compaction() {
            return Vec::new();
        }

        self.selected_messages_for_compaction(PROACTIVE_COMPACTION_RATIO)
    }

    pub fn apply_compaction_summary(
        &mut self,
        messages: &[ContextMessage],
        summary_text: String,
    ) -> Result<CompactionResult, String> {
        let indices = self.find_message_indices(messages)?;
        self.apply_compaction_summary_internal(&indices, summary_text)
    }

    /// Proactively compact the context window by summarizing older messages.
    ///
    /// The `summarize` callback receives the messages to be compacted and
    /// should return a summary string (typically by calling a budget AI model).
    /// Returns `None` if compaction isn't needed.
    ///
    /// Compaction selects the oldest ~50% of non-pinned messages by token
    /// count, replaces them with a single pinned summary message, and
    /// preserves the most recent messages intact.
    pub fn compact<F>(&mut self, summarize: F) -> Option<Result<CompactionResult, String>>
    where
        F: FnOnce(&[ContextMessage]) -> Result<String, String>,
    {
        if !self.needs_compaction() {
            return None;
        }

        Some(self.do_compact(PROACTIVE_COMPACTION_RATIO, summarize))
    }

    /// Compact unconditionally with a custom ratio. Used for both proactive
    /// and reactive paths.
    fn do_compact<F>(&mut self, ratio: f64, summarize: F) -> Result<CompactionResult, String>
    where
        F: FnOnce(&[ContextMessage]) -> Result<String, String>,
    {
        let to_compact = self.selected_messages_for_compaction(ratio);
        if to_compact.is_empty() {
            return Err("No compactable messages found".into());
        }

        // Call the summarizer.
        let summary_text = summarize(&to_compact)?;
        self.apply_compaction_summary(&to_compact, summary_text)
    }

    /// Reactive compaction — called after a context-length API error.
    ///
    /// Uses a more aggressive ratio (70% of messages) to free substantial
    /// space. Always runs regardless of threshold (the error proves we need it).
    pub fn compact_reactive<F>(&mut self, summarize: F) -> Result<CompactionResult, String>
    where
        F: FnOnce(&[ContextMessage]) -> Result<String, String>,
    {
        self.do_compact(REACTIVE_COMPACTION_RATIO, summarize)
    }

    /// Convenience: check and compact if needed using the provided summarizer.
    /// Returns `Ok(Some(result))` if compaction happened, `Ok(None)` if not needed,
    /// or `Err` if the summarizer failed.
    pub fn compact_if_needed<F>(&mut self, summarize: F) -> Result<Option<CompactionResult>, String>
    where
        F: FnOnce(&[ContextMessage]) -> Result<String, String>,
    {
        match self.compact(summarize) {
            None => Ok(None),
            Some(Ok(result)) => Ok(Some(result)),
            Some(Err(e)) => Err(e),
        }
    }

    // -----------------------------------------------------------------------
    // Pruning (existing)
    // -----------------------------------------------------------------------

    /// Prune oldest non-pinned messages until within budget.
    /// Uses a single-pass `retain()` instead of repeated `Vec::remove()` to
    /// avoid O(n^2) shifting.
    fn prune(&mut self) {
        let total = self.total_tokens();
        if total <= self.max_tokens {
            return;
        }
        let mut budget = total - self.max_tokens; // tokens we need to shed
        self.messages.retain(|m| {
            if budget == 0 || m.pinned {
                return true;
            }
            budget = budget.saturating_sub(m.tokens);
            false
        });
    }

    /// Clear all messages.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Summarize context state for debugging.
    pub fn summary(&self) -> ContextSummary {
        ContextSummary {
            message_count: self.messages.len(),
            total_tokens: self.total_tokens(),
            max_tokens: self.max_tokens,
            available_tokens: self.available_tokens(),
            usage_pct: self.usage_pct(),
            pinned_count: self.messages.iter().filter(|m| m.pinned).count(),
        }
    }
}

/// Summary of context window state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub message_count: usize,
    pub total_tokens: usize,
    pub max_tokens: usize,
    pub available_tokens: usize,
    pub usage_pct: f64,
    pub pinned_count: usize,
}

// ---------------------------------------------------------------------------
// Common model context sizes
// ---------------------------------------------------------------------------

/// Get the default context window size for a model.
pub fn model_context_size(model_id: &str) -> usize {
    match model_id {
        // Anthropic
        "claude-opus-4" | "claude-sonnet-4" => 200_000,
        "claude-haiku-3.5" => 200_000,
        // OpenAI
        "gpt-4o" | "gpt-4o-mini" => 128_000,
        "o1" | "o1-mini" => 128_000,
        // Local / small
        "llama3.2" | "llama3.2:latest" => 128_000,
        "mistral" | "mistral:latest" => 32_000,
        "codellama" | "codellama:latest" => 16_000,
        // Default
        _ => 8_000,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_basic() {
        // "hello" = 5 chars → ceil(5/4) = 2 tokens
        assert_eq!(estimate_tokens("hello"), 2);
        // Empty string = 0
        assert_eq!(estimate_tokens(""), 0);
        // 100 chars → 25 tokens
        let s = "a".repeat(100);
        assert_eq!(estimate_tokens(&s), 25);
    }

    #[test]
    fn context_message_creation() {
        let msg = ContextMessage::new("user", "Hello, how are you?");
        assert_eq!(msg.role, "user");
        assert!(!msg.pinned);
        assert!(!msg.is_compacted);
        assert!(msg.original_count.is_none());
        assert!(msg.tokens > 0);
    }

    #[test]
    fn context_message_pinned() {
        let msg = ContextMessage::new("system", "Important").pinned();
        assert!(msg.pinned);
    }

    #[test]
    fn context_window_basic() {
        let mut ctx = ContextWindow::new(1000);
        assert_eq!(ctx.message_count(), 0);
        assert_eq!(ctx.total_tokens(), 0);
        assert_eq!(ctx.available_tokens(), 1000);

        ctx.push(ContextMessage::new("user", "Hello"));
        assert_eq!(ctx.message_count(), 1);
        assert!(ctx.total_tokens() > 0);
    }

    #[test]
    fn context_window_with_system_prompt() {
        let mut ctx = ContextWindow::new(100);
        ctx.set_system_prompt("You are a helpful assistant.");
        let base_tokens = ctx.total_tokens();
        assert!(base_tokens > 0);

        ctx.push(ContextMessage::new("user", "Hi"));
        assert!(ctx.total_tokens() > base_tokens);
    }

    #[test]
    fn context_window_pruning() {
        // Small context window — 50 tokens
        let mut ctx = ContextWindow::new(50);

        // Add messages that together exceed 50 tokens
        ctx.push(ContextMessage::new("user", "a".repeat(100))); // ~25 tokens
        assert_eq!(ctx.message_count(), 1);

        ctx.push(ContextMessage::new("assistant", "b".repeat(100))); // ~25 tokens
        assert_eq!(ctx.message_count(), 2);

        // This should trigger pruning — total would be ~75 tokens
        ctx.push(ContextMessage::new("user", "c".repeat(100))); // ~25 tokens
        assert!(ctx.total_tokens() <= 50);
        // At least one old message should have been removed
        assert!(ctx.message_count() <= 2);
    }

    #[test]
    fn context_window_pinned_messages_survive_pruning() {
        let mut ctx = ContextWindow::new(30);

        // Add a pinned message
        ctx.push(ContextMessage::new("system", "a".repeat(40)).pinned()); // ~10 tokens
        // Add unpinned messages
        ctx.push(ContextMessage::new("user", "b".repeat(40))); // ~10 tokens
        ctx.push(ContextMessage::new("assistant", "c".repeat(40))); // ~10 tokens

        // When pruning happens, pinned message should survive
        let pinned_count = ctx.messages().iter().filter(|m| m.pinned).count();
        assert!(pinned_count >= 1);

        // The pinned message should still be there
        assert!(
            ctx.messages()
                .iter()
                .any(|m| m.pinned && m.role == "system")
        );
    }

    #[test]
    fn context_window_usage_pct() {
        let mut ctx = ContextWindow::new(100);
        assert!((ctx.usage_pct() - 0.0).abs() < f64::EPSILON);

        ctx.push(ContextMessage::new("user", "a".repeat(200))); // ~50 tokens
        assert!(ctx.usage_pct() > 0.0);
        assert!(ctx.usage_pct() <= 1.0);
    }

    #[test]
    fn context_window_clear() {
        let mut ctx = ContextWindow::new(1000);
        ctx.push(ContextMessage::new("user", "Hello"));
        ctx.push(ContextMessage::new("assistant", "Hi"));
        assert_eq!(ctx.message_count(), 2);

        ctx.clear();
        assert_eq!(ctx.message_count(), 0);
    }

    #[test]
    fn context_window_summary() {
        let mut ctx = ContextWindow::new(1000);
        ctx.push(ContextMessage::new("user", "Hello").pinned());
        ctx.push(ContextMessage::new("assistant", "Hi"));

        let summary = ctx.summary();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.max_tokens, 1000);
        assert_eq!(summary.pinned_count, 1);
        assert!(summary.total_tokens > 0);
        assert!(summary.available_tokens < 1000);
    }

    #[test]
    fn model_context_sizes() {
        assert_eq!(model_context_size("claude-opus-4"), 200_000);
        assert_eq!(model_context_size("gpt-4o"), 128_000);
        assert_eq!(model_context_size("mistral"), 32_000);
        assert_eq!(model_context_size("unknown-model"), 8_000);
    }

    #[test]
    fn context_window_zero_capacity() {
        let mut ctx = ContextWindow::new(0);
        assert!(ctx.is_over_budget() || ctx.total_tokens() == 0);
        ctx.push(ContextMessage::new("user", "hello"));
        // Should prune immediately, but if all messages are needed it stays over
        assert!(ctx.is_over_budget() || ctx.message_count() == 0);
    }

    // -- Compaction tests ---------------------------------------------------

    #[test]
    fn needs_compaction_below_threshold() {
        let mut ctx = ContextWindow::new(1000);
        // Add ~10 tokens worth of messages — well below 80%
        ctx.push(ContextMessage::new("user", "Hello"));
        assert!(!ctx.needs_compaction());
    }

    #[test]
    fn needs_compaction_above_threshold() {
        let mut ctx = ContextWindow::new(100);
        // Add ~85 tokens (> 80% of 100)
        ctx.push(ContextMessage::new("user", "a".repeat(100))); // ~25 tokens
        ctx.push(ContextMessage::new("assistant", "b".repeat(100))); // ~25 tokens
        ctx.push(ContextMessage::new("user", "c".repeat(100))); // ~25 tokens
        ctx.push(ContextMessage::new("assistant", "d".repeat(40))); // ~10 tokens
        // Total ~85 tokens, 85% > 80%
        assert!(ctx.needs_compaction());
    }

    #[test]
    fn needs_compaction_requires_enough_messages() {
        let mut ctx = ContextWindow::new(50);
        ctx.set_compaction_threshold(0.5);
        // One big message — above threshold but only 1 compactable message
        ctx.push(ContextMessage::new("user", "a".repeat(120))); // ~30 tokens = 60%
        assert!(
            !ctx.needs_compaction(),
            "Need at least 3 compactable messages"
        );
    }

    #[test]
    fn compact_reduces_usage() {
        // Each message: 400 chars / 4 = 100 tokens. 4 messages = 400 tokens.
        // max_tokens = 450 → usage = 400/450 = 88.9% > 80% threshold.
        // No pruning because 400 < 450.
        let mut ctx = ContextWindow::new(450);
        ctx.push(ContextMessage::new("user", "a".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "b".repeat(400)));
        ctx.push(ContextMessage::new("user", "c".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "d".repeat(400)));

        let usage_before = ctx.usage_pct();
        assert!(
            ctx.needs_compaction(),
            "usage={:.2}% should exceed 80%",
            usage_before * 100.0
        );

        let result = ctx.compact(|msgs| {
            // Simple mock summarizer — return a short summary
            Ok(format!("Summary of {} messages.", msgs.len()))
        });

        assert!(result.is_some());
        let result = result.unwrap().unwrap();
        assert!(result.messages_compacted > 0);
        assert!(result.tokens_freed > 0);
        assert!(ctx.usage_pct() < usage_before);
    }

    #[test]
    fn compact_returns_none_when_not_needed() {
        let mut ctx = ContextWindow::new(10000);
        ctx.push(ContextMessage::new("user", "Hello"));

        let result = ctx.compact(|_| Ok("summary".into()));
        assert!(result.is_none());
    }

    #[test]
    fn compact_preserves_pinned_messages() {
        // 1 pinned (~4 tokens) + 4 messages (100 tokens each) = ~404 tokens.
        // max_tokens = 450 → 89.8% > 80%. No pruning.
        let mut ctx = ContextWindow::new(450);
        ctx.push(ContextMessage::new("system", "Important context").pinned());
        ctx.push(ContextMessage::new("user", "a".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "b".repeat(400)));
        ctx.push(ContextMessage::new("user", "c".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "d".repeat(400)));

        ctx.compact(|msgs| {
            // Pinned messages should NOT be in the compaction set
            assert!(msgs.iter().all(|m| !m.pinned));
            Ok("summary".into())
        });

        // The original pinned message should still exist
        assert!(
            ctx.messages()
                .iter()
                .any(|m| m.pinned && m.content == "Important context")
        );
    }

    #[test]
    fn compact_creates_compacted_summary_message() {
        // 4 messages of 100 tokens = 400 tokens. max_tokens = 450 → 88.9%.
        let mut ctx = ContextWindow::new(450);
        ctx.push(ContextMessage::new("user", "a".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "b".repeat(400)));
        ctx.push(ContextMessage::new("user", "c".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "d".repeat(400)));

        ctx.compact(|_| Ok("Compacted summary".into()));

        let compacted = ctx.messages().iter().find(|m| m.is_compacted);
        assert!(compacted.is_some());
        let compacted = compacted.unwrap();
        assert_eq!(compacted.role, "system");
        assert!(compacted.pinned);
        assert!(compacted.original_count.is_some());
        assert!(compacted.original_count.unwrap() > 0);
    }

    #[test]
    fn apply_compaction_summary_preserves_message_selection() {
        let mut ctx = ContextWindow::new(1_000);
        ctx.push(ContextMessage::new("system", "Important context").pinned());
        ctx.push(ContextMessage::new("user", "a".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "b".repeat(400)));
        ctx.push(ContextMessage::new("user", "c".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "d".repeat(400)));

        let selected = vec![ctx.messages()[1].clone(), ctx.messages()[2].clone()];
        let result = ctx
            .apply_compaction_summary(&selected, "Selected summary".into())
            .unwrap();

        assert_eq!(result.messages_compacted, 2);
        assert_eq!(ctx.messages()[0].content, "Important context");
        assert_eq!(ctx.messages()[1].content, "Selected summary");
        assert!(ctx.messages()[1].is_compacted);
        assert_eq!(ctx.messages()[2].content, "c".repeat(400));
        assert_eq!(ctx.messages()[3].content, "d".repeat(400));
    }

    #[test]
    fn compact_reactive_always_runs() {
        let mut ctx = ContextWindow::new(10000);
        // Well below threshold — but reactive should still compact
        ctx.push(ContextMessage::new("user", "a".repeat(80)));
        ctx.push(ContextMessage::new("assistant", "b".repeat(80)));
        ctx.push(ContextMessage::new("user", "c".repeat(80)));

        assert!(!ctx.needs_compaction());

        let result = ctx.compact_reactive(|_| Ok("reactive summary".into()));
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.messages_compacted > 0);
    }

    #[test]
    fn compact_if_needed_convenience() {
        let mut ctx = ContextWindow::new(10000);
        ctx.push(ContextMessage::new("user", "Hello"));

        // Not needed — should return Ok(None)
        let result = ctx.compact_if_needed(|_| Ok("summary".into()));
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn compact_propagates_summarizer_error() {
        // 4 messages of 100 tokens = 400 tokens. max_tokens = 450 → 88.9%.
        let mut ctx = ContextWindow::new(450);
        ctx.push(ContextMessage::new("user", "a".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "b".repeat(400)));
        ctx.push(ContextMessage::new("user", "c".repeat(400)));
        ctx.push(ContextMessage::new("assistant", "d".repeat(400)));

        let result = ctx.compact(|_| Err("AI unavailable".into()));
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn set_compaction_threshold() {
        let mut ctx = ContextWindow::new(1000);
        assert!((ctx.compaction_threshold() - 0.80).abs() < f64::EPSILON);

        ctx.set_compaction_threshold(0.60);
        assert!((ctx.compaction_threshold() - 0.60).abs() < f64::EPSILON);

        // Clamp to valid range
        ctx.set_compaction_threshold(1.5);
        assert!((ctx.compaction_threshold() - 1.0).abs() < f64::EPSILON);

        ctx.set_compaction_threshold(-0.1);
        assert!((ctx.compaction_threshold() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compaction_result_serialization() {
        let result = CompactionResult {
            messages_compacted: 5,
            tokens_freed: 200,
            summary_tokens: 50,
            usage_after: 0.45,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: CompactionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.messages_compacted, 5);
        assert_eq!(parsed.tokens_freed, 200);
    }
}
