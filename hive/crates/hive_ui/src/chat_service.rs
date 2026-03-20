//! Chat service bridge between the ChatPanel UI and the AiService backend.
//!
//! `ChatService` is a GPUI Entity that manages the conversation state and
//! drives streaming responses from [`hive_ai::AiService`]. It keeps its own
//! message list, streaming buffer, and error state so the UI can render
//! reactively via `cx.notify()`.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use gpui::{AsyncApp, Context, EventEmitter, Task, WeakEntity};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};
use uuid::Uuid;

use hive_ai::providers::AiProvider;
use hive_ai::types::{
    ChatMessage as AiChatMessage, ChatRequest, MessageRole as AiMessageRole, StopReason,
    StreamChunk, TokenUsage, ToolCall as AiToolCall,
};
use hive_core::context::{ContextMessage, ContextWindow};
use hive_core::conversations::{
    Conversation, ConversationStore, ConversationSummary, StoredMessage, generate_title,
};
use hive_ui_panels::components::diff_viewer::DiffLine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Role of a message in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Error,
    Tool,
}

impl MessageRole {
    /// Convert to the `hive_ai` wire type used by providers.
    pub fn to_ai_role(self) -> AiMessageRole {
        match self {
            Self::User => AiMessageRole::User,
            Self::Assistant => AiMessageRole::Assistant,
            Self::System => AiMessageRole::System,
            Self::Error => AiMessageRole::Error,
            Self::Tool => AiMessageRole::Tool,
        }
    }

    /// Convert from a string role (as stored in `StoredMessage`).
    pub fn from_stored(role: &str) -> Self {
        match role {
            "user" => Self::User,
            "assistant" => Self::Assistant,
            "system" => Self::System,
            "tool" => Self::Tool,
            _ => Self::Error,
        }
    }

    /// Convert to the string representation used by `StoredMessage`.
    pub fn to_stored(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Error => "error",
            Self::Tool => "tool",
        }
    }
}

/// A single chat message with metadata.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub model: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub cost: Option<f64>,
    pub tokens: Option<(usize, usize)>,
    /// Tool calls made by the assistant (present when stop_reason is ToolUse).
    pub tool_calls: Option<Vec<AiToolCall>>,
    /// For tool result messages: the ID of the tool call this responds to.
    pub tool_call_id: Option<String>,
    /// Whether this message is a compaction summary replacing earlier content.
    pub is_compacted: bool,
    /// Indices of the visible messages replaced by a compaction summary.
    pub compacted_from: Option<Vec<usize>>,
}

impl ChatMessage {
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            content: content.into(),
            model: None,
            timestamp: Utc::now(),
            cost: None,
            tokens: None,
            tool_calls: None,
            tool_call_id: None,
            is_compacted: false,
            compacted_from: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    pub fn assistant_placeholder() -> Self {
        Self::new(MessageRole::Assistant, "")
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Error, content)
    }

    /// Convert this `ChatMessage` into a `StoredMessage` for persistence.
    pub fn to_stored(&self) -> StoredMessage {
        // StoredMessage.tokens is a single u32 (total tokens).
        // ChatMessage.tokens is (input, output). Sum them for storage.
        let total_tokens = self.tokens.map(|(i, o)| (i + o) as u32);

        StoredMessage {
            role: self.role.to_stored().to_string(),
            content: self.content.clone(),
            timestamp: self.timestamp,
            model: self.model.clone(),
            cost: self.cost,
            tokens: total_tokens,
            thinking: None,
            is_compacted: self.is_compacted,
            compacted_from: self.compacted_from.clone(),
        }
    }

    /// Construct a `ChatMessage` from a `StoredMessage`.
    pub fn from_stored(stored: &StoredMessage) -> Self {
        // StoredMessage.tokens is a single u32 total. We cannot recover the
        // input/output split, so we store (0, total) by convention.
        let tokens = stored.tokens.map(|t| (0usize, t as usize));

        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::from_stored(&stored.role),
            content: stored.content.clone(),
            model: stored.model.clone(),
            timestamp: stored.timestamp,
            cost: stored.cost,
            tokens,
            tool_calls: None,
            tool_call_id: None,
            is_compacted: stored.is_compacted,
            compacted_from: stored.compacted_from.clone(),
        }
    }

    fn to_context_message(&self) -> Option<ContextMessage> {
        let role = match self.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            _ => return None,
        };

        let mut msg = ContextMessage::new(role, &self.content);
        msg.is_compacted = self.is_compacted;
        msg.original_count = self
            .compacted_from
            .as_ref()
            .map(|indices| indices.len() as u32);
        if self.is_compacted {
            msg.pinned = true;
        }
        Some(msg)
    }
}

// ---------------------------------------------------------------------------
// Tool Approval
// ---------------------------------------------------------------------------

/// Describes a pending write_file tool call awaiting user approval.
#[derive(Clone, Debug)]
pub struct PendingToolApproval {
    pub tool_call_id: String,
    pub tool_name: String,
    pub file_path: String,
    pub new_content: String,
    /// `None` means the file does not exist yet (new file creation).
    pub old_content: Option<String>,
    pub diff_lines: Vec<DiffLine>,
}

