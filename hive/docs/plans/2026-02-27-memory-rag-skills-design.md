# Hive Intelligence Systems Design

**Date:** 2026-02-27
**Inspired by:** MiniMax MaxClaw / OpenClaw architecture
**Status:** Approved

Three interconnected systems that give Hive persistent intelligence across sessions.

---

## System 1: Memory & RAG with Vector Embeddings

### Problem
RagService, SemanticSearchService, and ContextEngine are fully built (1,800+ lines) but:
- RagService is never indexed (no files fed in)
- SemanticSearchService is a complete orphan (zero callers)
- No persistence — everything lost on restart
- TF-IDF only — can't find "login bug" when docs say "authentication issue"
- LearningService has SQLite backend but isn't registered as a global

### Architecture

```
┌──────────────────────────────────────────────────────┐
│                    HiveMemory                         │
│  (new module in hive_ai)                             │
│                                                       │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │ EmbeddingMgr│  │  LanceDB     │  │  Indexer     │ │
│  │             │  │  Store       │  │  (background)│ │
│  │ ├─OpenAI    │  │              │  │              │ │
│  │ ├─Ollama    │  │ ├─chunks     │  │ ├─on_open()  │ │
│  │ └─(trait)   │  │ ├─memories   │  │ ├─on_change()│ │
│  │             │  │ └─metadata   │  │ └─on_save()  │ │
│  └──────┬──────┘  └──────┬───────┘  └──────┬───────┘ │
│         │                │                  │         │
│         └────────┬───────┘──────────────────┘         │
│                  │                                    │
│         ┌────────▼────────┐                          │
│         │  HiveMemoryAPI  │  ← single interface       │
│         │                 │                           │
│         │  .index_file()  │                           │
│         │  .query()       │  ← hybrid: vector + TF-IDF│
│         │  .remember()    │  ← save memory entry      │
│         │  .recall()      │  ← retrieve memories      │
│         │  .stats()       │                           │
│         └─────────────────┘                           │
└──────────────────────────────────────────────────────┘
```

### Components

#### EmbeddingProvider Trait
```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn model_name(&self) -> &str;
    fn dimensions(&self) -> usize;
}
```
- **OpenAiEmbeddings** — text-embedding-3-small (1536 dims, $0.02/1M tokens)
- **OllamaEmbeddings** — nomic-embed-text or similar (local, free)
- User selects provider in settings; graceful fallback to TF-IDF if unavailable

#### LanceDB Store (`~/.hive/memory.lance`)
Two tables:

**`chunks` table** — indexed file content:
| Column | Type | Description |
|--------|------|-------------|
| id | String | UUID |
| source_file | String | Relative path |
| content | String | Chunk text |
| embedding | Vector(f32) | Dense embedding |
| start_line | u32 | Chunk start |
| end_line | u32 | Chunk end |
| last_modified | DateTime | File mtime |
| tfidf_tokens | String (JSON) | For hybrid search |

**`memories` table** — conversation memories:
| Column | Type | Description |
|--------|------|-------------|
| id | String | UUID |
| content | String | Memory text |
| embedding | Vector(f32) | Dense embedding |
| category | String | UserPreference/CodePattern/TaskProgress/Decision/General |
| importance | f32 | 1-10 score |
| conversation_id | String | Source conversation |
| timestamp | DateTime | When saved |
| decay_exempt | bool | Preferences don't decay |

#### Hybrid Query
```
query(text, max_results, min_score) → Vec<MemoryResult>

1. Embed query text
2. Vector search in LanceDB (cosine similarity)
3. TF-IDF keyword search via existing RagService
4. Weighted merge: 0.7 × vector_score + 0.3 × tfidf_score
5. Temporal decay: score × e^(-λ × age_days), half-life 30 days
6. MMR deduplication: penalize near-duplicate results
7. Return top-k with scores
```

#### Background Indexer
- Spawns on workspace open via `cx.spawn()`
- Indexes all non-binary, non-ignored files in workspace
- Uses hive_fs file watcher for incremental updates
- Throttled: batches changes, re-embeds every 5 seconds max
- Tracks file hashes to skip unchanged files
- Non-blocking: all I/O on background thread

