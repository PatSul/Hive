# Contract: CortexEvent Bus

**Purpose**: Central event channel connecting all learning subsystems to the LearningCortex.

## Interface

```
type CortexEventSender = broadcast::Sender<CortexEvent>
type CortexEventReceiver = broadcast::Receiver<CortexEvent>

CortexEventSender
├── send(event: CortexEvent) → Result<usize, SendError>  [sync, non-blocking]
└── subscribe() → CortexEventReceiver

CortexEventReceiver
└── recv() → Result<CortexEvent, RecvError>  [async, blocks until event or Lagged]
```

## Contract Rules

1. **Buffer size**: 256 entries. When full, oldest events are dropped and receivers get `RecvError::Lagged`.
2. **Fire-and-forget**: Producers MUST ignore `SendError` (no receivers) — this is expected during startup/shutdown.
3. **Non-blocking**: `send()` is synchronous and never blocks. Safe to call from sync functions like `LearningService::on_outcome()`.
4. **Ownership**: `main.rs` creates the channel, passes `Sender` clones to all producers, gives one `Receiver` to the Cortex.
5. **Serialization**: Events MUST derive `Debug, Clone`. Events are serialized to JSON for persistence but transmitted in-memory as typed values.

## Producers

| Component | Events Published |
|-----------|-----------------|
| LearningService | OutcomeRecorded, RoutingDecision |
| PromptEvolver | PromptVersionCreated |
| SelfEvaluator | SelfEvalCompleted |
| AutoResearchEngine | SkillEvalCompleted, PromptMutated |
| Queen | SwarmCompleted, CollectiveMemoryEntry |
| CollectiveMemory | CollectiveMemoryEntry |
| HiveDaemon | OutcomeRecorded (remote) |
| LearningCortex | ImprovementApplied, ImprovementRolledBack, StrategyWeightAdjusted |

## Consumer

Only `LearningCortex` subscribes to the event bus. It runs a single async task loop processing events sequentially.
