# Learning Cortex — Hyperagent Self-Improvement System

**Date:** 2026-03-24
**Status:** Design
**Scope:** Wire all learning subsystems into a unified, auto-applying, meta-learning cortex

---

## 1. Problem Statement

Hive has sophisticated learning infrastructure spread across two silos:

- **`hive_learn`** — Individual learning: outcome tracking, prompt evolution, routing optimization, pattern extraction, self-evaluation, and the AutoResearch engine for autonomous prompt improvement.
- **`hive_agents` CollectiveMemory** — Swarm learning: success/failure patterns recorded by Queen after multi-agent runs.

These systems don't communicate. Improvements are suggested but not applied. AutoResearch is built but not triggered from the live system. There is no meta-learning layer to optimize the improvement process itself.

The goal is to close these gaps and create a system inspired by Meta's Hyperagent research — AI that improves its own learning process, not just task performance.

---

## 2. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Architecture | Unified LearningCortex service above both systems | Respects existing architecture; both systems stay intact; clear coordination layer |
| Autonomy | Auto-apply with guardrails | More Hyperagent-like; three-tier safety with soak periods and auto-rollback |
| Meta-learning | Included | True differentiator; adjusts strategy parameters, not code (safe) |
| Location | `hive_learn/src/cortex/` | No new crate; extends existing learning crate |
| Dependency strategy | Trait-based bridge (no circular deps) | `hive_learn` defines `CortexBridge` trait; `hive_app` provides impl that accesses `hive_agents` |
| Agent changes | Event publishing only | Queen and CollectiveMemory unchanged internally; just emit CortexEvents |
| Threading | `spawn_blocking` for SQLite calls | `LearningStorage` uses `std::sync::Mutex`; Cortex async loop must not hold it across `.await` |

---

## 3. Architecture Overview

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   hive_learn    │     │  LearningCortex  │     │  hive_agents    │
│                 │     │                  │     │                 │
│ OutcomeTracker ─┼──►  │  EventBus        │  ◄──┼─ Queen          │
│ RoutingLearner ─┼──►  │    ↓             │  ◄──┼─ CollectiveMemory│
│ PromptEvolver  ─┼──►  │  Correlator      │     │                 │
│ PatternLibrary ─┼──►  │    ↓             │     ┌─────────────────┐
│ SelfEvaluator  ─┼──►  │  DecisionEngine  │  ◄──┤  hive_remote    │
│                 │     │    ↓             │     │                 │
│                 │  ◄──┼─ Applier         │     │ HiveDaemon     ─┼──► OutcomeRecorded
│ AutoResearch   ─┼──►  │    ↓             │     │ WebSocket      ─┼──► mark_active()
│                 │     │  MetaLearner     │     │ Feedback UI    ─┼──► quality update
└─────────────────┘     └──────────────────┘     └─────────────────┘
```

The Cortex subscribes to events from three systems — `hive_learn` (individual learning), `hive_agents` (swarm learning), and `hive_remote` (remote interactions). It correlates them and emits improvement actions that flow back into the existing subsystems. It does not replace any existing component.

---

## 4. File Structure

```
hive_learn/src/cortex/
  mod.rs              — LearningCortex service (event loop, decision engine, correlator)
  event_bus.rs        — Typed broadcast channel for cross-system events
  guardrails.rs       — Auto-apply thresholds, rollback windows, security checks
  meta_learner.rs     — Tracks which strategies work, adjusts weights
  bridge.rs           — CortexBridge trait + bridging logic (trait impl lives in hive_app)
  types.rs            — Domain, StrategyId, CortexEvent, and other shared types
```

### Dependency Strategy

`hive_learn` cannot depend on `hive_agents` (the dependency goes the other direction). The bridge uses the same trait pattern as the existing `LearningBridge` trait in `hive_learn/src/learning_bridge.rs`:

```rust
// Defined in hive_learn/src/cortex/bridge.rs
trait CortexBridge: Send + Sync {
    /// Read recent entries from collective memory
    fn read_collective_entries(&self, since: i64, limit: usize) -> Vec<BridgedMemoryEntry>;
    /// Write an insight to collective memory
    fn write_to_collective(&self, category: String, content: String, relevance_score: f64) -> Result<()>;
}

