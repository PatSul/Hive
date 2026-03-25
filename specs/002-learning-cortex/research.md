# Research: Learning Cortex

**Date**: 2026-03-24
**Feature**: 002-learning-cortex

## Research Topics

### 1. Cross-Crate Event Bus Without Circular Dependencies

**Decision**: Use `tokio::sync::broadcast` channel with the `Sender` passed to each crate at startup. `CortexEvent` enum and sender type defined in `hive_learn` (the Cortex's home crate). Other crates (`hive_agents`, `hive_remote`) receive `broadcast::Sender<CortexEvent>` as a constructor parameter.

**Rationale**: `broadcast::Sender::send()` is synchronous, so it works in both sync (`on_outcome()`) and async contexts without API changes. The broadcast channel supports multiple receivers (Cortex subscribes). Fire-and-forget semantics mean event publishing never blocks producers.

**Alternatives considered**:
- `tokio::sync::mpsc` — Single consumer only; we may want multiple subscribers later (e.g., logging, analytics).
- Trait-based event sink — More flexible but requires trait objects and dynamic dispatch. Broadcast is simpler and sufficient.
- Shared event bus crate — Would avoid defining events in `hive_learn`, but adds a crate for a single enum. Not worth the complexity.

### 2. Trait-Based Bridge for CollectiveMemory Access

**Decision**: Define `CortexBridge` trait in `hive_learn/src/cortex/bridge.rs`. Concrete implementation `CortexBridgeImpl` lives in `hive_app/src/cortex_bridge_impl.rs` (which depends on both `hive_learn` and `hive_agents`).

**Rationale**: Follows the existing `LearningBridge` trait pattern already in the codebase (`hive_learn/src/learning_bridge.rs`). Avoids circular dependency. The trait uses only primitive types (`String`, `f64`, `i64`) — no cross-crate type imports needed.

**Alternatives considered**:
- New shared crate for types — Would work but adds build overhead for ~5 type definitions.
- String-based messaging — Loses type safety. Would require parsing at every boundary.
- Merge `hive_agents` learning into `hive_learn` — Too large a refactor; the systems have genuinely different lifecycles.

### 3. Threading Model for Cortex Event Loop

**Decision**: Cortex runs its own `tokio::spawn` task that loops on `broadcast::Receiver::recv()`. When it needs to call into `LearningService` subsystems (which use `std::sync::Mutex<Connection>`), it uses `tokio::task::spawn_blocking` to avoid holding the mutex across `.await` points.

**Rationale**: `rusqlite::Connection` is `Send` but not `Sync`. The existing `LearningStorage` wraps it in `std::sync::Mutex`, which would panic or deadlock if held across await points in an async context. `spawn_blocking` moves the blocking work to a dedicated thread pool.

**Alternatives considered**:
- Switch to `tokio::sync::Mutex` in LearningStorage — Would require changes across the entire `hive_learn` crate. Too invasive.
- Run Cortex on a dedicated OS thread with its own sync event loop — Works but loses tokio integration (timers, spawn, select).

### 4. Soak Period Implementation

**Decision**: Each applied change gets a `cortex_changes` database row with `soak_until` timestamp. The Cortex event loop periodically checks (every 60 seconds) for changes whose soak period has expired and evaluates quality metrics for the soak window.

**Rationale**: Database-backed soak tracking survives app restarts (startup recovery reads `Soaking` entries). Polling every 60 seconds is sufficient granularity for 1-hour and 24-hour soak periods.

**Alternatives considered**:
- Timer-based with `tokio::time::sleep` — Doesn't survive restarts. Would need separate recovery logic.
- Event-driven (check on every new outcome) — More responsive but adds processing to every interaction.

### 5. Content-Hash Deduplication for Bridge

**Decision**: SHA-256 hash of `category + content` string. Stored in a `HashSet<[u8; 32]>` in memory, with periodic persistence to a dedicated `cortex_bridge_hashes` column in `cortex_events` metadata.

**Rationale**: SHA-256 is collision-resistant enough for dedup. In-memory `HashSet` for fast lookups. The set is bounded (content hashes from the last 30 days, matching event pruning).

**Alternatives considered**:
- Database UNIQUE constraint — Would work for writes but doesn't cover the read-then-check-then-write pattern needed for bidirectional bridging.
- Bloom filter — Lower memory but has false positives, which would prevent legitimate cross-pollination.

### 6. Remote Feedback Mechanism

**Decision**: Add `ResponseFeedback { message_id: String, positive: bool }` variant to `DaemonEvent`. Web UI shows thumbs-up/thumbs-down buttons after each AI response. On click, sends the event over the existing WebSocket connection. Daemon maps: thumbs-up → quality 0.9, thumbs-down → quality 0.3.

**Rationale**: Minimal change to existing remote protocol. Two buttons are the simplest possible feedback mechanism. Maps directly to the `Outcome` quality scale used by `hive_learn`.

**Alternatives considered**:
- Star rating (1-5) — More granular but remote users are unlikely to rate precisely. Binary is faster.
- Edit distance tracking — Would require reconstructing user edits over WebSocket. Too complex for remote.
- No remote feedback — Would leave all remote interactions at quality 0.5 (Unknown), degrading learning signal quality.

### 7. AutoResearch Executor Provisioning

**Decision**: `LearningCortex` constructor receives `Arc<dyn AutoResearchExecutor>` from `main.rs`. The same executor instance used for manual AutoResearch runs is shared. The Cortex constructs `AutoResearchEngine::new(executor.clone(), config)` for each triggered run.

**Rationale**: Reuses existing executor infrastructure. `Arc` allows sharing across async tasks safely. No new executor types needed.

**Alternatives considered**:
- Cortex creates its own executor — Would duplicate initialization logic and provider configuration.
- Lazy initialization — Adds complexity for no benefit since the executor is available at startup.

### 8. Idle Detection

**Decision**: `LearningCortex` holds `last_user_interaction: Arc<AtomicI64>` (epoch seconds). `ChatService` calls `cortex.mark_active()` on every user message. `HiveDaemon` calls `mark_active()` on every `DaemonEvent` dispatch. The Cortex checks this timestamp before auto-applying: if `now - last_interaction < 30 seconds`, defer the change.

**Rationale**: `AtomicI64` has zero contention — single atomic store from any thread. 30-second threshold balances responsiveness (don't interrupt active use) with timeliness (don't wait too long).

**Alternatives considered**:
- GPUI focus tracking — Would only detect local UI focus, not remote activity.
- Per-conversation idle tracking — Too granular; the Cortex operates at the system level.
