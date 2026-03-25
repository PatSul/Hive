# Tasks: Learning Cortex — Hyperagent Self-Improvement System

**Input**: Design documents from `/specs/002-learning-cortex/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Included per constitution requirement (Principle V: Comprehensive Testing).

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup

**Purpose**: Create the cortex module structure and shared types

- [x] T001 Create cortex module directory and mod.rs skeleton in hive/crates/hive_learn/src/cortex/mod.rs
- [x] T002 [P] Define shared types (Domain, StrategyId, CortexMemoryCategory, BridgedMemoryEntry) in hive/crates/hive_learn/src/cortex/types.rs
- [x] T003 [P] Define CortexEvent enum with all variants and derive Debug, Clone, Serialize, Deserialize in hive/crates/hive_learn/src/cortex/event_bus.rs
- [x] T004 [P] Add tokio and sha2 dependencies to hive/crates/hive_learn/Cargo.toml
- [x] T005 Export cortex module from hive/crates/hive_learn/src/lib.rs

---

## Phase 2: Foundational (Event Bus + Persistence)

**Purpose**: Core event infrastructure that ALL user stories depend on

**CRITICAL**: No user story work can begin until this phase is complete

- [x] T006 Implement broadcast channel creation (buffer 256) and CortexEventSender type alias in hive/crates/hive_learn/src/cortex/event_bus.rs
- [x] T007 Add cortex_events, cortex_changes, cortex_strategies tables (CREATE TABLE IF NOT EXISTS with indexes) to LearningStorage initialization in hive/crates/hive_learn/src/storage.rs
- [x] T008 Implement CortexStorage helper (insert_event, insert_change, update_change_status, load_strategies, save_strategy, prune_old_events, load_soaking_changes) in hive/crates/hive_learn/src/cortex/mod.rs
- [x] T009 Add event_tx: Option<broadcast::Sender<CortexEvent>> field to LearningService and publish OutcomeRecorded on on_outcome() in hive/crates/hive_learn/src/lib.rs
- [x] T010 [P] Publish PromptVersionCreated on version creation in hive/crates/hive_learn/src/prompt_evolver.rs
- [x] T011 [P] Publish SelfEvalCompleted on evaluation in hive/crates/hive_learn/src/self_evaluator.rs
- [x] T012 [P] Publish SkillEvalCompleted and PromptMutated on completion in hive/crates/hive_learn/src/autoresearch/engine.rs
- [x] T013 [P] Publish SwarmCompleted after record_learnings() and CollectiveMemoryEntry on remember() — add event_tx to Queen in hive/crates/hive_agents/src/queen.rs
- [x] T014 [P] Publish CollectiveMemoryEntry on remember() — add event_tx to CollectiveMemory in hive/crates/hive_agents/src/collective_memory.rs
- [x] T015 Implement LearningCortex skeleton: constructor, async event loop (tokio::spawn), event persistence, soak check timer (60s interval) in hive/crates/hive_learn/src/cortex/mod.rs
- [x] T016 Write event bus unit tests (send/receive, buffer overflow, multi-producer) in hive/crates/hive_learn/src/cortex/event_bus.rs (inline)
- [ ] T017 Wire LearningCortex creation in main.rs — create broadcast channel, pass sender to LearningService, Queen, CollectiveMemory in hive/crates/hive_app/src/main.rs

**Checkpoint**: Events flow end-to-end from all producers to the Cortex and are persisted to SQLite

---

## Phase 3: User Story 1 - System Self-Improves Prompt Quality (Priority: P1)

**Goal**: Cortex detects quality degradation and automatically triggers AutoResearch to improve prompts, with soak-period verification and rollback.

**Independent Test**: Generate 20+ low-quality interactions, verify AutoResearch triggers, prompt improves, and rollback fires on regression.

### Implementation for User Story 1

- [x] T018 [US1] Implement Guardrails module: Tier enum (Green/Yellow/Red), soak period logic, quality monitoring, rollback mechanics in hive/crates/hive_learn/src/cortex/guardrails.rs
- [x] T019 [US1] Implement idle detection: last_user_interaction AtomicI64, mark_active() method on LearningCortex in hive/crates/hive_learn/src/cortex/mod.rs
- [x] T020 [US1] Implement AutoResearch trigger logic: quality threshold check, executor provisioning (Arc<dyn AutoResearchExecutor>), queue (VecDeque, max 10, dedup by skill_id), budget cap in hive/crates/hive_learn/src/cortex/mod.rs
- [x] T021 [US1] Implement decision engine: correlate OutcomeRecorded events, detect quality degradation (<0.6 over 20+ samples), spawn AutoResearch background task in hive/crates/hive_learn/src/cortex/mod.rs
- [x] T022 [US1] Implement soak monitor: periodic check (60s) of cortex_changes with Soaking status, quality comparison, auto-rollback on regression, auto-confirm on success in hive/crates/hive_learn/src/cortex/guardrails.rs
- [x] T023 [US1] Implement hard stops: SecurityGateway check, security scanner validation, idle check, 3-per-24h limit in hive/crates/hive_learn/src/cortex/guardrails.rs
- [x] T024 [US1] Implement startup recovery: load Soaking changes on init, handle expired soak periods in hive/crates/hive_learn/src/cortex/mod.rs
- [x] T025 [US1] Write guardrails unit tests (tier assignment, soak timing, rollback trigger, hard stops, startup recovery) in hive/crates/hive_learn/tests/cortex_guardrails_tests.rs
- [ ] T026 [US1] Pass AutoResearchExecutor to LearningCortex constructor in hive/crates/hive_app/src/main.rs

**Checkpoint**: Cortex detects quality drops, triggers AutoResearch, applies with soak, rolls back on regression

---

## Phase 4: User Story 2 - Cross-System Learning (Priority: P1)

**Goal**: Insights bridge bidirectionally between hive_learn (individual) and CollectiveMemory (swarm), with deduplication.

**Independent Test**: Run a swarm task that records a success pattern, verify it appears as a refinement suggestion for the relevant persona.

### Implementation for User Story 2

- [x] T027 [US2] Define CortexBridge trait (read_collective_entries, write_to_collective, content_hash_exists) in hive/crates/hive_learn/src/cortex/bridge.rs
- [x] T028 [US2] Implement content-hash deduplication (SHA-256 of category+content, HashSet<[u8;32]>) in hive/crates/hive_learn/src/cortex/bridge.rs
- [x] T029 [US2] Implement bridging logic: Collective→Individual (SuccessPattern→PromptEvolver, ModelInsight→RoutingLearner) and Individual→Collective (promoted versions, high-quality patterns) in hive/crates/hive_learn/src/cortex/bridge.rs
- [x] T030 [US2] Implement relevance decay (bridged entries start at 0.6) in hive/crates/hive_learn/src/cortex/bridge.rs
- [ ] T031 [US2] Create CortexBridgeImpl (concrete impl accessing CollectiveMemory, converting CortexMemoryCategory↔MemoryCategory, RFC3339↔epoch timestamps) in hive/crates/hive_app/src/cortex_bridge_impl.rs
- [ ] T032 [US2] Wire CortexBridgeImpl into LearningCortex constructor, integrate bridge processing into event loop in hive/crates/hive_app/src/main.rs
- [x] T033 [US2] Write bridge unit tests (dedup hash, relevance decay, bidirectional sync, duplicate rejection) in hive/crates/hive_learn/tests/cortex_bridge_tests.rs

**Checkpoint**: Swarm insights improve individual prompts; individual improvements are available to swarm agents

---

## Phase 5: User Story 3 - Tiered Auto-Apply with Safety (Priority: P1)

**Goal**: Full guardrails system with all three tiers, user visibility, and global toggle.

**Independent Test**: Trigger improvements in each tier, verify soak periods, rollback, and 3-per-day limit.

### Implementation for User Story 3

- [ ] T034 [US3] Add AppCortexStatus global (idle/processing/applied + change count) in hive/crates/hive_ui_core/src/globals.rs
- [ ] T035 [US3] Update LearningCortex to publish status changes to AppCortexStatus global in hive/crates/hive_learn/src/cortex/mod.rs
- [ ] T036 [US3] Add auto_apply_enabled config field to hive_core::Config, persist to ~/.hive/config.toml in hive/crates/hive_core/src/config.rs
- [ ] T037 [US3] Add Cortex status indicator to status bar in sync_status_bar() in hive/crates/hive_ui/src/workspace.rs
- [ ] T038 [US3] Add Cortex tab to LearningPanel (recent changes list, strategy weights display, auto-apply toggle) in hive/crates/hive_ui_panels/src/learning_panel.rs
- [ ] T039 [US3] Add GPUI notification on Red-tier rollback using existing notification system in hive/crates/hive_ui/src/workspace.rs

**Checkpoint**: Users can see Cortex activity, pause auto-apply, and receive rollback notifications

---

## Phase 6: User Story 4 - Meta-Learning (Priority: P2)

**Goal**: System tracks which improvement strategies work and adjusts their parameters over time.

**Independent Test**: Simulate 10+ strategy applications with known outcomes, verify weights diverge from default 0.5.

### Implementation for User Story 4

- [ ] T040 [US4] Implement Strategy struct and MetaLearner (weight updates: success *=1.1 capped 1.0, failure *=0.7 floored 0.1) in hive/crates/hive_learn/src/cortex/meta_learner.rs
- [ ] T041 [US4] Implement periodic review (every 500 interactions): detect stagnant domains, shift resources in hive/crates/hive_learn/src/cortex/meta_learner.rs
- [ ] T042 [US4] Implement strategy weight influence on guardrails thresholds (low weight → higher threshold, high weight → lower threshold) in hive/crates/hive_learn/src/cortex/guardrails.rs
- [ ] T043 [US4] Integrate MetaLearner into Cortex event loop: record outcomes on ImprovementApplied/RolledBack, load/save weights via CortexStorage in hive/crates/hive_learn/src/cortex/mod.rs
- [ ] T044 [US4] Write meta-learner unit tests (weight convergence, stagnation detection, threshold influence, persistence round-trip) in hive/crates/hive_agents/tests/cortex_meta_learner_tests.rs

**Checkpoint**: Strategy weights evolve based on empirical outcomes; stagnant domains detected

---

## Phase 7: User Story 5 - Remote Interactions Feed Learning (Priority: P2)

**Goal**: Remote chat completions and agent tasks publish events. Remote feedback (thumbs-up/down) updates quality scores. Remote activity defers auto-apply.

**Independent Test**: Complete a chat via remote web UI, verify OutcomeRecorded appears in cortex_events table.

### Implementation for User Story 5

- [x] T045 [US5] Add event_tx to HiveDaemon constructor, publish OutcomeRecorded on complete_stream() completion in hive/crates/hive_remote/src/daemon.rs
- [x] T046 [US5] Call cortex.mark_active() on every DaemonEvent dispatch in hive/crates/hive_remote/src/daemon.rs
- [x] T047 [US5] Add ResponseFeedback { message_id, positive } variant to DaemonEvent, implement handler that publishes updated OutcomeRecorded in hive/crates/hive_remote/src/daemon.rs
- [x] T048 [US5] Add thumbs-up/thumbs-down buttons to remote web UI after AI responses, send ResponseFeedback over WebSocket in hive/crates/hive_remote/src/web_server.rs (embedded HTML/JS)
- [ ] T049 [US5] Wire event_tx to HiveDaemon in hive/crates/hive_app/src/main.rs
- [x] T050 [US5] Map remote agent completions to quality scores (AgentCompleted→0.8, AgentFailed→0.3) in hive/crates/hive_remote/src/daemon.rs

**Checkpoint**: Remote interactions visible in Cortex; feedback updates quality; idle detection includes remote

---

## Phase 8: User Story 6 - Event-Driven Observability (Priority: P3)

**Goal**: All events logged and inspectable. Learning panel shows Cortex activity.

**Independent Test**: Trigger any learning event, verify it appears in cortex_events table and Learning panel.

### Implementation for User Story 6

- [ ] T051 [US6] Implement event log query methods (by type, by time range, with pagination) in CortexStorage in hive/crates/hive_learn/src/cortex/mod.rs
- [ ] T052 [US6] Implement 30-day event pruning (run on startup and daily) in hive/crates/hive_learn/src/cortex/mod.rs
- [ ] T053 [US6] Add change history view (domain, tier, status, quality delta) to Cortex tab in LearningPanel in hive/crates/hive_ui_panels/src/learning_panel.rs
- [ ] T054 [US6] Add strategy weight visualization to Cortex tab in LearningPanel in hive/crates/hive_ui_panels/src/learning_panel.rs

**Checkpoint**: Full observability — users can inspect all Cortex activity

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Integration testing, security validation, performance verification

- [ ] T055 Write end-to-end integration test: event flow from all 3 sources → Cortex → improvement applied → soak → confirm/rollback in hive/crates/hive_agents/tests/cortex_integration_tests.rs
- [ ] T056 [P] Add ChatService integration: call cortex.mark_active() on user message submission in hive/crates/hive_ui/src/workspace.rs
- [ ] T057 [P] Security review: verify all mutated prompts pass security scanner, no user input in shell contexts, bridge uses trait abstraction
- [ ] T058 [P] Performance validation: verify Cortex event loop adds <1ms overhead, event bus is non-blocking, UI thread unaffected
- [ ] T059 Run full workspace test suite: cargo test --workspace --exclude hive_app

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS all user stories
- **US1 (Phase 3)**: Depends on Foundational — can start after Phase 2
- **US2 (Phase 4)**: Depends on Foundational — can run in parallel with US1
- **US3 (Phase 5)**: Depends on US1 (needs guardrails to display) — starts after Phase 3
- **US4 (Phase 6)**: Depends on US1 (needs guardrails to influence) — starts after Phase 3
- **US5 (Phase 7)**: Depends on Foundational — can run in parallel with US1/US2
- **US6 (Phase 8)**: Depends on US3 (needs Learning panel Cortex tab) — starts after Phase 5
- **Polish (Phase 9)**: Depends on all user stories being complete

### User Story Dependencies

```
Phase 1 (Setup) → Phase 2 (Foundational)
                        ├──→ US1 (Phase 3) ──→ US3 (Phase 5) ──→ US6 (Phase 8)
                        │                  └──→ US4 (Phase 6)
                        ├──→ US2 (Phase 4)
                        └──→ US5 (Phase 7)
                                                    All ──→ Phase 9 (Polish)