// BridgedMemoryEntry uses only primitive types — no hive_agents imports.
// Maps to hive_agents::MemoryEntry fields:
//   category ↔ MemoryEntry.category (enum → string)
//   relevance_score ↔ MemoryEntry.relevance_score
//   timestamp_epoch ↔ MemoryEntry.created_at (RFC 3339 string → epoch i64, converted by CortexBridgeImpl)
struct BridgedMemoryEntry {
    category: String,
    content: String,
    relevance_score: f64,  // Maps to MemoryEntry.relevance_score (not "confidence")
    timestamp_epoch: i64,  // Converted from MemoryEntry.created_at (RFC 3339) by CortexBridgeImpl
}
```

The concrete `CortexBridgeImpl` lives in `hive_app` (which depends on both crates) and translates between `BridgedMemoryEntry` and `hive_agents::collective_memory::MemoryEntry`. This matches the existing `LearningBridge` pattern. The existing `LearningBridge` trait is orthogonal — it handles outcome/model syncing; `CortexBridge` handles the Cortex's specific cross-pollination needs.

---

## 5. Event Bus

### Transport

`tokio::sync::broadcast` channel with buffer size 256. Events are fire-and-forget — if the Cortex is busy or the buffer fills during a burst (e.g., a swarm recording many patterns), events are dropped via `RecvError::Lagged`. Learning is best-effort, never blocking the main app.

### Shared Types

These types are defined in `hive_learn/src/cortex/types.rs` to avoid importing from `hive_agents`:

```rust
/// Mirrors hive_agents::collective_memory::MemoryCategory as strings.
/// The CortexBridgeImpl in hive_app handles the conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CortexMemoryCategory {
    SuccessPattern,
    FailurePattern,
    ModelInsight,
    ConflictResolution,
    CodePattern,
    UserPreference,
    General,
}

/// What kind of improvement the Cortex can apply.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Domain {
    Routing,
    Prompts,
    Patterns,
    SwarmConfig,
}

/// Identifies a specific improvement strategy.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum StrategyId {
    PromptMutation,
    TierAdjustment,
    PatternInjection,
    CrossPollination,
}
```

### Event Types

```rust
/// All events use only types defined in hive_learn — no hive_agents imports.
/// Maps to existing hive_learn types: Outcome (not OutcomeType), QualityTrend (not Trend).
#[derive(Debug, Clone)]
enum CortexEvent {
    // From hive_learn
    OutcomeRecorded {
        interaction_id: String,
        model: String,
        quality_score: f64,
        outcome: String, // Serialized from hive_learn::Outcome (Accepted|Corrected|Regenerated|Ignored|Unknown)
    },
    RoutingDecision {
        task_type: String,
        model_chosen: String,
        tier: u8,
        quality_result: Option<f64>,
    },
    PromptVersionCreated {
        persona: String,
        version: u32,
        avg_quality: f64,
    },
    PatternExtracted {
        pattern_id: String,
        language: String,
        category: String,
        quality: f64,
    },
    SelfEvalCompleted {
        overall_quality: f64,
        trend: String, // Serialized from hive_learn::QualityTrend (Improving|Stable|Declining)
        weak_areas: Vec<String>,
    },

    // From hive_agents (published via event bus sender passed at startup)
    SwarmCompleted {
        goal_id: String,
        success: bool,
        agent_count: usize,
        duration_ms: u64,
        patterns_recorded: u32,
    },
    CollectiveMemoryEntry {
        category: CortexMemoryCategory,
        content: String,
        relevance_score: f64, // Maps to MemoryEntry.relevance_score
    },
    QueenPlanGenerated {
        goal_id: String,
        team_count: usize,
        memory_context_used: bool,
    },

    // From AutoResearch
    SkillEvalCompleted {
        skill_id: String,
        pass_rate: f64,
        iteration: u32,
    },
    PromptMutated {
        skill_id: String,
        old_pass_rate: f64,
        new_pass_rate: f64,
        promoted: bool,
    },

