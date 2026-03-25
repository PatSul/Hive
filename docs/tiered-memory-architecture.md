# Tiered Memory Architecture for Hive

**Date:** 2026-03-23
**Source:** OpenClaw 5-Layer Memory System (https://www.youtube.com/watch?v=m0V-hOjSHOw)
**Target:** `hive/crates/hive_agents/`

---

## Context

The OpenClaw 5-Layer Memory System addresses context compaction failures by layering different storage mechanisms, each serving a distinct purpose in the information lifecycle.

Hive already has a partial foundation:
- `CollectiveMemory` — SQLite store with LIKE queries and relevance decay
- `AgentPersistenceService` — JSON snapshots to disk
- `Queen` — calls `gather_memory_context()` with limited recall
- **`hive_ai::memory::HiveMemory`** — existing LanceDB-backed vector store with `EmbeddingProvider` support (OLLAMA, OpenAI, Mock)
- **`hive_ai::embeddings`** — existing `EmbeddingProvider` trait with `OllamaEmbeddings`, `OpenAiEmbeddings`, `MockEmbeddingProvider`

**Gaps:**
- No hot/warm tier distinction in hive_agents
- No session state survival across compaction
- No pre-compaction memory flush
- No bootstrap files that survive compaction
- No daily log archive layer

---

## Proposed Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                         HIVE MEMORY SYSTEM                           │
├──────────┬─────────────────────┬──────────────┬──────────────┬───────┤
│  LAYER 1 │      LAYER 2        │   LAYER 3    │   LAYER 4    │   5   │
│   HOT    │        WARM         │     COLD     │   ARCHIVE    │ CLOUD │
│  (RAM)   │      (Vectors)      │   (SQLite)   │  (Markdown) │(Opt.) │
├──────────┼─────────────────────┼──────────────┼──────────────┼───────┤
│Session-  │ hive_ai::memory::  │ Collective   │  Daily logs │ Turso │
│STATE.md   │ HiveMemory (WARM)  │ Memory       │  + MEMORY.md│  DB   │
│(WAL)     │ delegates here     │ (enhanced)   │             │       │
│          │ from hive_agents   │              │             │       │
├──────────┼─────────────────────┼──────────────┼──────────────┼───────┤
│Survives  │ Semantic search     │ Permanent    │ Human-      │ Cross │
│compaction │ via HiveMemory      │ Category-    │ readable    │ device│
│✓         │ ✓                   │ based ✓      │ ✓           │ sync  │
└──────────┴─────────────────────┴──────────────┴──────────────┴───────┘
```

### Layer Responsibilities

| Layer | Storage | Purpose | Survives Compaction | Owner |
|-------|---------|---------|---------------------|-------|
| **HOT** | `SESSION-STATE.md` + WAL | Active working memory | ✅ | hive_agents |
| **WARM** | `hive_ai::memory::HiveMemory` | Semantic search via LanceDB | ✅ | hive_ai |
| **COLD** | SQLite (`CollectiveMemory`) | Permanent categorized knowledge | ✅ | hive_agents |
| **ARCHIVE** | Markdown files | Human-readable daily logs | ✅ | hive_agents |
| **CLOUD** | Turso DB (optional) | Cross-device sync | ✅ | defer |

### Key Correction: Warm Layer delegates to hive_ai

The **warm layer delegates to `hive_ai::memory::HiveMemory`**, which already wraps LanceDB and an `EmbeddingProvider`. No new vector store is created in hive_agents. The `hive_agents::memory` module provides a **warm layer facade** that thin-wraps `HiveMemory` and adapts it to the tiered memory API.

---

## Implementation Phases

### Phase 1: Session State Hot Layer

**New File:** `hive_agents/src/memory/session_state.rs`

Session state is the agent's active working memory, designed to survive context compaction via WAL protocol.

**Key Types:**
```rust
pub struct Decision {
    pub timestamp: DateTime<Utc>,
    pub context: String,
    pub decision: String,
    pub rationale: Option<String>,
}

pub struct Entity {
    pub name: String,
    pub kind: String,
    pub properties: HashMap<String, String>,
    pub first_seen: DateTime<Utc>,
}

pub struct SessionState {
    pub active_context: Vec<String>,
    pub current_task: Option<String>,
    pub decisions_log: Vec<Decision>,
    pub entity_cache: HashMap<String, Entity>,
    pub pending_memory_writes: Vec<MemoryEntry>,
}
```

**WAL Protocol:**
- Write-ahead log flushed **before** context compaction events
- `session_state.md` loaded at session start
- Auto-snapshot on idle after N seconds of inactivity (configurable)

**API:**
```rust
impl SessionState {
    pub fn flush_to_wal(&self, path: &Path) -> Result<()>;
    pub fn recover_from_wal(path: &Path) -> Result<Self>;
    pub fn checkpoint(&self, path: &Path) -> Result<()>;
    pub fn touch_entity(&mut self, name: &str, kind: &str) -> &mut Entity;
    pub fn log_decision(&mut self, context: &str, decision: &str);
}
```

---

### Phase 2: Warm Layer Facade (Delegates to hive_ai::memory::HiveMemory)

**New File:** `hive_agents/src/memory/warm_layer.rs`

A thin facade in hive_agents that adapts `hive_ai::memory::HiveMemory` to the tiered memory API. No new vector store — this module **delegates** to the existing `HiveMemory`.

**Key Types:**
```rust
use hive_ai::memory::{HiveMemory, QueryResult};
use hive_ai::embeddings::EmbeddingProvider;

pub struct WarmLayer {
    inner: Arc<HiveMemory>,
}

pub struct SearchHit {
    pub content: String,
    pub category: String,
    pub importance: f32,
    pub score: f32,
}
```

**API:**
```rust
impl WarmLayer {
    pub async fn new(
        path: &str,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self>;

    pub async fn remember(&self, entry: &MemoryEntry) -> Result<()>;
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>>;
    pub async fn reindex_all(&self, entries: &[MemoryEntry]) -> Result<()>;
}
```

**EmbeddingProvider choices (from hive_ai::embeddings):**
| Provider | Model | Dimensions | Notes |
|----------|-------|------------|-------|
| `OllamaEmbeddings` | nomic-embed-text (default) | 768 | Local, no API cost |
| `OpenAiEmbeddings` | text-embedding-3-small | 1536 | Requires API key |
| `MockEmbeddingProvider` | N/A | configurable | Testing only |

---

### Phase 3: Enhanced Collective Memory (COLD)

**Modify:** `hive_agents/src/collective_memory.rs`

**Additions:**
```rust
impl MemoryEntry {
    pub source_session_id: Option<String>,  // NEW — track which session created this
    pub is_consolidated: bool,             // NEW — whether promoted to warm layer
}

impl CollectiveMemory {
    pub fn get_session_memories(&self, session_id: &str) -> Result<Vec<MemoryEntry>>;
    pub fn consolidate_batch(&self, entries: &[MemoryEntry]) -> Result<usize>;
    pub fn get_or_create_session_memory(&self, session_id: &str, category: MemoryCategory) -> Result<MemoryEntry>;
}
```

**LearnedPattern (NEW type — to be created):**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedPattern {
    pub id: i64,
    pub content: String,
    pub source: PatternSource,
    pub confidence: f32,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternSource {
    ActivityLog,
    SessionReplay,
    QueenPlanning,
    Manual,
}
```

**Integration with ActivityLog:**
- `ActivityService` emits structured events that `TieredMemory` consumes
- `LearnedPattern` entries are created from high-value `ActivityEvent` streams
- Phase 3: define the type; Phase 4+: wire up event consumption

---

### Phase 4: Bootstrap Files + Pre-Compaction Flush

**New File:** `hive_agents/src/memory/bootstrap.rs`
**Modify:** `hive_agents/src/hiveloop.rs`

Inspired by OpenClaw's SOUL.md, AGENTS.md, MEMORY.md:

**Bootstrap Files (auto-generated, survive compaction):**

| File | Purpose |
|------|---------|
| `IDENTITY.md` | Agent persona, tone, ethical boundaries |
| `AGENTS.md` | Role definitions, workflow rules |
| `MEMORY.md` | Curated cross-session knowledge (hot consolidated) |
| `CONTEXT.md` | Current session goals and constraints |

**Pre-Compaction Flush Hook:**
```rust
pub struct PreCompactionGuard {
    session_state: Arc<SessionState>,
    flush_buffer_tokens: usize,
}

impl PreCompactionGuard {
    pub fn before_compaction(&self) -> Result<()>;
    pub fn after_compaction(&self) -> Result<()>;
}
```

- Before compaction: calls `session_state.flush_to_wal()`
- After compaction: calls `session_state.recover_from_wal()`
- Config: `memory_flush_buffer_tokens` (default: 500 tokens)

---

### Phase 5: Archive + Daily Logs (ARCHIVE)

**New File:** `hive_agents/src/memory/archive.rs`

**Daily logs:** `memory/YYYY-MM-DD.md` in project root

**Format:**
```markdown
---
date: 2026-03-23
session_id: abc123
entries: 15
---

## Decisions

- [14:32] Chose OllamaEmbeddings for local vector search — no API cost

## Patterns Observed

- When encountering import errors, check PYTHONPATH first

## Tasks Completed

- Implemented session state WAL protocol
- Added pre-compaction flush hook
```

**API:**
```rust
pub struct ArchiveService {
    base_path: PathBuf,
}

impl ArchiveService {
    pub fn new(base_path: PathBuf) -> Self;
    pub fn consolidate_to_daily_log(&self, entry: &MemoryEntry) -> Result<()>;
    pub fn query_daily_logs(&self, date_range: DateRange, query: &str) -> Result<Vec<String>>;
}
```

**Git-Notes Sync (deferred to Phase 2):**
- Push consolidated logs to git notes for permanence
- Requires git integration

---

### Phase 6: TieredMemory Orchestrator

**New File:** `hive_agents/src/memory/tiered_memory.rs`

The top-level orchestrator that coordinates all 5 layers and exposes a unified API.

```rust
pub struct TieredMemory {
    hot: Arc<SessionState>,
    warm: Arc<WarmLayer>,
    cold: Arc<CollectiveMemory>,
    archive: Arc<ArchiveService>,
    config: TieredMemoryConfig,
}

impl TieredMemory {
    pub async fn new(config: TieredMemoryConfig) -> Result<Self>;
    pub async fn remember(&self, entry: &MemoryEntry) -> Result<()>;
    pub async fn recall(&self, query: &str, layer: Option<MemoryLayer>) -> Result<Vec<MemoryEntry>>;
    pub async fn consolidate(&self) -> Result<ConsolidationReport>;
}
```

---

## Module Structure

```
hive_agents/src/memory/
├── mod.rs                    # Public re-exports + TieredMemory
├── session_state.rs         # Phase 1: HOT layer + WAL
├── warm_layer.rs            # Phase 2: WARM facade (delegates to HiveMemory)
├── bootstrap.rs             # Phase 4: Bootstrap file generation
├── archive.rs               # Phase 5: ARCHIVE layer + daily logs
├── tiered_memory.rs         # Phase 6: TieredMemory orchestrator
└── tests/
    ├── session_state_tests.rs
    ├── warm_layer_tests.rs
    └── integration_tests.rs
```

---

## Dependency Additions

**No new crate dependencies needed.**

`hive_agents` already has `hive_ai` as a dependency. The warm layer uses `hive_ai::memory::HiveMemory` and `hive_ai::embeddings::EmbeddingProvider` (OllamaEmbeddings, OpenAiEmbeddings, MockEmbeddingProvider) — all already exist.

```
# hive_agents/Cargo.toml — no changes needed for warm layer
# hive_ai already brings in lancedb = "0.26" and arrow-array = "57"
```

Optional future feature flag for cloud sync (Layer 5):
```toml
[features]
cloud-sync = ["hive_agents/turso"]
```

---

## Backward Compatibility

- All new modules are **additive**
- `CollectiveMemory` enhanced in place (new fields optional)
- `Queen::gather_memory_context()` will call into `TieredMemory`
- `HiveMemory` in hive_ai is unchanged — warm_layer.rs only wraps it
- Default feature set unchanged — no new mandatory dependencies

---

## Verification Plan

1. **Unit tests** — each layer independently tested
2. **Integration test** — full compaction cycle: verify session state survives
3. **Benchmarks** — warm layer search latency vs cold layer LIKE query (expect 10-100x improvement)
4. **Load test** — 1000 memory entries, verify tiered retrieval < 50ms

---

## Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Embedding drift over time | Medium | Pin model version; periodic re-embed on config change |
| WAL corruption on crash | Low | Write-ahead + read-behind validation |
| Memory bloat in hot layer | Medium | Hard cap + LRU eviction |
| hive_ai / hive_agents circular dependency | Low | warm_layer uses Arc<dyn> interface only |

---

## Decision Points

Please review and confirm:

1. **Embedder for warm layer?**
   - ✅ `OllamaEmbeddings` (nomic-embed-text, local, 768d) — no API cost
   - `OpenAiEmbeddings` (text-embedding-3-small, 1536d) — better quality, requires API key

2. **Cloud sync (Layer 5)?**
   - ✅ Defer to Phase 2
   - Turso DB for SQLite sync across devices

3. **Git-notes permanence?**
   - ✅ Defer
   - Requires git integration

4. **LearnedPattern type — confirm Phase 3 creation?**
   - ✅ Define as new type in hive_agents
   - Wire into ActivityService event stream in Phase 4+

---

## Proposed File Changes Summary

| Action | File | Notes |
|--------|------|-------|
| **Create** | `hive_agents/src/memory/mod.rs` | Re-exports + TieredMemory |
| **Create** | `hive_agents/src/memory/session_state.rs` | HOT layer + WAL |
| **Create** | `hive_agents/src/memory/warm_layer.rs` | Delegates to `hive_ai::memory::HiveMemory` |
| **Create** | `hive_agents/src/memory/bootstrap.rs` | Bootstrap file generation |
| **Create** | `hive_agents/src/memory/archive.rs` | ARCHIVE layer + daily logs |
| **Create** | `hive_agents/src/memory/tiered_memory.rs` | Orchestrator |
| **Modify** | `hive_agents/src/collective_memory.rs` | Add session_id, is_consolidated, consolidate_batch |
| **Modify** | `hive_agents/src/hiveloop.rs` | Wire PreCompactionGuard |
| **Modify** | `hive_agents/src/lib.rs` | Add `pub mod memory;` |
| **Modify** | `hive_agents/Cargo.toml` | No changes needed (hive_ai already in deps) |