```

### Parallel Opportunities

**After Phase 2 completes, these can run in parallel:**
- US1 (Prompt self-improvement) + US2 (Cross-system learning) + US5 (Remote integration)

**After US1 (Phase 3) completes:**
- US3 (Auto-apply UI) + US4 (Meta-learning) can run in parallel

**Within each phase, [P] tasks can run in parallel**

---

## Parallel Example: Phase 2 (Foundational)

```bash
# Launch all event publishing tasks together (different files):
Task: T010 "Publish PromptVersionCreated in prompt_evolver.rs"
Task: T011 "Publish SelfEvalCompleted in self_evaluator.rs"
Task: T012 "Publish SkillEvalCompleted/PromptMutated in autoresearch/engine.rs"
Task: T013 "Publish SwarmCompleted in queen.rs"
Task: T014 "Publish CollectiveMemoryEntry in collective_memory.rs"
```

## Parallel Example: After Phase 2

```bash
# Launch three independent user stories simultaneously:
Agent 1: US1 tasks (T018-T026) — Prompt self-improvement
Agent 2: US2 tasks (T027-T033) — Cross-system learning
Agent 3: US5 tasks (T045-T050) — Remote integration
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T005)
2. Complete Phase 2: Foundational (T006-T017)
3. Complete Phase 3: User Story 1 (T018-T026)
4. **STOP and VALIDATE**: Verify Cortex detects quality drops, triggers AutoResearch, applies with soak, rolls back on regression
5. This alone delivers core Hyperagent capability

