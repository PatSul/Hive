# Feature Specification: Learning Cortex — Hyperagent Self-Improvement System

**Feature Branch**: `002-learning-cortex`
**Created**: 2026-03-24
**Status**: Draft
**Input**: User description: "Wire all learning subsystems (hive_learn, hive_agents CollectiveMemory, hive_remote) into a unified LearningCortex with auto-apply guardrails, meta-learning, and AutoResearch triggering."
**Design Spec**: `docs/superpowers/specs/2026-03-24-learning-cortex-design.md`

## User Scenarios & Testing *(mandatory)*

### User Story 1 - System Self-Improves Prompt Quality (Priority: P1)

The application tracks outcomes of every AI interaction (accepted, corrected, regenerated, ignored). When quality for a persona drops below threshold, the system automatically triggers a prompt improvement cycle — evaluating, mutating, and promoting better prompts without user intervention.

**Why this priority**: This is the core Hyperagent capability — autonomous prompt improvement. Without this, the system is just collecting data passively.

**Independent Test**: Can be fully tested by generating 20+ interactions with intentionally low-quality prompts, then verifying the system detects the quality drop, triggers AutoResearch, and promotes an improved prompt version.

**Acceptance Scenarios**:

1. **Given** a persona has accumulated 20+ interactions with average quality below 0.6, **When** the Cortex evaluates the next batch of events, **Then** an AutoResearch run is triggered for that persona's prompt.
2. **Given** AutoResearch produces a mutated prompt with 10%+ pass rate improvement, **When** the mutation passes security scanning, **Then** the new prompt version is promoted after a 1-hour soak period.
3. **Given** a promoted prompt causes quality to drop 15%+ during the soak period, **When** the soak monitor detects the regression, **Then** the change is automatically rolled back and the rollback is logged.

---

### User Story 2 - Cross-System Learning (Priority: P1)

Insights discovered by the agent swarm (Queen/CollectiveMemory) benefit individual AI interactions, and vice versa. A success pattern discovered during a multi-agent task improves single-user chat prompts. A high-quality code pattern from individual use becomes available to swarm agents.

**Why this priority**: This bridges the two learning silos — without it, individual and swarm learning remain disconnected, each missing half the picture.

**Independent Test**: Can be tested by running a swarm task that records a success pattern, then verifying that pattern appears as a refinement suggestion for the relevant individual persona prompt.

**Acceptance Scenarios**:

1. **Given** Queen records a SuccessPattern about a technique (e.g., "structured output improves accuracy"), **When** the bridge processes the event, **Then** the relevant persona's PromptEvolver receives a refinement suggestion with the pattern as evidence.
2. **Given** PromptEvolver promotes a new prompt version with higher quality, **When** the bridge processes the promotion event, **Then** a SuccessPattern entry appears in CollectiveMemory for swarm agents to use.
3. **Given** the same insight has already been bridged, **When** a duplicate event arrives, **Then** the bridge deduplicates by content hash and does not cross-pollinate twice.

---

### User Story 3 - Tiered Auto-Apply with Safety (Priority: P1)

The system applies improvements automatically based on blast radius — low-risk routing changes apply immediately, medium-risk prompt changes soak for 1 hour, high-risk pattern and strategy changes soak for 24 hours. Users can see what's happening and pause auto-apply.

**Why this priority**: Auto-apply is what makes this a Hyperagent rather than a suggestion engine. The guardrails make it safe for a shipping desktop app.

**Independent Test**: Can be tested by triggering improvements in each tier and verifying soak periods, rollback behavior, and the 3-change-per-day limit.

**Acceptance Scenarios**:

1. **Given** a routing tier adjustment passes the quality delta threshold (>0.05 over 20+ samples), **When** the Cortex applies it, **Then** the change takes effect immediately and is monitored for 10 subsequent interactions.
2. **Given** a prompt version promotion passes all thresholds, **When** the Cortex applies it, **Then** the change enters a 1-hour soak period before being confirmed.
3. **Given** 3 auto-applied changes have occurred in the last 24 hours without user acknowledgment, **When** a 4th improvement is ready, **Then** the system queues it and waits for user acknowledgment.
4. **Given** the user toggles the global auto-apply pause, **When** the Cortex has pending improvements, **Then** all improvements are queued and none are applied until unpaused.