/// Compute a simple line-by-line diff between old and new content.
fn compute_diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut result = Vec::new();

    // Simple sequential comparison (not LCS, but good enough for file diffs
    // where changes are typically localized).
    let mut oi = 0;
    let mut ni = 0;
    while oi < old_lines.len() && ni < new_lines.len() {
        if old_lines[oi] == new_lines[ni] {
            result.push(DiffLine::Context(old_lines[oi].to_string()));
            oi += 1;
            ni += 1;
        } else {
            // Look ahead in new for a match of current old line.
            let mut found_in_new = None;
            for j in (ni + 1)..new_lines.len().min(ni + 5) {
                if new_lines[j] == old_lines[oi] {
                    found_in_new = Some(j);
                    break;
                }
            }
            if let Some(j) = found_in_new {
                // Lines ni..j are additions.
                for k in ni..j {
                    result.push(DiffLine::Added(new_lines[k].to_string()));
                }
                ni = j;
            } else {
                // Look ahead in old for a match of current new line.
                let mut found_in_old = None;
                for j in (oi + 1)..old_lines.len().min(oi + 5) {
                    if old_lines[j] == new_lines[ni] {
                        found_in_old = Some(j);
                        break;
                    }
                }
                if let Some(j) = found_in_old {
                    for k in oi..j {
                        result.push(DiffLine::Removed(old_lines[k].to_string()));
                    }
                    oi = j;
                } else {
                    result.push(DiffLine::Removed(old_lines[oi].to_string()));
                    result.push(DiffLine::Added(new_lines[ni].to_string()));
                    oi += 1;
                    ni += 1;
                }
            }
        }
    }

    // Remaining old lines are removals.
    while oi < old_lines.len() {
        result.push(DiffLine::Removed(old_lines[oi].to_string()));
        oi += 1;
    }
    // Remaining new lines are additions.
    while ni < new_lines.len() {
        result.push(DiffLine::Added(new_lines[ni].to_string()));
        ni += 1;
    }

    result
}

// ---------------------------------------------------------------------------
// ChatService
// ---------------------------------------------------------------------------

/// GPUI Entity that bridges the chat UI to the AI backend.
///
/// Owns the conversation message list, drives streaming, and exposes
/// read-only accessors for the renderer.
pub struct ChatService {
    pub messages: Vec<ChatMessage>,
    pub streaming_content: String,
    pub is_streaming: bool,
    current_model: String,
    pub error: Option<String>,
    /// Handle to the in-flight streaming task so it is not dropped.
    _stream_task: Option<Task<()>>,
    /// ID of the current conversation for persistence. `None` means the
    /// conversation has not been saved yet (a new UUID will be generated on
    /// first save).
    pub conversation_id: Option<String>,
    /// Last time we notified the UI during streaming. Used to throttle
    /// re-renders to ~15 fps instead of per-token.
    last_stream_notify: std::time::Instant,
    /// Monotonically increasing counter bumped on every mutation to the
    /// message list. Used by the UI to detect when cached display messages
    /// need to be rebuilt, avoiding per-frame string cloning.
    generation: u64,
    /// Pending tool approval (write_file) awaiting user decision.
    pub pending_approval: Option<PendingToolApproval>,
    /// Sender to resume the tool loop after approval/rejection.
    approval_tx: Option<oneshot::Sender<bool>>,
    /// Context window tracking token usage across conversation messages.
    /// Used to trigger proactive compaction before exceeding model limits.
    context_window: ContextWindow,
}

/// Route any "Unknown tool" results through the MCP integration server.
///
/// After the builtin tool registry runs, any tool it doesn't recognise gets
/// a second chance via `AppMcpServer::call_tool_value`. This keeps integration
/// tools (messaging, browser, deploy, etc.) reachable from both the normal
/// and the rejected-write_file dispatch paths.
fn route_unknown_to_mcp(
    this: &WeakEntity<ChatService>,
    app: &mut AsyncApp,
    results: &mut [hive_agents::tool_use::ToolResult],
    calls: &[hive_agents::tool_use::ToolCall],
) {
    for (result, call) in results.iter_mut().zip(calls.iter()) {
        if result.is_error && result.content.contains("Unknown tool") {
            let tool_name = call.name.clone();
            let tool_input = call.input.clone();
            if let Ok(mcp_result) = this.update(app, |_svc: &mut ChatService, cx| {
                if cx.has_global::<hive_ui_core::AppMcpServer>() {
                    cx.global::<hive_ui_core::AppMcpServer>()
                        .0
                        .call_tool_value(&tool_name, tool_input)
                } else {
                    Err(format!("Unknown tool: {tool_name}"))
                }
            }) {
                match mcp_result {
                    Ok(value) => {
                        result.content = serde_json::to_string_pretty(&value).unwrap_or_default();
                        result.is_error = false;
                    }
                    Err(e) => {
                        result.content = format!("Error: {e}");
                    }
                }
            }
        }
    }
}

impl ChatService {
    pub fn new(default_model: String) -> Self {
        Self {
            messages: Vec::new(),
            streaming_content: String::new(),
            is_streaming: false,
            current_model: default_model,
            error: None,
            _stream_task: None,
            conversation_id: None,
            last_stream_notify: std::time::Instant::now(),
            generation: 0,
            pending_approval: None,
            approval_tx: None,
            context_window: ContextWindow::new(128_000),
        }
    }

    // -- Accessors ----------------------------------------------------------

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    pub fn streaming_content(&self) -> &str {
        &self.streaming_content
    }