    // From Cortex itself (meta-events)
    ImprovementApplied {
        domain: Domain,
        action: String,
        expected_impact: f64,
    },
    ImprovementRolledBack {
        domain: Domain,
        action: String,
        reason: String,
    },
    StrategyWeightAdjusted {
        strategy: StrategyId,
        old_weight: f64,
        new_weight: f64,
    },
}
```

---

## 6. Bridge — CollectiveMemory ↔ hive_learn

Bidirectional sync that breaks the silo between individual and swarm learning.

### Collective → Individual

- **SuccessPattern** from Queen → Bridge checks if the relevant persona's PromptEvolver could benefit. If so, queues a prompt refinement suggestion with the pattern as evidence.
- **ModelInsight** from Queen (e.g., "Claude outperformed GPT on this task type") → feeds into RoutingLearner as a synthetic routing observation.

### Individual → Collective

- **Prompt version promoted** with higher quality → Bridge writes a SuccessPattern to CollectiveMemory so swarm agents benefit from the same insight.
- **High-quality code pattern** extracted by PatternLibrary → becomes available to swarm agents via CollectiveMemory lookup.

### Sync Rules

- Bridge runs on Cortex event loop, not on hot path.
- Deduplication by content hash — same insight doesn't cross-pollinate twice.
- Relevance decay — bridged insights start at 0.6 relevance_score (not 1.0) since they're cross-context.
- SQLite transactions for atomicity.

### Example Flow

```
Queen completes swarm run
    → records "structured output prompts boost accuracy" to CollectiveMemory
    → Bridge picks it up via CortexEvent::CollectiveMemoryEntry
    → Checks PromptEvolver: does any persona use unstructured output?
    → If yes: queues refinement with evidence
    → AutoResearch evaluates the refinement
    → If pass rate improves: auto-promoted (guardrails permitting)
```

---

## 7. Guardrails — Auto-Apply Safety

### Three Tiers by Blast Radius

| Tier | Domain | Auto-apply? | Threshold | Rollback |
|---|---|---|---|---|
| **Green** | Routing tier adjustments | Yes, immediate | Quality delta > 0.05 over 20+ samples | Instant revert if next 10 interactions degrade |
| **Yellow** | Prompt version promotion | Yes, with delay | Pass rate improvement > 10%, security scan clean, 1-hour soak period | Auto-rollback if quality drops > 15% in soak |
| **Red** | Pattern injection into context, strategy weight changes | Yes, with extended soak | 24-hour soak, quality must stay stable across 50+ interactions | Auto-rollback + user notification |

### Hard Stops (Never Auto-Apply)

- Any change that would affect the SecurityGateway.
- Prompt mutations that fail the existing `autoresearch/security.rs` injection scan.
- Changes during active user conversation (wait for idle). Idle is detected via a new `last_user_interaction: Arc<AtomicI64>` field on `LearningCortex`, updated by `ChatService` (in `hive_ui`) via a public `cortex.mark_active()` method called on every user message submission. Idle threshold: 30 seconds since last interaction.
- More than 3 auto-applied changes in a 24-hour window without user acknowledgment.

### Rollback Mechanics

- Every applied change gets a `change_id` and snapshot of prior state.
- Cortex monitors quality metrics for a configurable soak window after each change.
- If quality regresses beyond threshold, automatic rollback fires and logs the reason.
- All rollbacks generate a `CortexEvent::ImprovementRolledBack` that feeds back into meta-learning ("this type of change tends to fail").

### User Visibility

- Status bar indicator when Cortex has pending/applied changes.
- Learning panel shows change history with outcomes.
- User can pause auto-apply globally with one toggle.

---

## 8. Meta-Learner — Learning About Learning

The MetaLearner doesn't improve tasks — it improves how the system improves.

### Strategy Model

```rust
struct Strategy {
    id: StrategyId,
    domain: Domain,           // Routing, Prompts, Patterns, SwarmConfig
    weight: f64,              // 0.0-1.0, how much to trust this strategy
    attempts: u32,
    successes: u32,           // improvement held through soak period
    failures: u32,            // rolled back or quality degraded
    avg_impact: f64,          // average quality delta when successful
    last_adjusted: i64,       // Unix epoch seconds (consistent with cortex table timestamps)
}
```

### Managed Strategies

| Strategy | What it does | Example adjustment |
|---|---|---|
| `PromptMutation` | AutoResearch rewrites prompts based on failures | If mutations keep getting rolled back for a persona, reduce mutation frequency and increase sample size before triggering |
| `TierAdjustment` | RoutingLearner upgrades/downgrades models | If tier downgrades consistently fail, raise the quality threshold for downgrade recommendations |
| `PatternInjection` | Adding extracted patterns to context | If injected patterns don't improve quality, narrow extraction to higher-quality-only (0.9+ instead of 0.8+) |
| `CrossPollination` | Bridge syncing insights between systems | If bridged insights have low relevance outcomes, increase the relevance decay factor |

### The Meta-Learning Loop

```
Strategy applied → soak period → outcome measured
    ↓