### Integration Points
- **main.rs**: Register `AppHiveMemory` global, start background indexer
- **workspace.rs**: Replace direct RagService query with HiveMemory.query()
- **ContextEngine**: Feed HiveMemory results via existing add_file()/add_pattern()
- **LearningService**: Register as global, wire OutcomeTracker to ChatService

---

## System 2: Memory Flush on Compaction

### Problem
When context hits 80% capacity, compaction summarizes old messages and replaces them. Valuable insights, decisions, and user preferences are lost to a generic summary.

### Solution
Intercept before compaction → extract durable memories → persist to LanceDB → then compact normally.

### Flow

```
Context at 80% → needs_compaction() = true
         │
         ▼
┌─────────────────────────────────┐
│  1. MEMORY FLUSH (new step)     │
│                                  │
│  Inject system message:          │
│  "Before context compaction,     │
│   extract key memories as JSON:  │
│   - User preferences/decisions   │
│   - Important code patterns      │
│   - Task context & progress      │
│   - Anything needed long-term"   │
│                                  │
│  Model responds with structured  │
│  JSON: [{content, importance,    │
│          category}]              │
│                                  │
│  → Embed each memory             │
│  → Store in LanceDB memories     │
│  → Silent (user doesn't see)     │
└──────────────┬──────────────────┘
               ▼
┌─────────────────────────────────┐
│  2. NORMAL COMPACTION           │
│  (existing behavior unchanged)  │
│  Summarize → replace → prune    │
└─────────────────────────────────┘
               ▼
┌─────────────────────────────────┐
│  3. MEMORY INJECTION            │
│                                  │
│  On each new message:            │
│  → recall(query) from LanceDB   │
│  → Inject top-k memories as     │
│    system context                │
│  → ContextEngine handles budget  │
└─────────────────────────────────┘
```

### Implementation Details

- **Hook location**: `hive_core::context::ContextWindow::compact()` — add pre-compaction callback
- **Model**: Use Budget tier (cheapest available) for memory extraction — it's structured extraction, not creative work
- **Silent turn**: Model response hidden from chat UI, not added to conversation history
- **Importance threshold**: Only memories scoring ≥5/10 are stored
- **Category enum**: `UserPreference`, `CodePattern`, `TaskProgress`, `Decision`, `General`
- **Temporal decay**: `score × e^(-λ × age_days)`
  - Task memories: 30-day half-life
  - User preferences: decay-exempt
  - Code patterns: 90-day half-life

### Memory Injection on New Messages
- Before sending any message to the AI, query LanceDB for relevant memories
- Inject as system context: "From previous conversations: [memory1], [memory2]..."
- Budget: max 500 tokens for memory injection (configurable)
- ContextEngine's existing token budgeting handles the allocation

---

## System 3: Skills Creation UI

### Problem
SkillRegistry + PluginManager exist with GitHub fetching and injection scanning, but users can't see, create, edit, or toggle skills from within Hive. It's all code-level.

### Design

```
┌─ Skills Panel (new panel in hive_ui_panels) ─────────┐
│                                                       │
│  ┌─ Tab Bar ────────────────────────────────────────┐ │
│  │  [My Skills]  [Installed]  [Browse]              │ │
│  └──────────────────────────────────────────────────┘ │
│                                                       │
│  ┌─ My Skills Tab ──────────────────────────────────┐ │
│  │  [+ New Skill]                                    │ │
│  │                                                   │ │
│  │  📝 Code Review Helper          [toggle] [edit]   │ │
│  │     "Reviews code for patterns..."                │ │
│  │                                                   │ │
│  │  📝 Bug Report Template         [toggle] [edit]   │ │
│  │     "Generates structured bug..."                 │ │
│  └───────────────────────────────────────────────────┘ │
│                                                       │
│  ┌─ Skill Editor (modal) ───────────────────────────┐ │
│  │  Name:  [________________]                        │ │
│  │  Desc:  [________________]                        │ │
│  │  Instructions (markdown):                         │ │
│  │  ┌──────────────────────────────────────────────┐ │ │
│  │  │ You are a code review assistant.             │ │ │
│  │  │ When reviewing code:                         │ │ │
│  │  │ 1. Check for security issues                 │ │ │
│  │  │ 2. Verify error handling                     │ │ │
│  │  └──────────────────────────────────────────────┘ │ │
│  │  ⚠️ Security scan: PASSED ✓                      │ │
│  │  [Save]  [Test in Chat]  [Cancel]                 │ │
│  └───────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────┘
```

