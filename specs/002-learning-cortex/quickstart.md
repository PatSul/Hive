# Quickstart: Learning Cortex

## What This Feature Does

The Learning Cortex is a self-improvement system for Hive. It observes how well AI interactions perform, discovers patterns across individual and swarm usage, and automatically applies improvements — better prompts, smarter routing, richer patterns — with safety guardrails that automatically roll back changes that don't work.

## Architecture (30-second overview)

```
Events flow in from 3 sources:
  hive_learn (individual AI interactions)
  hive_agents (multi-agent swarm runs)
  hive_remote (remote device interactions)
       ↓
  LearningCortex (correlates, decides, applies)
       ↓
  Improvements flow back out:
    Better prompts (via PromptEvolver)
    Smarter routing (via RoutingLearner)
    Richer patterns (via PatternLibrary)
    Swarm insights (via CollectiveMemory bridge)
```

## Key Files

| File | Purpose |
|------|---------|
| `hive_learn/src/cortex/mod.rs` | Main Cortex service — event loop, correlator, decision engine |
| `hive_learn/src/cortex/event_bus.rs` | CortexEvent enum and channel setup |
| `hive_learn/src/cortex/guardrails.rs` | Three-tier auto-apply safety |
| `hive_learn/src/cortex/meta_learner.rs` | Strategy tracking and weight optimization |
| `hive_learn/src/cortex/bridge.rs` | CortexBridge trait for cross-crate communication |
| `hive_learn/src/cortex/types.rs` | Shared types (Domain, StrategyId, etc.) |
| `hive_app/src/cortex_bridge_impl.rs` | Concrete bridge implementation |
| `hive_app/src/main.rs` | Startup wiring |

## How to Test

```bash
# Run all cortex tests
cd hive && cargo test -p hive_learn cortex

# Run integration tests
cd hive && cargo test cortex_integration

# Full workspace test
cd hive && cargo test --workspace --exclude hive_app
```

## How the Cortex Decides

1. **Observe**: Events arrive via broadcast channel from all subsystems
2. **Correlate**: Match events to improvement opportunities (quality degradation, cross-system insights, stagnation)
3. **Trigger**: Spawn AutoResearch or queue bridge actions
4. **Apply**: Use tiered guardrails (Green/Yellow/Red) with soak periods
5. **Learn**: MetaLearner tracks which strategies work and adjusts weights
6. **Rollback**: If quality regresses during soak, automatically revert

## User Controls

- **Status bar**: Shows Cortex state (idle/processing/applied)
- **Learning panel → Cortex tab**: View changes, weights, history
- **Global toggle**: Pause/resume auto-apply
- **Notifications**: Red-tier rollbacks trigger a user notification