MetaLearner updates strategy weight:
    success → weight *= 1.1 (capped at 1.0)
    failure → weight *= 0.7 (floored at 0.1)
    ↓
Weight influences future decisions:
    low weight → higher thresholds to trigger
    high weight → lower thresholds, faster application
    ↓
Every 500 interactions: MetaLearner reviews all weights
    → detects which domains are improving vs stagnant
    → shifts resources toward high-potential domains
```

### Safety Constraint

Meta-learning adjusts parameters (thresholds, frequencies, confidence levels), not code. The worst a bad meta-learning decision can do is make improvements slower, never break functionality.

---

## 9. AutoResearch Wiring

AutoResearch is already built (`hive_learn/src/autoresearch/`). The gap is triggering it from the live system.

### Trigger Conditions (any of these)

1. **Quality degradation** — PromptEvolver reports persona average quality < 0.6 over 20+ samples.
2. **Weak area detected** — SelfEvaluator flags a weak area; Cortex maps it to relevant skill prompts.
3. **Cross-pollinated insight** — Bridge delivers an insight; Cortex runs AutoResearch to validate it actually improves the target prompt before promoting.
4. **Stagnation detected** — MetaLearner detects a stagnant domain; Cortex triggers exploratory AutoResearch runs with broader mutation parameters.

### Integration

```
LearningCortex
    → checks trigger conditions on each relevant CortexEvent
    → spawns AutoResearchEngine::run() as background task
    → AutoResearch publishes SkillEvalCompleted / PromptMutated events
    → Cortex receives those events
    → If promoted: applies via Yellow-tier guardrails (1-hour soak)
    → MetaLearner records whether the mutation held
```

### Resource Management

- Only one AutoResearch run at a time. Additional triggers queued in a `VecDeque<AutoResearchTrigger>` (max depth: 10). Duplicate triggers for the same skill_id are collapsed — only the most recent trigger is kept.
- Budget cap per 24-hour period (configurable, default: 20 eval runs).
- Runs only during idle periods (no active user conversation). Idle is detected via a `last_user_interaction` timestamp updated by the chat service; idle threshold: 30 seconds of no user input.
- Each run bounded by existing AutoResearch config: max iterations, plateau detection.

No changes to AutoResearch internals — just a new caller (the Cortex) and event publishing from its existing outcome paths.

### Executor Provisioning

`AutoResearchEngine<E>` is generic over `E: AutoResearchExecutor`. The Cortex needs an executor instance to spawn runs. At startup in `main.rs`, the same executor used for manual AutoResearch is wrapped in `Arc<dyn AutoResearchExecutor>` and passed to the `LearningCortex` constructor. The Cortex stores it as `executor: Arc<dyn AutoResearchExecutor>` and constructs `AutoResearchEngine::new(executor.clone(), ...)` for each triggered run.

---

## 10. Persistence

Three new tables in the existing `hive_learn` SQLite database.

```sql
-- Cortex event log (bounded, rolling 30-day window)
-- Note: uses INTEGER (epoch seconds) for timestamps, not TEXT (RFC 3339) like
-- existing hive_learn tables. Cortex tables are self-contained; cross-table
-- correlation with existing tables uses epoch conversion.
CREATE TABLE IF NOT EXISTS cortex_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,        -- JSON serialized CortexEvent
    timestamp INTEGER NOT NULL    -- Unix epoch seconds
);
CREATE INDEX IF NOT EXISTS idx_cortex_events_timestamp ON cortex_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_cortex_events_type ON cortex_events(event_type);

-- Applied improvements with rollback state
CREATE TABLE IF NOT EXISTS cortex_changes (
    change_id TEXT PRIMARY KEY,
    domain TEXT NOT NULL,          -- Routing | Prompts | Patterns | SwarmConfig
    tier TEXT NOT NULL,            -- Green | Yellow | Red
    action TEXT NOT NULL,          -- JSON: what was changed
    prior_state TEXT NOT NULL,     -- JSON: snapshot for rollback
    applied_at INTEGER NOT NULL,   -- Unix epoch seconds
    soak_until INTEGER NOT NULL,   -- Unix epoch seconds
    status TEXT NOT NULL,          -- Soaking | Confirmed | RolledBack
    quality_before REAL,
    quality_after REAL
);
CREATE INDEX IF NOT EXISTS idx_cortex_changes_status ON cortex_changes(status);