### Skill Storage Format
Skills are markdown files in `~/.hive/skills/`:
```markdown
---
name: code-review-helper
description: Reviews code for common patterns and security issues
enabled: true
created: 2026-02-27T10:00:00Z
integrity_hash: sha256:abc123...
---

You are a code review assistant specialized in Rust.

When reviewing code:
1. Check for security issues (unsanitized input, command injection)
2. Verify error handling (no unwrap in production paths)
3. Look for performance issues (unnecessary clones, allocations)
...
```

### Key Behaviors

1. **Security scan on every save** — `scan_for_injection()` runs before skill is activated. Flagged skills show warnings but can still be saved (user's own skills, user's choice).

2. **Integrity verification on load** — SHA256 hash checked. If file was tampered with outside Hive, warn the user.

3. **Skill activation in chat** — user types `/skillname` or selects from a dropdown. Skill instructions injected as system message. ContextEngine handles token budget.

4. **"Test in Chat"** — opens a fresh conversation with the skill pre-loaded so users can iterate on the instructions.

5. **Three tabs**:
   - **My Skills** — user-created, full CRUD
   - **Installed** — from GitHub plugins, toggle on/off only
   - **Browse** — future marketplace placeholder (curated list for now)

### Files Touched
- **New**: `hive_ui_panels/src/skills_panel.rs` — GPUI panel view
- **Modify**: `hive_agents/src/skills.rs` — add CRUD for user skills, file-based persistence
- **Modify**: `hive_ui/src/workspace.rs` — register skills panel, wire skill activation to chat
- **Modify**: `hive_ui_core/src/sidebar.rs` — add skills icon to sidebar

---

## Cross-System Integration

```
User types message
       │
       ▼
┌─ Context Assembly ──────────────────────┐
│ 1. Active skill instructions (System 3)  │
│ 2. Recalled memories (System 1)          │
│ 3. RAG file context (System 1)           │
│ 4. Conversation history                  │
│ 5. System prompt                         │
│ → ContextEngine curates within budget    │
└──────────────────┬──────────────────────┘
                   ▼
            Send to AI provider
                   │
                   ▼
         Stream response back
                   │
                   ▼
┌─ Post-Response ────────────────────────┐
│ 1. Check context usage (System 2)       │
│ 2. If ≥80% → memory flush → compact    │
│ 3. Record outcome → LearningService     │
└─────────────────────────────────────────┘
```

---

## Dependencies

### New Crate Dependencies
- `lancedb` — embedded vector database (Rust bindings)
- `arrow` — Apache Arrow (required by LanceDB)
- `reqwest` — HTTP client for OpenAI embedding API (likely already present)

### Existing Code Reused
- `RagService` — TF-IDF scoring for hybrid search
- `SemanticSearchService` — file content search (may be replaced by LanceDB)
- `ContextEngine` — context curation and token budgeting
- `LearningService` — outcome tracking, routing, patterns
- `SkillRegistry` — skill storage, injection scanning, integrity hashing
- `PluginManager` — GitHub fetch, markdown parsing
- `hive_fs::FileWatcher` — file change notifications for indexer

### Implementation Order
1. **System 1 Phase A**: EmbeddingProvider trait + OpenAI/Ollama implementations
2. **System 1 Phase B**: LanceDB store + HiveMemoryAPI
3. **System 1 Phase C**: Background indexer + file watcher integration
4. **System 1 Phase D**: Wire into workspace context assembly
5. **System 2**: Memory flush on compaction (depends on System 1)
6. **System 3**: Skills panel UI (independent of Systems 1-2)
