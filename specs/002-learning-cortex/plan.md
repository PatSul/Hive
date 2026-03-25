# Implementation Plan: Learning Cortex — Hyperagent Self-Improvement System

**Branch**: `002-learning-cortex` | **Date**: 2026-03-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/002-learning-cortex/spec.md`
**Design Spec**: `docs/superpowers/specs/2026-03-24-learning-cortex-design.md`

## Summary

Wire the three learning subsystems (hive_learn individual learning, hive_agents CollectiveMemory, hive_remote interactions) into a unified LearningCortex service with tiered auto-apply guardrails, bidirectional insight bridging, meta-learning strategy optimization, and automatic AutoResearch triggering. The Cortex lives in `hive_learn/src/cortex/` and uses a trait-based bridge to avoid circular dependencies with `hive_agents`.

## Technical Context

**Language/Version**: Rust (stable, latest)
**Primary Dependencies**: tokio (async runtime, broadcast channel), serde/serde_json (event serialization), rusqlite (SQLite persistence), sha2 (content hashing for dedup)
**Storage**: SQLite (existing `hive_learn` database, 3 new tables: cortex_events, cortex_changes, cortex_strategies)
**Testing**: `cargo test` from `hive/` — unit tests in cortex modules, integration tests in external files
**Target Platform**: Windows (primary), macOS/Linux (future)
**Project Type**: Desktop app (Rust + GPUI)
**Performance Goals**: <1ms overhead on user-facing interactions; event bus non-blocking; all Cortex work on background tasks
**Constraints**: No circular dependencies between crates; `std::sync::Mutex` requires `spawn_blocking` in async contexts; max 256-entry broadcast buffer
**Scale/Scope**: ~1000 events/day at peak; 4 strategy types; 3 auto-apply tiers; 6 event source types

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Native Performance First | PASS | Cortex runs on background tokio tasks, never touches GPUI main thread. Event bus is fire-and-forget. SQLite calls via `spawn_blocking`. |
| II. Security Gate | PASS | All mutated prompts go through existing `autoresearch/security.rs` scanner. No user input in shell commands. Bridge uses trait abstraction, no direct cross-crate imports. |
| III. AI Integration Quality | PASS | Cortex improves AI quality over time via prompt evolution and routing optimization. Graceful degradation: Cortex disables itself on database errors. |
| IV. Simplicity & Elegance | PASS | 5 files in cortex module. Extends existing systems via event publishing, doesn't rewrite them. Trait-based bridge follows existing `LearningBridge` pattern. |
| V. Comprehensive Testing | PASS | Unit tests for each cortex module. Integration tests for cross-system bridging. Guardrail rollback tests. Meta-learner weight convergence tests. |
| VI. UX Consistency | PASS | Minimal UI: status bar indicator, Learning panel tab, notification on rollback. All follow existing GPUI patterns. |

No violations. No complexity tracking needed.

## Project Structure

### Documentation (this feature)

```text
specs/002-learning-cortex/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (internal interfaces)
├── checklists/          # Quality checklists
│   └── requirements.md
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
hive/crates/
  hive_learn/src/
    cortex/
      mod.rs              — LearningCortex service (event loop, correlator, decision engine)
      event_bus.rs         — CortexEvent enum, broadcast channel setup, CortexEventSender type
      guardrails.rs        — Tier enum, soak monitor, rollback logic, change tracking
      meta_learner.rs      — Strategy struct, weight updates, periodic review
      bridge.rs            — CortexBridge trait, bridging logic, dedup hash
      types.rs             — Domain, StrategyId, CortexMemoryCategory, BridgedMemoryEntry
    lib.rs                 — Add event_tx field to LearningService, publish events on on_outcome()
    prompt_evolver.rs      — Publish PromptVersionCreated on version creation
    self_evaluator.rs      — Publish SelfEvalCompleted on evaluation
    autoresearch/engine.rs — Publish SkillEvalCompleted/PromptMutated on completion

  hive_agents/src/
    queen.rs               — Publish SwarmCompleted after record_learnings()
    collective_memory.rs   — Publish CollectiveMemoryEntry on remember()

  hive_remote/src/
    daemon.rs              — Add event_tx, publish OutcomeRecorded on chat completion, call mark_active()
    web_server.rs          — Add ResponseFeedback handler

  hive_app/src/
    main.rs                — Create LearningCortex, wire event bus sender to all systems
    cortex_bridge_impl.rs  — CortexBridgeImpl (concrete impl of CortexBridge trait)

  hive_ui/src/
    workspace.rs           — Status bar cortex indicator
  hive_ui_panels/src/
    learning_panel.rs      — Cortex tab (changes, weights, toggle)

  hive_ui_core/src/
    globals.rs             — AppCortexStatus global for UI access

tests/ (external test files)
  cortex_event_bus_tests.rs
  cortex_guardrails_tests.rs
  cortex_bridge_tests.rs
  cortex_meta_learner_tests.rs
  cortex_integration_tests.rs
```

**Structure Decision**: Cortex module lives inside `hive_learn` (no new crate). Cross-crate communication via trait-based bridge (impl in `hive_app`). Event publishing added to existing crates via shared `broadcast::Sender`.