-- Meta-learner strategy weights
CREATE TABLE IF NOT EXISTS cortex_strategies (
    strategy_id TEXT PRIMARY KEY,
    domain TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 0.5,
    attempts INTEGER NOT NULL DEFAULT 0,
    successes INTEGER NOT NULL DEFAULT 0,
    failures INTEGER NOT NULL DEFAULT 0,
    avg_impact REAL NOT NULL DEFAULT 0.0,
    last_adjusted INTEGER NOT NULL  -- Unix epoch seconds
);
```

### Database Notes

- All three tables live in the existing `hive_learn` SQLite database (same file as `learning_outcomes`, `routing_history`, etc.).
- `CollectiveMemory` uses its own separate SQLite database (`CollectiveMemory::open()`). The `CortexBridge` trait abstracts access to it — the Cortex never opens CollectiveMemory's database directly.
- All Cortex SQLite operations run via `tokio::task::spawn_blocking` since `LearningStorage` uses `std::sync::Mutex<rusqlite::Connection>` (not async-safe).

### Startup Recovery

When the app launches, Cortex reads `cortex_changes` for any changes still in `Soaking` status. If the soak period expired while the app was closed, it evaluates quality from the next few interactions before confirming or rolling back.

### Pruning

- `cortex_events` auto-prunes entries older than 30 days.
- `cortex_changes` keeps confirmed/rolled-back entries indefinitely (small and valuable for meta-learning).

---

## 11. Relationship to Queen

The Queen and LearningCortex operate at different levels:

| | Queen | LearningCortex |
|---|---|---|
| **Purpose** | Execute tasks via agent swarms | Improve the system's ability to execute tasks |
| **Trigger** | User goal ("build X", "research Y") | Accumulated outcomes (every 50-200 interactions, swarm completions) |
| **Output** | Task results (code, analysis, plans) | Better prompts, routing, patterns, strategy weights |
| **Learning** | Side effect — records what worked after each run | Primary job — analyzes patterns across all systems |
| **Scope** | One swarm run at a time | Cross-cutting: individual sessions + swarm runs + routing history |

The Queen doesn't change. She keeps orchestrating swarms and recording outcomes. The Cortex consumes her output and feeds improvements back into her prompts and the broader system.

```
User goal arrives
    → Queen plans & executes swarm
    → Queen records outcomes → CollectiveMemory
    → LearningCortex observes (CollectiveMemory + hive_learn + AutoResearch)
    → LearningCortex improves (Queen prompts, agent roles, routing, patterns)
    → Next time Queen runs → she's better
