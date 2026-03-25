# Data Model: Learning Cortex

**Date**: 2026-03-24
**Feature**: 002-learning-cortex

## Entities

### CortexEvent

A typed event representing a learning-relevant occurrence from any subsystem.

| Field | Type | Description |
|-------|------|-------------|
| variant | enum | One of: OutcomeRecorded, RoutingDecision, PromptVersionCreated, PatternExtracted, SelfEvalCompleted, SwarmCompleted, CollectiveMemoryEntry, QueenPlanGenerated, SkillEvalCompleted, PromptMutated, ImprovementApplied, ImprovementRolledBack, StrategyWeightAdjusted |
| (variant fields) | mixed | Each variant carries its own typed payload (see design spec §5) |

**Constraints**: Must derive `Debug, Clone` (required for `broadcast` channel). Uses only types from `hive_learn` — no cross-crate type imports.

**Persistence**: Serialized as JSON to `cortex_events` table with event_type and payload columns.

---

### CortexChange

A record of an improvement applied by the Cortex.

| Field | Type | Description |
|-------|------|-------------|
| change_id | String (UUID) | Unique identifier for this change |
| domain | Domain enum | Routing, Prompts, Patterns, or SwarmConfig |
| tier | Tier enum | Green (immediate), Yellow (1h soak), Red (24h soak) |
| action | JSON String | What was changed (serialized action details) |
| prior_state | JSON String | Snapshot of state before change (for rollback) |
| applied_at | i64 (epoch) | When the change was applied |
| soak_until | i64 (epoch) | When the soak period ends |
| status | Status enum | Soaking, Confirmed, or RolledBack |
| quality_before | f64 (optional) | Quality score before the change |
| quality_after | f64 (optional) | Quality score after soak period |

**State transitions**: `Soaking → Confirmed` (quality held) or `Soaking → RolledBack` (quality regressed).

---

### Strategy

A meta-learning record tracking effectiveness of an improvement approach.

| Field | Type | Description |
|-------|------|-------------|
| id | StrategyId enum | PromptMutation, TierAdjustment, PatternInjection, CrossPollination |
| domain | Domain enum | Which domain this strategy operates on |
| weight | f64 | 0.0–1.0, how much to trust this strategy |
| attempts | u32 | Total times this strategy has been applied |
| successes | u32 | Times the improvement survived soak period |
| failures | u32 | Times the improvement was rolled back |
| avg_impact | f64 | Average quality delta when successful |
| last_adjusted | i64 (epoch) | When the weight was last updated |

**Invariants**: `weight` clamped to [0.1, 1.0]. `successes + failures <= attempts`. Default weight: 0.5.

---

### BridgedMemoryEntry

A cross-system insight translated between learning systems.

| Field | Type | Description |
|-------|------|-------------|
| category | String | Memory category (SuccessPattern, FailurePattern, ModelInsight, etc.) |
| content | String | The insight content |
| relevance_score | f64 | Relevance/confidence, decayed to 0.6 for cross-context entries |
| timestamp_epoch | i64 | When the original entry was created |

**Constraints**: Uses only primitive types (no cross-crate imports). Content hash (SHA-256 of `category + content`) used for deduplication.

---

### Domain (enum)

| Variant | Description |
|---------|-------------|
| Routing | Model tier routing decisions |
| Prompts | Prompt version management |
| Patterns | Code/behavior pattern injection |
| SwarmConfig | Swarm agent configuration |

---

### StrategyId (enum)

| Variant | Description |
|---------|-------------|
| PromptMutation | AutoResearch-driven prompt rewriting |
| TierAdjustment | RoutingLearner tier upgrades/downgrades |
| PatternInjection | Adding patterns to context |
| CrossPollination | Bridge syncing between learning systems |

---

### CortexMemoryCategory (enum)

Mirrors `hive_agents::collective_memory::MemoryCategory` without importing it.

| Variant | Maps to |
|---------|---------|
| SuccessPattern | MemoryCategory::SuccessPattern |
| FailurePattern | MemoryCategory::FailurePattern |
| ModelInsight | MemoryCategory::ModelInsight |
| ConflictResolution | MemoryCategory::ConflictResolution |
| CodePattern | MemoryCategory::CodePattern |
| UserPreference | MemoryCategory::UserPreference |
| General | MemoryCategory::General |

Conversion between `CortexMemoryCategory` and `MemoryCategory` happens in `CortexBridgeImpl` (in `hive_app`).

## Relationships

```
LearningService ──event_tx──► CortexEvent bus ◄──event_tx── Queen
                                    │                         ▲
                                    ▼                         │
                              LearningCortex ──bridge──► CortexBridge trait
                                    │                         │
                                    ├──► CortexChange (persisted)
                                    ├──► Strategy (persisted)
                                    └──► AutoResearchEngine (triggered)
                                              │
                                              ▼
                                        PromptEvolver (versions updated)
```

## Database Tables

Three new tables in the existing `hive_learn` SQLite database. See design spec §10 for full DDL with indexes.

| Table | Primary Key | Purpose |
|-------|-------------|---------|
| cortex_events | id (autoincrement) | Event log (30-day rolling window) |
| cortex_changes | change_id (UUID text) | Applied improvements with rollback state |
| cortex_strategies | strategy_id (enum text) | Meta-learner strategy weights |
