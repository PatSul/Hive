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
3. **Recovers** session state automatically when conversations restart — queries FTS5 and injects a structured summary as a pinned message

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
| `session_id` | TEXT NOT NULL | FK to sessions |
| `entry_type` | TEXT NOT NULL | `decision`, `edit`, `task`, `tool_output`, `preference` |
| `key` | TEXT NOT NULL | Human-readable label |
| `content` | TEXT NOT NULL | Full content |
| `metadata` | TEXT | JSON blob (file paths, task IDs, etc.) |
| `tokens` | INTEGER NOT NULL | Estimated token count |
| `created_at` | TEXT NOT NULL | ISO 8601 |

**`session_entries_fts`** — FTS5 virtual table:

```sql
CREATE VIRTUAL TABLE session_entries_fts USING fts5(
    key, content,
    content=session_entries,
    content_rowid=id,
    tokenize='porter unicode61'
);
```

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
- Total DB capped at **100 MB** with LRU eviction of oldest sessions
- Sandbox threshold configurable via `~/.hive/config.toml`:

```toml
[context]
sandbox_threshold_tokens = 2000
```

## Tool Output Sandboxing

### Flow

```
Tool executes -> returns result string
    |
    v
estimate_tokens(result) > 2000?
    |-- NO  -> pass through as normal ChatMessage
    |-- YES -> journal.sandbox_tool_output(tool_name, result)
                |-- Store full output in session_entries (type: tool_output)
                |-- Generate summary: first 3 lines + entry reference
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
-> Full output stored (session entry #42, 3400 tokens). Use "recall #42" to retrieve.
```

### Recall

When the AI references `recall #N`, the `ContextEngine` injects the stored content as a temporary context source. It does not permanently re-enter the context window.

## Session Continuity

### Capture

The `SessionJournal` writes entries on these triggers:

| Trigger | Entry Type | Content |
|---|---|---|
| `ContextWindow::compact()` fires | `decision` | Decisions extracted from messages being pruned |
| File write/edit | `edit` | File path + change summary |
| Task status change | `task` | Task description + old -> new status |
| Tool output sandboxed | `tool_output` | Full output (handled by sandboxing) |
| User states preference | `preference` | Preference text |
| Conversation ends | — | Session `summary` updated |

### Decision Extraction

Before compaction discards messages, they are scanned for decision markers:

- "let's go with", "I'll use", "decided to", "choosing" -> `decision` entry
- "don't", "avoid", "instead of", "rejected" -> `decision` with rejected alternative
- Strong preference language in user messages -> `preference` entry

Keyword-based heuristics, not AI-powered. Fast and deterministic.

### Recovery

On conversation start for a project:

1. Query `sessions` for most recent session matching `project_dir` (within 24h)
2. Pull session `summary`
3. Query FTS5 for top entries by type:
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
  -> Compaction triggers decision extraction
  -> Tool outputs sandboxed

Conversation ends (or idle > 30min)
  -> Generate session summary
  -> Update sessions.last_active
```

## Public API

### SessionJournal

```rust
// hive_core/src/session_journal.rs

pub struct SessionJournal {
    db: rusqlite::Connection,
    current_session: Option<SessionId>,
}

impl SessionJournal {
    pub fn open(hive_dir: &Path) -> Result<Self>;
    pub fn in_memory() -> Result<Self>;
    pub fn start_session(&mut self, project_dir: &str) -> Result<SessionId>;
    pub fn record(&self, entry: JournalEntry) -> Result<EntryId>;
    pub fn sandbox_tool_output(&self, tool_name: &str, output: &str) -> Result<SandboxResult>;
    pub fn recall(&self, entry_id: EntryId) -> Result<String>;
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<JournalEntry>>;
    pub fn build_recovery_context(&self, project_dir: &str) -> Result<Option<String>>;
    pub fn extract_and_store_decisions(&self, messages: &[ContextMessage]) -> Result<usize>;
    pub fn end_session(&mut self) -> Result<()>;
    pub fn prune(&self, max_age_days: u32) -> Result<usize>;
}
```

### Supporting Types

```rust
pub type SessionId = String;  // UUID
pub type EntryId = i64;       // SQLite rowid

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
    pub tokens: usize,
    pub created_at: DateTime<Utc>,
}

pub struct SandboxResult {
    pub entry_id: EntryId,
    pub summary: String,
    pub tokens_saved: usize,
}
```

## Integration Points

| Crate | Usage |
|---|---|
| `hive_core` | `ContextWindow::compact()` calls `extract_and_store_decisions()` before pruning |
| `hive_ai` | Tool result handler calls `sandbox_tool_output()` for large outputs |
| `hive_ai` | Conversation start calls `build_recovery_context()`, injects as pinned message |
| `hive_ui` | File edit events call `record()` with `EntryType::Edit` |
| `hive_agents` | Task state changes call `record()` with `EntryType::Task` |

## Out of Scope

- Vector embeddings / semantic search (future phase)
- Cross-project search (sessions are project-scoped)
- UI for browsing session history (future)
- Manual "recall" command from users (AI-driven only)

## Dependencies

- `rusqlite` with `bundled` and `fts5` features added to `hive_core/Cargo.toml`
- `uuid` (already in workspace)
- `chrono` (already in workspace)
- `serde_json` (already in workspace)

## Testing Strategy

- Unit tests for `SessionJournal` using `in_memory()` constructor
- Test FTS5 search ranking and recall accuracy
- Test sandbox threshold logic (under/over 2000 tokens)
- Test recovery context assembly and 500-token cap
- Test decision extraction heuristics against sample messages
- Test session lifecycle (start, record, end, resume)
- Test pruning and size management