```

---

## 12. Remote Integration (hive_remote)

`hive_remote` provides a daemon + web UI for controlling Hive from remote devices over WebSocket. Remote interactions (chat, agent tasks, approvals) currently bypass the learning pipeline — they record to `ActivityLog` but never reach `LearningService::on_outcome()`. The Cortex must not be blind to remote usage.

### 12.1 Remote Outcomes → Cortex Events

The `HiveDaemon` gains an `event_tx: Option<broadcast::Sender<CortexEvent>>` field (same sender type as `LearningService`). On relevant events:

| Remote Event | Cortex Event Published |
|---|---|
| `complete_stream()` finishes (chat response) | `OutcomeRecorded { outcome: "Unknown", model, quality_score: 0.5 }` — default until feedback received |
| `DaemonEvent::StartAgentTask` completes | `OutcomeRecorded { outcome: "Accepted"/"Corrected" based on AgentCompleted/AgentFailed }` |
| Remote user sends explicit feedback (see 12.3) | `OutcomeRecorded` with actual outcome quality |

Agent completions map to quality: `AgentCompleted` → `Accepted` (quality 0.8), `AgentFailed` → `Corrected` (quality 0.3). These are coarser than local outcomes but still valuable for routing optimization.

### 12.2 Remote Activity → Idle Detection

Every `DaemonEvent` received from a WebSocket client calls `cortex.mark_active()`. This prevents auto-apply during remote sessions. The daemon already runs in its own tokio runtime, so the `mark_active()` call is a single `AtomicI64::store()` — zero contention.

### 12.3 Remote Feedback Mechanism

The web UI currently has no way for remote users to signal outcome quality. Add a lightweight feedback mechanism:

- After each AI response in the remote web UI, show thumbs-up/thumbs-down buttons.
- Clicking sends a new `DaemonEvent::ResponseFeedback { message_id, positive: bool }`.
- Daemon maps: thumbs-up → `Accepted` (quality 0.9), thumbs-down → `Corrected` (quality 0.3).
- Publishes updated `OutcomeRecorded` event to replace the initial `Unknown` score.

This is intentionally simple — two buttons, one event. No edit-distance tracking or regeneration tracking for remote (those require the full local UI).

### 12.4 Learning Panel over WebSocket

The remote web UI should expose read-only Cortex status. Add a `DaemonEvent::RequestLearningStatus` that returns:
- Current Cortex state (idle/processing/applied)
- Count of changes in soak period
- Auto-apply enabled/disabled

This is low priority — remote users can see the Learning panel locally. Include it for completeness but defer implementation if needed.

---

## 13. Changes to Existing Code

### hive_agents (minimal — event publishing only)

- `queen.rs` — After `record_learnings()` (called from within `execute()`), publish `SwarmCompleted` and relevant `CollectiveMemoryEntry` events to the Cortex event bus sender (passed at construction).
- `collective_memory.rs` — On `remember()`, publish `CollectiveMemoryEntry` event to the event bus sender.

### hive_learn (extend, don't modify)

- `lib.rs` — LearningService gains a `event_tx: Option<broadcast::Sender<CortexEvent>>` field. On `on_outcome()`, calls `event_tx.send()` (sync — `broadcast::Sender::send()` is not async, so no change to the sync `on_outcome()` signature). On routing analysis, publishes `RoutingDecision`. Existing behavior unchanged.
- `prompt_evolver.rs` — On version creation, publishes `PromptVersionCreated`.
- `self_evaluator.rs` — On evaluation, publishes `SelfEvalCompleted`.
- `autoresearch/engine.rs` — On eval completion, publishes `SkillEvalCompleted`/`PromptMutated`.

### hive_remote (event publishing + feedback UI)

- `daemon.rs` — `HiveDaemon` gains `event_tx: Option<broadcast::Sender<CortexEvent>>`. On `complete_stream()` completion, publishes `OutcomeRecorded`. On every `DaemonEvent` dispatch, calls `cortex.mark_active()`.
- `web_server.rs` — Add `ResponseFeedback` handler that publishes updated `OutcomeRecorded` with real quality score.
- Web UI (embedded HTML/JS) — Add thumbs-up/thumbs-down buttons after AI responses, sending `ResponseFeedback` event over WebSocket.

### hive_app (startup wiring)

- `main.rs` — Create `LearningCortex`, pass event bus sender to `LearningService`, `hive_agents`, and `HiveDaemon` via shared channel.

---

## 13. Success Criteria

1. **Events flow end-to-end** — CortexEvents published by both hive_learn and hive_agents are received and logged by the Cortex.
2. **Bridge syncs insights** — A Queen success pattern triggers a PromptEvolver refinement suggestion. A promoted prompt version appears in CollectiveMemory.
3. **AutoResearch triggers automatically** — Quality degradation below threshold kicks off an AutoResearch run without manual intervention.
4. **Guardrails hold** — Green-tier changes auto-apply immediately. Yellow-tier changes soak for 1 hour. Red-tier changes soak for 24 hours. Quality regression triggers rollback.
5. **Meta-learning adjusts strategy weights** — After 10+ strategy applications, weights diverge from default 0.5 based on success/failure history.
6. **User can see and control** — Learning panel shows Cortex changes. Global toggle pauses auto-apply. Status bar reflects Cortex state.
7. **No performance regression** — Event bus is non-blocking. Cortex runs on background task. Main UI thread unaffected.
8. **Remote interactions feed the Cortex** — Chat completions and agent tasks via hive_remote publish `OutcomeRecorded` events. Remote user activity resets idle detection. Thumbs-up/down feedback updates quality scores.

---

## 14. UI Integration Notes

UI changes are minimal — extending existing surfaces, not creating new ones.

- **Status bar:** Add a Cortex status indicator to the existing `sync_status_bar()` in `hive_ui`. Shows: idle / processing / applied (with count). Clicking opens the Learning panel.
- **Learning panel:** Already exists (`LearningPanel` in `hive_ui_panels`). Add a "Cortex" tab showing: recent changes (from `cortex_changes` table), strategy weights, and a global auto-apply toggle (persisted to `hive_core::Config`).
- **Notifications:** On Red-tier rollback, emit a GPUI notification (existing notification system) so the user knows something was tried and reverted.

These UI details are intentionally lightweight — the Cortex is primarily a backend system. Full UI design for the Cortex tab can be refined during implementation.