---

### User Story 4 - Meta-Learning (Priority: P2)

The system tracks which improvement strategies work best and adjusts their parameters over time. If prompt mutations for a certain persona keep getting rolled back, the system reduces mutation frequency for that persona. If routing adjustments consistently succeed, the system becomes more aggressive about routing optimizations.

**Why this priority**: Meta-learning is the advanced differentiator, but the system is valuable without it. It amplifies the other stories.

**Independent Test**: Can be tested by simulating 10+ strategy applications with known success/failure ratios and verifying that strategy weights diverge from defaults.

**Acceptance Scenarios**:

1. **Given** a strategy has been applied 10+ times with a 70%+ success rate, **When** the MetaLearner reviews, **Then** the strategy weight increases above 0.5 (making it more aggressive).
2. **Given** a strategy has been applied 10+ times with a 30%- success rate, **When** the MetaLearner reviews, **Then** the strategy weight decreases below 0.5 (raising thresholds to trigger).
3. **Given** every 500 interactions, **When** the MetaLearner performs a full review, **Then** it identifies stagnant domains and shifts resources toward high-potential domains.

---

### User Story 5 - Remote Interactions Feed Learning (Priority: P2)

Users controlling Hive from remote devices (via hive_remote web UI) contribute to the learning pipeline. Remote chat completions and agent tasks publish outcome events. Remote user activity prevents auto-apply during active sessions. Remote users can provide quality feedback via thumbs-up/thumbs-down.

**Why this priority**: Without remote integration, the Cortex has a blind spot — all remote usage is invisible to the learning system.

**Independent Test**: Can be tested by completing a chat via the remote web UI and verifying an OutcomeRecorded event appears in the Cortex event log.

**Acceptance Scenarios**:

1. **Given** a remote user completes a chat via WebSocket, **When** the AI response finishes streaming, **Then** an OutcomeRecorded event is published with initial quality 0.5 (Unknown outcome).
2. **Given** a remote user clicks thumbs-up on a response, **When** the feedback event is received, **Then** the OutcomeRecorded quality is updated to 0.9.
3. **Given** a remote user is actively interacting, **When** the Cortex checks idle status, **Then** auto-apply is deferred until 30 seconds after the last remote interaction.

---

### User Story 6 - Event-Driven Observability (Priority: P3)

All learning events flow through a central event bus and are logged for inspection. Users can view Cortex activity in the Learning panel — recent changes, their outcomes, strategy weights, and the current Cortex state.

**Why this priority**: Observability is important but the system works without it. Users need to trust the Cortex before relying on it.

**Independent Test**: Can be tested by triggering any learning event and verifying it appears in the cortex_events table and the Learning panel UI.

**Acceptance Scenarios**:

1. **Given** any CortexEvent is published, **When** the Cortex receives it, **Then** the event is persisted to the cortex_events table with event type, JSON payload, and timestamp.
2. **Given** a change has been applied and is soaking, **When** the user opens the Learning panel Cortex tab, **Then** the change appears with its tier, domain, soak-until time, and current quality metrics.
3. **Given** a Red-tier change is rolled back, **When** the rollback occurs, **Then** a notification is displayed to the user explaining what was tried and why it was reverted.

---

### Edge Cases