### Incremental Delivery

1. Setup + Foundational → Events flow end-to-end
2. Add US1 → Prompt self-improvement works (MVP!)
3. Add US2 → Cross-system learning bridges silos
4. Add US3 → Users can see and control Cortex
5. Add US4 → Meta-learning amplifies improvements
6. Add US5 → Remote interactions feed learning
7. Add US6 → Full observability
8. Polish → Integration tests, security review, performance validation

### Agent Team Strategy

With parallel agent execution:

1. All agents: Complete Setup (Phase 1) together
2. All agents: Complete Foundational (Phase 2) together
3. Once Foundational is done:
   - Agent A: US1 (prompt self-improvement) — 9 tasks
   - Agent B: US2 (cross-system learning) — 7 tasks
   - Agent C: US5 (remote integration) — 6 tasks
4. After US1 completes:
   - Agent A: US3 (auto-apply UI) — 6 tasks
   - Agent B: US4 (meta-learning) — 5 tasks
5. After US3 completes:
   - Agent A: US6 (observability) — 4 tasks
6. All agents: Polish (Phase 9) — 5 tasks

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Constitution requires tests (Principle V) — included in each phase
- External test files for hive_ui_panels to avoid rustc stack overflow
- All SQLite operations via spawn_blocking (std::sync::Mutex safety)
- Commit after each task or logical group
