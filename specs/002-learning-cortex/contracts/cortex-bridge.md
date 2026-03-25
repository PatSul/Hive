# Contract: CortexBridge Trait

**Purpose**: Cross-crate interface between LearningCortex (in `hive_learn`) and CollectiveMemory (in `hive_agents`).

## Interface

```
trait CortexBridge: Send + Sync
├── read_collective_entries(since: epoch, limit: usize) → Vec<BridgedMemoryEntry>
├── write_to_collective(category: String, content: String, relevance_score: f64) → Result<()>
└── content_hash_exists(hash: [u8; 32]) → bool
```

## Contract Rules

1. **No cross-crate types**: All parameters and return types use primitives (`String`, `f64`, `i64`, `Vec`, `Result`).
2. **Relevance decay**: Entries written via `write_to_collective` MUST have relevance_score capped at 0.6 (cross-context decay).
3. **Deduplication**: `content_hash_exists` checks SHA-256 of `category + content` before writing. The bridge MUST NOT write duplicate entries.
4. **Thread safety**: The trait requires `Send + Sync`. Implementations MUST be safe to call from the Cortex's async event loop (via `spawn_blocking`).
5. **Error handling**: `write_to_collective` returns `Result` — the Cortex logs errors but does not retry or propagate them.