- What happens when the event bus buffer fills during a burst of swarm events? Events are dropped silently — learning is best-effort, never blocking.
- What happens when the app closes during a soak period? On next startup, Cortex checks for Soaking entries and evaluates quality from subsequent interactions before confirming or rolling back.
- What happens when AutoResearch and a user-initiated prompt change conflict? AutoResearch queues are drained before user changes take effect; user changes always win.
- What happens when the learning database is corrupted? Cortex fails gracefully — logs the error and disables itself. The main app continues functioning without self-improvement.
- What happens when no AI interactions occur for extended periods? Strategy weights and soak periods expire naturally; the system resumes normally when interactions resume.
- What happens when both local and remote users are active simultaneously? Both contribute events; idle detection uses the most recent interaction from either source.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST observe outcomes from individual AI interactions, multi-agent swarm runs, and remote sessions through a unified event bus.
- **FR-002**: System MUST correlate events across all three sources to identify improvement opportunities.
- **FR-003**: System MUST automatically trigger prompt improvement cycles when quality degrades below configurable thresholds.
- **FR-004**: System MUST bridge insights bidirectionally between individual learning and collective learning systems.
- **FR-005**: System MUST deduplicate bridged insights by content hash to prevent redundant cross-pollination.
- **FR-006**: System MUST enforce three tiers of auto-apply safety: immediate (routing), 1-hour soak (prompts), and 24-hour soak (patterns and strategies).
- **FR-007**: System MUST automatically roll back any applied change that causes quality regression beyond the tier's threshold.
- **FR-008**: System MUST limit auto-applied changes to 3 per 24-hour window without user acknowledgment.
- **FR-009**: System MUST never auto-apply changes during active user interaction (30-second idle threshold from both local and remote sources).
- **FR-010**: System MUST track strategy effectiveness and adjust strategy parameters based on historical performance.
- **FR-011**: System MUST persist events (30-day rolling window), changes (indefinite), and strategy weights.
- **FR-012**: System MUST provide a global toggle for users to pause/resume auto-apply.
- **FR-013**: System MUST publish events from remote chat completions and agent tasks to the learning pipeline.
- **FR-014**: System MUST accept thumbs-up/thumbs-down quality feedback from the remote web UI.
- **FR-015**: System MUST scan all mutated prompts through the existing security scanner before promotion.
- **FR-016**: System MUST limit concurrent AutoResearch runs to one at a time, with a bounded queue deduplicating by skill.
- **FR-017**: System MUST recover gracefully on startup — evaluating pending soak periods and resuming operation.
- **FR-018**: System MUST never block the main UI thread — all Cortex processing runs on background tasks.

### Key Entities

- **CortexEvent**: A typed event representing a learning-relevant occurrence from any subsystem (outcome, routing decision, swarm completion, prompt mutation, etc.).
- **CortexChange**: A record of an improvement applied by the Cortex, including its tier, prior state snapshot, soak period, and outcome (Soaking/Confirmed/RolledBack).
- **Strategy**: A meta-learning record tracking the effectiveness of a specific improvement approach, including weight, attempt count, and average impact.
- **BridgedMemoryEntry**: A cross-system insight translated from one learning system's format to another's, with relevance decay applied.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The system detects and initiates improvement for 90%+ of quality degradation events within one analysis cycle.
- **SC-002**: Auto-applied improvements that survive their soak period result in measurable quality improvement (average quality delta > 0.05).
- **SC-003**: Rollback triggers within the soak window 100% of the time when quality regresses beyond the tier threshold.
- **SC-004**: Cross-system insight bridging delivers at least 1 actionable refinement suggestion per 100 swarm runs.
- **SC-005**: Meta-learning strategy weights diverge from default after 10+ strategy applications, reflecting actual effectiveness.
- **SC-006**: Remote interactions account for their proportional share of learning events.
- **SC-007**: The Cortex adds less than 1ms overhead to any user-facing interaction.
- **SC-008**: Users can view Cortex status within the existing Learning panel.

## Assumptions

- The existing AutoResearch engine, PromptEvolver, RoutingLearner, PatternLibrary, SelfEvaluator, and CollectiveMemory are functional and stable — the Cortex coordinates them, it doesn't rewrite them.
- The existing security scanner is sufficient for validating mutated prompts — no new security infrastructure needed.
- The local database is adequate for the volume of Cortex events (estimated ~1000 events/day at peak usage).
- The 256-entry event bus buffer is sufficient for normal operation; event loss during extreme bursts is acceptable since learning is best-effort.
- Remote web UI changes (thumbs-up/down buttons) are lightweight additions to the existing embedded UI — no framework changes required.
- The trait-based bridge pattern follows the existing cross-crate communication pattern in the codebase and is a proven approach for avoiding circular dependencies.
