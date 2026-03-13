# Session Journal — Context Continuity & Tool Sandboxing

**Date:** 2026-03-13
**Status:** Approved
**Crate:** `hive_core`
**Inspired by:** [context-mode](https://github.com/mksglu/context-mode)

## Problem

When Hive conversations compact or restart, important session state is lost: architectural decisions, file edits, task progress, and user preferences. Users must re-explain context. Additionally, large tool outputs (file reads, git diffs, search results) consume disproportionate context window space, triggering premature compaction.

## Solution

A `SessionJournal` service in `hive_core` backed by SQLite FTS5 at `~/.hive/sessions.db`. It:

1. **Captures** session events (decisions, edits, tasks, preferences) as structured entries
2. **Sandboxes** large tool outputs — stores full content in the DB, injects only a summary into the context window
3. **Recovers** session state automatically when conversations restart — queries the DB and injects a structured summary as a pinned message

## Approach

**Chosen:** Session Journal in `hive_core` (Approach A).

**Rejected alternatives:**
- Extending `LearningStorage` in `hive_learn` — inverts dependency direction (core would depend on learn)
- New `hive_context` crate — unnecessary boilerplate for the current scope

The journal lives in `hive_core` alongside `ContextWindow` because they're tightly coupled (compaction triggers decision extraction). Adding `rusqlite` to `hive_core` is the only new dependency.

## Data Model

### Tables

**`sessions`** — One row per conversation session:

| Column | Type | Description |
|---|---|---|
| `id` | TEXT PK | UUID |
| `project_dir` | TEXT | Project root path |
| `started_at` | TEXT | ISO 8601 |
| `last_active` | TEXT | ISO 8601 |
| `summary` | TEXT | Auto-generated on session end |

**`session_entries`** — Every meaningful event:

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER PK | Auto-increment |
| `session_id` | TEXT NOT NULL | FK to sessions (ON DELETE CASCADE) |
| `entry_type` | TEXT NOT NULL | `decision`, `edit`, `task`, `tool_output`, `preference` |
| `key` | TEXT NOT NULL | Human-readable label |
| `content` | TEXT NOT NULL | Full content |
| `metadata` | TEXT | JSON blob (file paths, task IDs, etc.) |
| `tokens` | INTEGER NOT NULL | Estimated token count |
| `created_at` | TEXT NOT NULL | ISO 8601 |

Foreign key: `FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE`

**`session_entries_fts`** — FTS5 virtual table with sync triggers:

```sql
CREATE VIRTUAL TABLE session_entries_fts USING fts5(
    key, content,
    content=session_entries,
    content_rowid=id,
    tokenize='porter unicode61'
);

-- Sync triggers (required for external content FTS5 tables)
CREATE TRIGGER session_entries_ai AFTER INSERT ON session_entries BEGIN
    INSERT INTO session_entries_fts(rowid, key, content)
    VALUES (new.id, new.key, new.content);
END;

CREATE TRIGGER session_entries_ad AFTER DELETE ON session_entries BEGIN
    INSERT INTO session_entries_fts(session_entries_fts, rowid, key, content)
    VALUES ('delete', old.id, old.key, old.content);
END;

CREATE TRIGGER session_entries_au AFTER UPDATE ON session_entries BEGIN
    INSERT INTO session_entries_fts(session_entries_fts, rowid, key, content)
    VALUES ('delete', old.id, old.key, old.content);
    INSERT INTO session_entries_fts(rowid, key, content)
    VALUES (new.id, new.key, new.content);
END;
```

### Database Initialization

On `open()`, the connection runs:
```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
```

WAL mode allows concurrent reads while writing. Foreign keys must be explicitly enabled per-connection in SQLite.

### Entry Types

| Type | What's Stored | Trigger |
|---|---|---|
| `decision` | Choice + reasoning + rejected alternatives | AI/user makes an approach decision |
| `edit` | File path + change summary | File modified during session |
| `task` | Description + status + dependencies | Task state changes |
| `tool_output` | Full tool result content | Tool returns large result (sandboxed) |
| `preference` | User preference text | User states a preference |

### Size Management

- Tool outputs over **2000 tokens** are sandboxed (full stored, summary in context)
- Entries older than **30 days** auto-pruned on startup
- Total DB capped at **100 MB** with LRU eviction of oldest sessions via `enforce_size_limit()`
- Sandbox threshold configurable via `~/.hive/config.toml`:

```toml
[context]
sandbox_threshold_tokens = 2000
```

## Thread Safety

`SessionJournal` wraps the SQLite connection in `parking_lot::Mutex` (already a workspace dependency). All public methods acquire the lock internally. Integration points hold `Arc<SessionJournal>` for shared access across crates.

```rust
pub struct SessionJournal {
    db: parking_lot::Mutex<rusqlite::Connection>,
    current_session: parking_lot::Mutex<Option<SessionId>>,
}
```

This follows the same pattern as `LearningStorage` in `hive_learn` which uses `std::sync::Mutex<Connection>`.

## Tool Output Sandboxing

### Flow

```
Tool executes -> returns result string
    |
    v
estimate_tokens(result) > threshold?
    |-- NO  -> pass through as normal ChatMessage
    |-- YES -> journal.sandbox_tool_output(tool_name, result)
                |-- Store full output in session_entries (type: tool_output)
                |-- Generate summary: first N lines (up to 3) + entry reference
                |-- Return SandboxResult with summary string
                       |
                       v
                ChatMessage.content = summary (not full output)
```

### Integration Point

Between tool execution and `ChatMessage` creation in `hive_ai`. Currently:

1. AI returns `ChatResponse` with `tool_calls`
2. Tool executes, produces output string
3. Output becomes `ChatMessage { role: Tool, content: full_output, tool_call_id }`
4. Message pushed into `ContextWindow`

**Change:** Between steps 2 and 3, check token count. If over threshold, sandbox.

### Summary Format

```
[Tool: read_file] 847 lines from src/main.rs
First 3 lines: use std::env; use anyhow::Result; fn main() -> Result<()> {
-> Full output stored (session entry #42, 3400 tokens).
```

### Recall via Tool Call

Recall is implemented as a **tool definition** exposed to the AI, not a text pattern:

```rust
ToolDefinition {
    name: "recall_session_entry",
    description: "Retrieve full content of a sandboxed tool output by entry ID",
    input_schema: { "entry_id": "integer" },
}
```

When the AI invokes this tool, the handler calls `journal.recall(entry_id)` and returns the content as a regular tool result. This uses the existing tool call infrastructure and requires no changes to `ContextEngine`. The recalled content enters the context as a normal tool result message and is subject to standard context window management (compaction, etc.).

## Session Continuity

### Capture

The `SessionJournal` writes entries on these triggers:

| Trigger | Entry Type | Content |
|---|---|---|
| Caller invokes `compact()` on ContextWindow | `decision` | Decisions extracted from messages being pruned |
| File write/edit | `edit` | File path + change summary |
| Task status change | `task` | Task description + old -> new status |
| Tool output sandboxed | `tool_output` | Full output (handled by sandboxing) |
| User states preference | `preference` | Preference text |
| Conversation ends | — | Session `summary` updated |

### Compaction Integration

`ContextWindow::compact()` currently takes a summarizer callback `FnOnce(&[ContextMessage]) -> Result<String, String>`. The journal hooks in at the **call site**, not inside `compact()` itself:

```rust
// In the caller (hive_ai conversation loop):
if context_window.needs_compaction() {
    // 1. Get the messages that will be compacted
    let compactable = context_window.get_compactable_messages();

    // 2. Extract decisions BEFORE compaction discards them
    journal.extract_and_store_decisions(&compactable)?;

    // 3. Run compaction as normal
    context_window.compact(|msgs| ai_summarize(msgs))?;
}
```

This requires adding `pub fn get_compactable_messages(&self) -> Vec<ContextMessage>` to `ContextWindow` — a read-only method that returns the same message slice that `compact()` would pass to the summarizer.

### Decision Extraction

Before compaction discards messages, they are scanned for decision markers:

- "let's go with", "I'll use", "decided to", "choosing" -> `decision` entry
- "don't", "avoid", "instead of", "rejected" -> `decision` with rejected alternative
- Strong preference language in user messages -> `preference` entry

Keyword-based heuristics, not AI-powered. Fast and deterministic.

**Known limitation:** English-only heuristics for v1. Future versions may use the AI model for multi-language extraction.

### Recovery

On conversation start for a project:

1. Query `sessions` table for most recent session matching `project_dir` (within 24h)
2. Pull session `summary`
3. Query `session_entries` table (not FTS5) for top entries by type and recency:
   - Last 3 `decision` entries
   - Last 3 `edit` entries
   - Last 2 `task` entries (incomplete prioritized)
   - Last 2 `preference` entries
4. Assemble structured block (~500 tokens max):

```
[Session Context -- resumed from 2h ago]
Decisions: Chose Approach A (SessionJournal in hive_core). Rejected extending LearningStorage.
Recent edits: Added sandbox() to session_journal.rs. Updated ContextWindow::compact() hook.
Open tasks: "Wire up FTS5 recall" (in_progress), "Add config threshold" (pending)
Preferences: User prefers SQLite in ~/.hive/. User wants automatic recovery.
```

5. Inject as pinned `ContextMessage` at conversation start

Note: Recovery uses `SELECT ... WHERE session_id = ? AND entry_type = ? ORDER BY created_at DESC LIMIT ?` on the regular table. FTS5 is used only by the `search()` method for keyword-based recall during conversations.

### Token Budget

Recovery injection capped at **500 tokens**. Entries ranked by recency and type priority: `task` > `decision` > `edit` > `preference`.

### Session Lifecycle

```
Conversation starts
  -> Check for recent session (same project, last 24h)
     |-- Found -> inject recovery context, continue same session_id
     |-- Not found -> create new session

During conversation
  -> Events flow into SessionJournal entries
  -> Compaction triggers decision extraction (at call site)
  -> Tool outputs sandboxed

Conversation ends (or idle > 30min)
  -> Generate session summary
  -> Update sessions.last_active
```

Idle detection is the **caller's responsibility**. The UI layer should maintain a timer and call `end_session()` after 30 minutes of inactivity. `SessionJournal` itself has no timer.

### Session Summary Generation

`end_session()` generates the summary by concatenating the most recent entry keys grouped by type into a structured string (same format as recovery context). This is deterministic (no AI call) so it works offline. The summary is capped at 500 tokens.

## Public API

### SessionJournal

```rust
// hive_core/src/session_journal.rs

pub struct SessionJournal {
    db: parking_lot::Mutex<rusqlite::Connection>,
    current_session: parking_lot::Mutex<Option<SessionId>>,
}

impl SessionJournal {
    /// Open or create the journal DB at ~/.hive/sessions.db
    /// Runs PRAGMA foreign_keys = ON and PRAGMA journal_mode = WAL
    pub fn open(hive_dir: &Path) -> Result<Self>;

    /// In-memory DB for testing. Caller must call start_session() before recording.
    pub fn in_memory() -> Result<Self>;

    /// Start or resume a session for a project directory.
    /// Resumes if a session for the same project exists within 24h.
    pub fn start_session(&self, project_dir: &str) -> Result<SessionId>;

    /// Record a session entry. Auto-fills session_id from current_session.
    /// Returns error if no session is active.
    pub fn record(&self, entry_type: EntryType, key: &str, content: &str, metadata: Option<serde_json::Value>) -> Result<EntryId>;

    /// Sandbox a large tool output: store full content, return summary.
    /// Requires an active session.
    pub fn sandbox_tool_output(&self, tool_name: &str, output: &str) -> Result<SandboxResult>;

    /// Recall a sandboxed entry by ID. Returns the full content string.
    pub fn recall(&self, entry_id: EntryId) -> Result<String>;

    /// FTS5 search across session entries (keyword-based).
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<JournalEntry>>;

    /// Build recovery context for conversation start (~500 tokens).
    /// Returns None if no recent session exists for this project.
    pub fn build_recovery_context(&self, project_dir: &str) -> Result<Option<String>>;

    /// Extract decisions from messages about to be compacted.
    /// Called by the compaction call site, not by ContextWindow itself.
    pub fn extract_and_store_decisions(&self, messages: &[ContextMessage]) -> Result<usize>;

    /// End session: generate summary from entry keys, update last_active.
    pub fn end_session(&self) -> Result<()>;

    /// Prune entries older than max_age_days.
    pub fn prune(&self, max_age_days: u32) -> Result<usize>;

    /// Evict oldest sessions until DB is under max_bytes.
    pub fn enforce_size_limit(&self, max_bytes: u64) -> Result<usize>;
}
```

### Supporting Types

```rust
pub type SessionId = String;  // UUID
pub type EntryId = i64;       // SQLite rowid (i64, not usize)

pub enum EntryType {
    Decision,
    Edit,
    Task,
    ToolOutput,
    Preference,
}

pub struct JournalEntry {
    pub id: Option<EntryId>,
    pub session_id: SessionId,
    pub entry_type: EntryType,
    pub key: String,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub tokens: i64,            // i64 to match SQLite INTEGER
    pub created_at: DateTime<Utc>,
}

pub struct SandboxResult {
    pub entry_id: EntryId,
    pub summary: String,        // What goes into the ChatMessage
    pub tokens_saved: i64,      // Full tokens - summary tokens
}
```

### Changes to ContextWindow

Add one new read-only method:

```rust
impl ContextWindow {
    /// Returns the messages that would be selected for compaction,
    /// without actually compacting. Used by the caller to extract
    /// decisions before invoking compact().
    pub fn get_compactable_messages(&self) -> Vec<ContextMessage>;
}
```

### Recall Tool Definition

```rust
// Registered alongside other tool definitions in hive_ai
ToolDefinition {
    name: "recall_session_entry".to_string(),
    description: "Retrieve full content of a previously sandboxed tool output by entry ID".to_string(),
    input_schema: serde_json::json!({
        "type": "object",
        "properties": {
            "entry_id": { "type": "integer", "description": "The session entry ID to recall" }
        },
        "required": ["entry_id"]
    }),
}
```

## Integration Points

| Crate | Usage |
|---|---|
| `hive_core` | New `SessionJournal` module + `ContextWindow::get_compactable_messages()` |
| `hive_ai` | Conversation loop: call `extract_and_store_decisions()` before `compact()` |
| `hive_ai` | Tool result handler: call `sandbox_tool_output()` for large outputs |
| `hive_ai` | Conversation start: call `build_recovery_context()`, inject as pinned message |
| `hive_ai` | Register `recall_session_entry` tool definition |
| `hive_ui` | File edit events: call `record()` with `EntryType::Edit` |
| `hive_agents` | Task state changes: call `record()` with `EntryType::Task` |

## Out of Scope

- Vector embeddings / semantic search (future phase)
- Cross-project search (sessions are project-scoped)
- UI for browsing session history (future)
- Multi-language decision extraction heuristics (v1 is English-only)

## Dependencies

- `rusqlite` with `bundled` and `fts5` features — **must update workspace `Cargo.toml`** at `hive/Cargo.toml` to add `fts5` feature to the existing `rusqlite` dependency (currently only has `bundled`)
- `parking_lot` (already in workspace)
- `uuid` (already in workspace)
- `chrono` (already in workspace)
- `serde_json` (already in workspace)

## Testing Strategy

- Unit tests for `SessionJournal` using `in_memory()` constructor
  - All tests must call `start_session("test-project")` before recording
- Test FTS5 search ranking and recall accuracy
- Test FTS5 sync triggers (insert, update, delete reflected in search)
- Test sandbox threshold logic (under/over 2000 tokens, single-line outputs)
- Test recovery context assembly and 500-token cap
- Test decision extraction heuristics against sample messages
- Test session lifecycle (start, record, end, resume within 24h, new after 24h)
- Test pruning by age and size limit enforcement
- Test foreign key cascade (deleting session removes its entries)
- Test thread safety: concurrent reads and writes from multiple threads