    pub fn current_model(&self) -> &str {
        &self.current_model
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Returns the current conversation ID, if one has been assigned.
    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    /// Returns the current generation counter. Incremented on every mutation
    /// to the message list, allowing the UI to detect stale caches.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    // -- Mutators -----------------------------------------------------------

    pub fn set_model(&mut self, model: String) {
        self.current_model = model;
    }

    // -- Context Window -----------------------------------------------------

    /// Whether the conversation context has grown enough to trigger compaction.
    pub fn needs_compaction(&self) -> bool {
        self.context_window.needs_compaction()
    }

    /// Context window usage as a percentage (0.0 to 1.0+).
    pub fn context_usage_pct(&self) -> f64 {
        self.context_window.usage_pct()
    }

    /// Returns the messages eligible for compaction (oldest non-pinned).
    /// The caller should summarize these and pass the result to [`apply_compaction`].
    pub fn messages_for_compaction(&self) -> Vec<ContextMessage> {
        self.context_window.compaction_candidates()
    }

    /// Apply a pre-computed compaction summary to the conversation.
    ///
    /// This replaces the oldest non-pinned messages in both the context window
    /// and the visible message list with a single system summary.
    pub fn apply_compaction(&mut self, summary: String, compacted_messages: &[ContextMessage]) {
        let Some(remove_indices) = self.find_visible_compaction_indices(compacted_messages) else {
            warn!("ChatService: compaction candidates no longer match visible messages");
            return;
        };

        let compaction = match self
            .context_window
            .apply_compaction_summary(compacted_messages, summary.clone())
        {
            Ok(compaction) => compaction,
            Err(e) => {
                warn!("ChatService: failed to apply compaction summary: {e}");
                return;
            }
        };

        let insert_at = remove_indices[0];
        let compacted_from = remove_indices.clone();
        for idx in remove_indices.into_iter().rev() {
            self.messages.remove(idx);
        }

        let mut summary_msg = ChatMessage::system(format!(
            "[Conversation compacted: {} messages summarized]\n\n{}",
            compaction.messages_compacted, summary
        ));
        summary_msg.is_compacted = true;
        summary_msg.compacted_from = Some(compacted_from);
        self.messages.insert(insert_at, summary_msg);
        self.generation += 1;

        info!(
            "ChatService: compacted {} messages, freed {} tokens, usage now {:.0}%",
            compaction.messages_compacted,
            compaction.tokens_freed,
            compaction.usage_after * 100.0,
        );

        if self.conversation_id.is_some()
            && let Err(e) = self.save_conversation()
        {
            warn!("ChatService: failed to save compacted conversation: {e}");
        }
    }

    fn matches_context_snapshot(message: &ChatMessage, snapshot: &ContextMessage) -> bool {
        matches!(
            (message.role, snapshot.role.as_str()),
            (MessageRole::User, "user")
                | (MessageRole::Assistant, "assistant")
                | (MessageRole::System, "system")
        ) && message.content == snapshot.content
            && message.is_compacted == snapshot.is_compacted
            && message
                .compacted_from
                .as_ref()
                .map(|indices| indices.len() as u32)
                == snapshot.original_count
    }

    fn find_visible_compaction_indices(
        &self,
        compacted_messages: &[ContextMessage],
    ) -> Option<Vec<usize>> {
        let mut indices = Vec::with_capacity(compacted_messages.len());
        let mut search_start = 0usize;

        for snapshot in compacted_messages {
            let (idx, _) = self
                .messages
                .iter()
                .enumerate()
                .skip(search_start)
                .find(|(_, message)| Self::matches_context_snapshot(message, snapshot))?;
            indices.push(idx);
            search_start = idx + 1;
        }

        Some(indices)
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.streaming_content.clear();
        self.is_streaming = false;
        self.error = None;
        self._stream_task = None;
        self.pending_approval = None;
        self.approval_tx = None;
        self.context_window = ContextWindow::new(self.context_window.max_tokens());
        self.generation += 1;
    }

    // -- Tool Approval ------------------------------------------------------

    /// Resolve a pending tool approval. If `approved` is true the write_file
    /// tool will execute; if false the tool is skipped and the AI is informed.
    pub fn resolve_approval(&mut self, approved: bool, cx: &mut gpui::Context<Self>) {
        self.pending_approval = None;
        if let Some(tx) = self.approval_tx.take() {
            let _ = tx.send(approved);
        }
        self.generation += 1;
        cx.notify();
    }

    // -- Persistence --------------------------------------------------------

    /// Start a fresh conversation, clearing all messages and assigning a new
    /// UUID. The previous conversation (if any) is not automatically saved;
    /// call [`save_conversation`] first if you need to persist it.
    pub fn new_conversation(&mut self) {
        self.clear();
        self.conversation_id = Some(Uuid::new_v4().to_string());
    }

    /// Save the current conversation to disk via [`ConversationStore`].
    ///
    /// If no `conversation_id` has been set yet, a new UUID is generated.
    /// The title is auto-generated from the first user message (up to 50
    /// chars). Error messages are excluded from the persisted data.
    pub fn save_conversation(&mut self) -> anyhow::Result<()> {
        // Lazily assign an ID on first save.
        let id = match &self.conversation_id {
            Some(id) => id.clone(),
            None => {
                let id = Uuid::new_v4().to_string();
                self.conversation_id = Some(id.clone());
                id
            }
        };

        let store = ConversationStore::new()?;
        self.save_to_store(&store, &id)
    }

    /// Save the current conversation to an arbitrary [`ConversationStore`].
    /// Useful for tests that provide a temp-dir-backed store.
    pub fn save_to_store(&self, store: &ConversationStore, id: &str) -> anyhow::Result<()> {
        // Convert ChatMessages -> StoredMessages, skipping errors and empty
        // placeholders (same filter as build_ai_messages).
        let stored_messages: Vec<StoredMessage> = self
            .messages
            .iter()
            .filter(|m| {
                m.role != MessageRole::Error
                    && !(m.role == MessageRole::Assistant && m.content.is_empty())
            })
            .map(|m| m.to_stored())
            .collect();

        let title = generate_title(&stored_messages);

        let total_cost: f64 = stored_messages.iter().filter_map(|m| m.cost).sum();

        let total_tokens: u32 = stored_messages.iter().filter_map(|m| m.tokens).sum();

        let now = Utc::now();

        // Try to load existing conversation to preserve created_at.
        let created_at = store
            .load(id)
            .map(|existing| existing.created_at)
            .unwrap_or(now);

        let conversation = Conversation {
            id: id.to_string(),
            title,
            messages: stored_messages,
            model: self.current_model.clone(),
            total_cost,
            total_tokens,
            created_at,
            updated_at: now,
            parent_id: None,
            branch_point_index: None,
            branch_name: None,
        };

        store.save(&conversation)
    }

    /// Load a conversation from disk by ID, replacing the current message
    /// list and state.
    ///
    /// On success the `conversation_id` is set to the loaded conversation's
    /// ID, and the `current_model` is updated to match the persisted model.
    pub fn load_conversation(&mut self, id: &str) -> anyhow::Result<()> {
        let store = ConversationStore::new()?;
        self.load_from_store(&store, id)
    }

    /// Load a conversation from an arbitrary [`ConversationStore`].
    /// Useful for tests that provide a temp-dir-backed store.
    pub fn load_from_store(&mut self, store: &ConversationStore, id: &str) -> anyhow::Result<()> {
        let conversation = store.load(id)?;

        // Convert StoredMessage -> ChatMessage.
        let messages: Vec<ChatMessage> = conversation
            .messages
            .iter()
            .map(ChatMessage::from_stored)
            .collect();

        self.messages = messages;
        self.conversation_id = Some(conversation.id);
        self.current_model = conversation.model;
        self.streaming_content.clear();
        self.is_streaming = false;
        self.error = None;
        self._stream_task = None;
        self.generation += 1;

        // Rebuild the context window from loaded messages.
        self.context_window = ContextWindow::new(self.context_window.max_tokens());
        for msg in &self.messages {
            if let Some(ctx_msg) = msg.to_context_message() {
                self.context_window.push(ctx_msg);
            }
        }

        info!(
            "ChatService: loaded conversation {} ({} messages, ctx_usage={:.0}%)",
            id,
            self.messages.len(),
            self.context_window.usage_pct() * 100.0,
        );

        Ok(())
    }

    /// List conversation summaries from disk, sorted newest-first.
    pub fn list_conversations() -> anyhow::Result<Vec<ConversationSummary>> {
        let store = ConversationStore::new()?;
        store.list_summaries()
    }

    /// Delete a conversation from disk by ID.
    pub fn delete_conversation(id: &str) -> anyhow::Result<()> {
        let store = ConversationStore::new()?;
        store.delete(id)
    }

    // -- Sending ------------------------------------------------------------

    /// Send a user message and begin streaming the assistant response.
    ///
    /// This is the primary entry point called by the UI when the user presses
    /// Send. It:
    /// 1. Appends the user message to the conversation.
    /// 2. Creates a placeholder assistant message.
    /// 3. Spawns an async task that receives a `tokio::sync::mpsc::Receiver`
    ///    of `StreamChunk`s and feeds them back to `self` through
    ///    `WeakEntity::update`.
    ///
    /// The actual provider call (`AiService::stream_chat`) is expected to be
    /// initiated *outside* this entity because `AiService` lives as a GPUI
    /// Global and cannot be accessed from within `Context<ChatService>`.
    /// Instead, we use a channel: the caller is responsible for calling
    /// [`ChatService::attach_stream`] with the receiver.
    pub fn send_message(&mut self, content: String, model: &str, cx: &mut Context<Self>) {
        // Clear previous error.
        self.error = None;

        // Budget enforcement: block the send when daily or monthly cost limit
        // is exceeded. The CostTracker lives inside AppAiService (a GPUI Global).
        if cx.has_global::<crate::AppAiService>() {
            let tracker = cx.global::<crate::AppAiService>().0.cost_tracker();
            if tracker.is_daily_budget_exceeded() {
                warn!("ChatService: daily cost budget exceeded — blocking send");
                self.set_error(
                    "Daily cost budget exceeded. Adjust your limit in Settings \u{2192} Costs.",
                    cx,
                );
                return;
            }
            if tracker.is_monthly_budget_exceeded() {
                warn!("ChatService: monthly cost budget exceeded — blocking send");
                self.set_error(
                    "Monthly cost budget exceeded. Adjust your limit in Settings \u{2192} Costs.",
                    cx,
                );
                return;
            }
        }

        // Shield: scan outgoing message for secrets/credentials.
        let shield_enabled = if cx.has_global::<crate::AppConfig>() {
            cx.global::<crate::AppConfig>().0.get().shield_enabled
        } else {
            true // default to enabled if no config
        };
        if shield_enabled && cx.has_global::<crate::AppShield>() {
            let shield = &cx.global::<crate::AppShield>().0;
            let secrets = shield.scan_secrets(&content);
            if !secrets.is_empty() {
                let count = secrets.len();
                let types: Vec<String> =
                    secrets.iter().map(|s| s.secret_type.to_string()).collect();
                let unique_types: Vec<&str> = {
                    let mut seen = std::collections::HashSet::new();
                    types
                        .iter()
                        .filter(|t| seen.insert(t.as_str()))
                        .map(|t| t.as_str())
                        .collect()
                };
                warn!(
                    "Shield: detected {} secret(s) in outgoing message: [{}]",
                    count,
                    unique_types.join(", ")
                );
                // Insert a warning message into the chat so the user is aware.
                let warning_text = format!(
                    "\u{26a0}\u{fe0f} Secret scan: {} credential(s) detected ({}). \
                     The message was sent, but consider removing secrets before sharing with AI.",
                    count,
                    unique_types.join(", ")
                );
                let warning_msg = ChatMessage::system(&warning_text);
                self.messages.push(warning_msg);
                self.generation += 1;
                cx.notify();
            }
        }

        // 1. Record the user message.
        let user_msg = ChatMessage::user(&content);
        self.messages.push(user_msg);

        // Track in context window for token budget management.
        if let Some(ctx_msg) = self
            .messages
            .last()
            .and_then(ChatMessage::to_context_message)
        {
            self.context_window.push(ctx_msg);
        }

        // 2. Prepare streaming state.
        self.is_streaming = true;
        self.streaming_content.clear();
        self.current_model = model.to_string();

        // 3. Add a placeholder assistant message that will be finalized later.
        let placeholder = ChatMessage::assistant_placeholder();
        self.messages.push(placeholder);

        self.generation += 1;

        info!(
            "ChatService: user message queued, awaiting stream attachment (model={})",
            model
        );

        // Notify the UI so the user message renders immediately.
        cx.notify();
    }

    /// Attach a stream receiver from `AiService::stream_chat` and begin
    /// consuming chunks.
    ///
    /// This must be called immediately after `send_message` while the
    /// placeholder assistant message is still the last entry. Typically the
    /// orchestrating layer (workspace or app) does:
    ///
    /// ```ignore
    /// chat_service.update(cx, |svc, cx| svc.send_message(text, model, cx));
    /// let rx = ai_service.stream_chat(messages, model, None).await?;
    /// chat_service.update(cx, |svc, cx| svc.attach_stream(rx, model, cx));
    /// ```
    pub fn attach_stream(
        &mut self,
        mut rx: mpsc::Receiver<StreamChunk>,
        model: String,
        cx: &mut Context<Self>,
    ) {
        let assistant_idx = self.messages.len().saturating_sub(1);
        let model_clone = model.clone();

        let task = cx.spawn(
            async move |this: WeakEntity<ChatService>, app: &mut AsyncApp| {
                let mut accumulated = String::new();
                let mut final_usage: Option<TokenUsage> = None;

                loop {
                    // Receive the next chunk. We poll via a small async block
                    // because `rx.recv()` is cancel-safe.
                    let chunk = rx.recv().await;

                    match chunk {
                        Some(chunk) => {
                            accumulated.push_str(&chunk.content);

                            if let Some(usage) = &chunk.usage {
                                final_usage = Some(usage.clone());
                            }

                            let is_done = chunk.done;

                            // Throttle UI updates to ~15 fps (67ms) during streaming.
                            // Always notify on the final chunk.
                            let content_snapshot = accumulated.clone();
                            let update_result = this.update(app, |this: &mut ChatService, cx| {
                                this.streaming_content = content_snapshot;
                                let elapsed = this.last_stream_notify.elapsed();
                                if is_done || elapsed.as_millis() >= 67 {
                                    this.last_stream_notify = std::time::Instant::now();
                                    cx.notify();
                                }
                            });

                            if update_result.is_err() {
                                // Entity was dropped.
                                break;
                            }

                            if is_done {
                                break;
                            }
                        }
                        None => {
                            // Channel closed (stream ended without a done flag).
                            break;
                        }
                    }
                }

                // Finalize: move accumulated content into the placeholder message.
                let usage = final_usage;
                let _ = this.update(app, |this: &mut ChatService, cx| {
                    this.finalize_stream(assistant_idx, &accumulated, &model_clone, usage.as_ref());

                    // Shield: scan incoming AI response for PII / secrets.
                    let shield_enabled = if cx.has_global::<crate::AppConfig>() {
                        cx.global::<crate::AppConfig>().0.get().shield_enabled
                    } else {
                        true
                    };
                    if shield_enabled && cx.has_global::<crate::AppShield>() {
                        let shield = &cx.global::<crate::AppShield>().0;
                        let result = shield.process_incoming(&accumulated);
                        match result.action {
                            hive_shield::ShieldAction::CloakAndAllow(ref cloaked) => {
                                if let Some(msg) = this.messages.get_mut(assistant_idx) {
                                    info!("Shield: PII cloaked in incoming AI response");
                                    msg.content = cloaked.text.clone();
                                }
                            }
                            hive_shield::ShieldAction::Warn(ref warning) => {
                                warn!("Shield: warning on incoming AI response: {warning}");
                            }
                            _ => {} // Allow or Block (we don't block incoming, just cloak)
                        }
                    }

                    this.emit_stream_completed(&model_clone, cx);
                    cx.notify();
                });
            },
        );

        self._stream_task = Some(task);
    }

    /// Attach a stream receiver and run the tool-use execution loop.
    ///
    /// Like [`attach_stream`], but when the model stops with `StopReason::ToolUse`,
    /// this method executes the requested tools via `hive_agents::tool_use`,
    /// appends the results to the conversation, and re-sends to the provider
    /// for continuation. Repeats up to `MAX_TOOL_ITERATIONS` times.
    pub fn attach_tool_stream(
        &mut self,
        rx: mpsc::Receiver<StreamChunk>,
        model: String,
        provider: Arc<dyn AiProvider>,
        initial_request: ChatRequest,
        cx: &mut Context<Self>,
    ) {
        let assistant_idx = self.messages.len().saturating_sub(1);
        let model_clone = model.clone();

        let task = cx.spawn(
            async move |this: WeakEntity<ChatService>, app: &mut AsyncApp| {
                let mut current_rx = rx;
                let mut current_request = initial_request;
                let mut current_assistant_idx = assistant_idx;
                let mut iteration = 0usize;
                const MAX_TOOL_ITERATIONS: usize = 10;

                loop {
                    // --- Consume the current stream ---
                    let mut accumulated = String::new();
                    let mut final_tool_calls: Vec<AiToolCall> = Vec::new();
                    let mut final_usage: Option<TokenUsage> = None;
                    let mut final_stop_reason: Option<StopReason> = None;

                    while let Some(chunk) = current_rx.recv().await {
                        accumulated.push_str(&chunk.content);

                        if let Some(ref u) = chunk.usage {
                            final_usage = Some(u.clone());
                        }
                        if let Some(ref tc) = chunk.tool_calls {
                            final_tool_calls = tc.clone();
                        }
                        if let Some(sr) = chunk.stop_reason {
                            final_stop_reason = Some(sr);
                        }

                        let is_done = chunk.done;
                        let snap = accumulated.clone();

                        let upd = this.update(app, |svc: &mut ChatService, cx| {
                            svc.streaming_content = snap;
                            let elapsed = svc.last_stream_notify.elapsed();
                            if is_done || elapsed.as_millis() >= 67 {
                                svc.last_stream_notify = std::time::Instant::now();
                                cx.notify();
                            }
                        });

                        if upd.is_err() || is_done {
                            break;
                        }
                    }

                    // --- Decide: tool loop or finalize ---
                    let is_tool_use = matches!(final_stop_reason, Some(StopReason::ToolUse))
                        && !final_tool_calls.is_empty()
                        && iteration < MAX_TOOL_ITERATIONS;

                    if !is_tool_use {
                        // Normal end — finalize the assistant message.
                        let m = model_clone.clone();
                        let acc_clone = accumulated.clone();
                        let _ = this.update(app, |svc: &mut ChatService, cx| {
                            svc.finalize_stream(
                                current_assistant_idx,
                                &accumulated,
                                &m,
                                final_usage.as_ref(),
                            );

                            // Shield: scan incoming AI response for PII / secrets.
                            let shield_enabled = if cx.has_global::<crate::AppConfig>() {
                                cx.global::<crate::AppConfig>().0.get().shield_enabled
                            } else {
                                true
                            };
                            if shield_enabled && cx.has_global::<crate::AppShield>() {
                                let shield = &cx.global::<crate::AppShield>().0;
                                let result = shield.process_incoming(&acc_clone);
                                match result.action {
                                    hive_shield::ShieldAction::CloakAndAllow(ref cloaked) => {
                                        if let Some(msg) = svc.messages.get_mut(current_assistant_idx) {
                                            info!("Shield: PII cloaked in incoming AI response");
                                            msg.content = cloaked.text.clone();
                                        }
                                    }
                                    hive_shield::ShieldAction::Warn(ref warning) => {
                                        warn!("Shield: warning on incoming AI response: {warning}");
                                    }
                                    _ => {}
                                }
                            }

                            svc.emit_stream_completed(&m, cx);
                            cx.notify();
                        });
                        break;
                    }

                    // --- Execute tools (with approval gate for write_file) ---
                    info!(
                        "Tool loop iteration {}: executing {} tool call(s)",
                        iteration + 1,
                        final_tool_calls.len()
                    );

                    // Check if any tool call is write_file — if so, gate it.
                    let write_file_call = final_tool_calls.iter().find(|tc| tc.name == "write_file");

                    if let Some(wf) = write_file_call {
                        // Extract file_path and content from tool call input.
                        let file_path = wf.input.get("file_path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let new_content = wf.input.get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        // Read existing file for diff.
                        let old_content = std::fs::read_to_string(&file_path).ok();
                        let diff_lines = if let Some(ref old) = old_content {
                            compute_diff_lines(old, &new_content)
                        } else {
                            new_content.lines()
                                .map(|l| DiffLine::Added(l.to_string()))
                                .collect()
                        };

                        let approval = PendingToolApproval {
                            tool_call_id: wf.id.clone(),
                            tool_name: wf.name.clone(),
                            file_path: file_path.clone(),
                            new_content: new_content.clone(),
                            old_content: old_content.clone(),
                            diff_lines,
                        };

                        // Create oneshot channel and set pending approval.
                        let (tx, rx) = oneshot::channel::<bool>();
                        let _ = this.update(app, |svc: &mut ChatService, cx| {
                            svc.pending_approval = Some(approval);
                            svc.approval_tx = Some(tx);
                            svc.generation += 1;
                            cx.notify();
                        });

                        // Wait for user decision.
                        let approved = rx.await.unwrap_or(false);

                        if !approved {
                            // User rejected — skip write_file, execute other tools,
                            // and add a rejection result.
                            let registry = hive_agents::tool_use::builtin_registry();
                            let agent_calls: Vec<hive_agents::tool_use::ToolCall> = final_tool_calls
                                .iter()
                                .filter(|tc| tc.name != "write_file")
                                .map(|tc| hive_agents::tool_use::ToolCall {
                                    id: tc.id.clone(),
                                    name: tc.name.clone(),
                                    input: tc.input.clone(),
                                })
                                .collect();
                            let mut results = registry.execute_all(&agent_calls);
                            route_unknown_to_mcp(&this, app, &mut results, &agent_calls);

                            // Add rejection result for write_file call.
                            results.push(hive_agents::tool_use::ToolResult {
                                tool_use_id: wf.id.clone(),
                                content: format!("User rejected write to {file_path}. Do not retry without asking."),
                                is_error: true,
                            });

                            // Continue with results below.
                            let _ = this.update(app, |svc: &mut ChatService, cx| {
                                svc.pending_approval = None;
                                svc.approval_tx = None;
                                cx.notify();
                            });

                            // Use these results for the rest of the loop.
                            let registry_results = results;

                            // --- Update conversation (rejected) ---
                            let m = model_clone.clone();
                            let tc_for_msg = final_tool_calls.clone();
                            let update_result = this.update(app, |svc: &mut ChatService, cx| {
                                if let Some(msg) = svc.messages.get_mut(current_assistant_idx) {
                                    msg.content = accumulated.clone();
                                    msg.model = Some(m.clone());
                                    msg.tool_calls = Some(tc_for_msg);
                                    if let Some(ref u) = final_usage {
                                        let cost = hive_ai::cost::calculate_cost(
                                            &m,
                                            u.prompt_tokens as usize,
                                            u.completion_tokens as usize,
                                        );
                                        msg.cost = Some(cost.total_cost);
                                        msg.tokens = Some((u.prompt_tokens as usize, u.completion_tokens as usize));
                                    }
                                }
                                for result in &registry_results {
                                    let mut tool_msg = ChatMessage::new(MessageRole::Tool, &result.content);
                                    tool_msg.tool_call_id = Some(result.tool_use_id.clone());
                                    svc.messages.push(tool_msg);
                                }
                                svc.messages.push(ChatMessage::assistant_placeholder());
                                svc.streaming_content.clear();
                                svc.generation += 1;
                                cx.notify();
                                svc.messages.len() - 1
                            });

                            let Ok(new_idx) = update_result else { break; };
                            current_assistant_idx = new_idx;

                            // Rebuild request and continue loop.
                            let msgs_result = this.update(app, |svc: &mut ChatService, _cx| svc.build_ai_messages());
                            let Ok(ai_messages) = msgs_result else { break; };
                            current_request = ChatRequest {
                                messages: ai_messages,
                                model: current_request.model.clone(),
                                max_tokens: current_request.max_tokens,
                                temperature: current_request.temperature,
                                system_prompt: current_request.system_prompt.clone(),
                                tools: current_request.tools.clone(),
                                cache_system_prompt: false,
                            };
                            match provider.stream_chat(&current_request).await {
                                Ok(rx) => { current_rx = rx; }
                                Err(e) => {
                                    error!("Tool re-send failed: {e}");
                                    let _ = this.update(app, |svc: &mut ChatService, cx| {
                                        svc.set_error(format!("Tool re-send failed: {e}"), cx);
                                    });
                                    break;
                                }
                            }
                            iteration += 1;
                            continue;
                        }

                        // User approved — clear approval state and proceed normally.
                        let _ = this.update(app, |svc: &mut ChatService, cx| {
                            svc.pending_approval = None;
                            svc.approval_tx = None;
                            cx.notify();
                        });
                    }

                    let registry = hive_agents::tool_use::builtin_registry();
                    let agent_calls: Vec<hive_agents::tool_use::ToolCall> = final_tool_calls
                        .iter()
                        .map(|tc| hive_agents::tool_use::ToolCall {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            input: tc.input.clone(),
                        })
                        .collect();
                    let mut results = registry.execute_all(&agent_calls);
                    route_unknown_to_mcp(&this, app, &mut results, &agent_calls);

                    // --- Update conversation ---
                    let m = model_clone.clone();
                    let tc_for_msg = final_tool_calls.clone();
                    let update_result = this.update(app, |svc: &mut ChatService, cx| {
                        // Finalize assistant message with tool_calls metadata.
                        if let Some(msg) = svc.messages.get_mut(current_assistant_idx) {
                            msg.content = accumulated.clone();
                            msg.model = Some(m.clone());
                            msg.tool_calls = Some(tc_for_msg);
                            if let Some(ref u) = final_usage {
                                let cost = hive_ai::cost::calculate_cost(
                                    &m,
                                    u.prompt_tokens as usize,
                                    u.completion_tokens as usize,
                                );
                                msg.cost = Some(cost.total_cost);
                                msg.tokens =
                                    Some((u.prompt_tokens as usize, u.completion_tokens as usize));
                            }
                        }

                        // Append tool result messages.
                        for result in &results {
                            let mut tool_msg = ChatMessage::new(MessageRole::Tool, &result.content);
                            tool_msg.tool_call_id = Some(result.tool_use_id.clone());
                            svc.messages.push(tool_msg);
                        }

                        // New placeholder for the next assistant response.
                        svc.messages.push(ChatMessage::assistant_placeholder());

                        svc.streaming_content.clear();
                        svc.generation += 1;
                        cx.notify();

                        // Return the index of the new placeholder.
                        svc.messages.len() - 1
                    });

                    let Ok(new_idx) = update_result else {
                        break; // entity dropped
                    };
                    current_assistant_idx = new_idx;

                    // --- Rebuild request with updated conversation ---
                    let msgs_result =
                        this.update(app, |svc: &mut ChatService, _cx| svc.build_ai_messages());

                    let Ok(ai_messages) = msgs_result else {
                        break; // entity dropped
                    };

                    current_request = ChatRequest {
                        messages: ai_messages,
                        model: current_request.model.clone(),
                        max_tokens: current_request.max_tokens,
                        temperature: current_request.temperature,
                        system_prompt: current_request.system_prompt.clone(),
                        tools: current_request.tools.clone(),
                        cache_system_prompt: false,
                    };

                    // --- Get new stream from provider ---
                    match provider.stream_chat(&current_request).await {
                        Ok(rx) => {
                            current_rx = rx;
                        }
                        Err(e) => {
                            error!("Tool re-send failed: {e}");
                            let _ = this.update(app, |svc: &mut ChatService, cx| {
                                svc.set_error(format!("Tool re-send failed: {e}"), cx);
                            });
                            break;
                        }
                    }

                    iteration += 1;
                }
            },
        );

        self._stream_task = Some(task);
    }

    /// Convenience method that combines `send_message` and `attach_stream`.
    ///
    /// Use this when the stream receiver is already available (e.g. in tests
    /// or when the caller has pre-started the stream).
    pub fn send_message_with_stream(
        &mut self,
        content: String,
        model: &str,
        rx: mpsc::Receiver<StreamChunk>,
        cx: &mut Context<Self>,
    ) {
        self.send_message(content, model, cx);
        self.attach_stream(rx, model.to_string(), cx);
    }

    /// Build the AI wire-format message history for the current conversation.
    ///
    /// Skips placeholder (empty assistant) and error messages. Useful for
    /// the caller to construct the `AiService::stream_chat` request.
    pub fn build_ai_messages(&self) -> Vec<AiChatMessage> {
        self.messages
            .iter()
            .filter(|m| {
                m.role != MessageRole::Error
                    // Skip empty assistant placeholders, but keep messages with tool_calls.
                    && !(m.role == MessageRole::Assistant
                        && m.content.is_empty()
                        && m.tool_calls.is_none())
            })
            .map(|m| AiChatMessage {
                role: m.role.to_ai_role(),
                content: m.content.clone(),
                timestamp: m.timestamp,
                tool_call_id: m.tool_call_id.clone(),
                tool_calls: m.tool_calls.clone(),
            })
            .collect()
    }

    // -- Internal -----------------------------------------------------------

    /// Replace the placeholder assistant message with the final content and
    /// update streaming state.
    pub fn finalize_stream(
        &mut self,
        assistant_idx: usize,
        content: &str,
        model: &str,
        usage: Option<&TokenUsage>,
    ) {
        if let Some(msg) = self.messages.get_mut(assistant_idx) {
            msg.content = content.to_string();
            msg.model = Some(model.to_string());

            if let Some(usage) = usage {
                let cost = hive_ai::cost::calculate_cost(
                    model,
                    usage.prompt_tokens as usize,
                    usage.completion_tokens as usize,
                );
                msg.cost = Some(cost.total_cost);
                msg.tokens = Some((
                    usage.prompt_tokens as usize,
                    usage.completion_tokens as usize,
                ));
            }
        }

        // Track assistant response in context window.
        if let Some(msg) = self.messages.get(assistant_idx)
            && let Some(ctx_msg) = msg.to_context_message()
        {
            self.context_window.push(ctx_msg);
        }

        self.streaming_content.clear();
        self.is_streaming = false;
        self._stream_task = None;
        self.generation += 1;

        info!(
            "ChatService: stream finalized ({} messages, model={}, ctx_usage={:.0}%)",
            self.messages.len(),
            model,
            self.context_window.usage_pct() * 100.0,
        );

        // Auto-save after finalization. Fire-and-forget: log on error but
        // don't propagate since streaming itself succeeded.
        if let Err(e) = self.save_conversation() {
            warn!("ChatService: auto-save failed: {e}");
        }
    }

    /// Emit a stream-completed event. Called from the attach_stream closure
    /// after finalize_stream completes.
    fn emit_stream_completed(&self, model: &str, cx: &mut Context<Self>) {
        let last_msg = self.messages.last();
        let cost = last_msg.and_then(|m| m.cost);
        let tokens = last_msg.and_then(|m| m.tokens);
        let content = last_msg.map(|m| m.content.clone());

        // TTS Auto-Speak
        if let Some(text) = content {
            if cx.has_global::<crate::AppConfig>() && cx.has_global::<crate::AppTts>() {
                let config = cx.global::<crate::AppConfig>().0.get();
                if config.tts_auto_speak {
                    let tts = cx.global::<crate::AppTts>().0.clone();
                    cx.spawn(
                        |_this: gpui::WeakEntity<Self>, _app: &mut gpui::AsyncApp| async move {
                            let _ = tts.speak_auto(&text).await;
                        },
                    )
                    .detach();
                }
            }
        }

        cx.emit(StreamCompleted {
            model: model.to_string(),
            message_count: self.messages.len(),
            cost,
            tokens,
        });
    }

    /// Record an error from the streaming task.
    pub fn set_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        let msg = message.into();
        error!("ChatService error: {}", msg);
        self.error = Some(msg.clone());
        self.is_streaming = false;
        self.streaming_content.clear();
        self._stream_task = None;

        // Remove the placeholder assistant message (last entry) if it is empty.
        if self
            .messages
            .last()
            .is_some_and(|last| last.role == MessageRole::Assistant && last.content.is_empty())
        {
            self.messages.pop();
        }

        // Push an error message so the user sees what happened.
        self.messages.push(ChatMessage::error(msg));
        self.generation += 1;
        cx.notify();
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when a streaming response is fully finalized.
///
/// The workspace subscribes to this to record learning outcomes.
#[derive(Debug, Clone)]
pub struct StreamCompleted {
    pub model: String,
    pub message_count: usize,
    pub cost: Option<f64>,
    pub tokens: Option<(usize, usize)>,
}

impl EventEmitter<StreamCompleted> for ChatService {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_round_trips_compaction_metadata() {
        let mut message = ChatMessage::system("Compacted summary");
        message.is_compacted = true;
        message.compacted_from = Some(vec![0, 1, 2]);

        let stored = message.to_stored();
        assert!(stored.is_compacted);
        assert_eq!(stored.compacted_from.as_deref(), Some(&[0, 1, 2][..]));

        let restored = ChatMessage::from_stored(&stored);
        assert!(restored.is_compacted);
        assert_eq!(restored.compacted_from.as_deref(), Some(&[0, 1, 2][..]));
    }

    #[test]
    fn apply_compaction_removes_exact_visible_messages() {
        let mut service = ChatService::new("auto".into());
        service.context_window = ContextWindow::new(1_000);

        let pinned = ChatMessage::system("Pinned context");
        service.context_window.push(
            pinned
                .to_context_message()
                .expect("system messages should map to context"),
        );
        service.messages.push(pinned);

        let user_a = ChatMessage::user("a".repeat(400));
        service
            .context_window
            .push(user_a.to_context_message().expect("user message"));
        service.messages.push(user_a);

        let assistant_b = ChatMessage::new(MessageRole::Assistant, "b".repeat(400));
        service
            .context_window
            .push(assistant_b.to_context_message().expect("assistant message"));
        service.messages.push(assistant_b);

        let mut tool_msg = ChatMessage::new(MessageRole::Tool, "tool output");
        tool_msg.tool_call_id = Some("call-1".into());
        service.messages.push(tool_msg);

        let user_c = ChatMessage::user("c".repeat(400));
        service
            .context_window
            .push(user_c.to_context_message().expect("user message"));
        service.messages.push(user_c);

        let assistant_d = ChatMessage::new(MessageRole::Assistant, "d".repeat(400));
        service
            .context_window
            .push(assistant_d.to_context_message().expect("assistant message"));
        service.messages.push(assistant_d);

        let selected = vec![
            service.context_window.messages()[1].clone(),
            service.context_window.messages()[2].clone(),
        ];
        service.apply_compaction("Summary".into(), &selected);

        assert_eq!(service.messages.len(), 5);
        assert_eq!(service.messages[0].content, "Pinned context");
        assert!(service.messages[1].is_compacted);
        assert_eq!(
            service.messages[1].compacted_from.as_deref(),
            Some(&[1, 2][..])
        );
        assert_eq!(service.messages[2].role, MessageRole::Tool);
        assert_eq!(service.messages[3].content, "c".repeat(400));
        assert_eq!(service.messages[4].content, "d".repeat(400));
    }
}
