# Contract: Guardrails System

**Purpose**: Tiered auto-apply safety with soak periods and automatic rollback.

## Tiers

| Tier | Domain | Soak Period | Apply Condition | Rollback Trigger |
|------|--------|-------------|-----------------|------------------|
| Green | Routing | Immediate (monitor 10 interactions) | Quality delta > 0.05 over 20+ samples | Quality degrades in next 10 interactions |
| Yellow | Prompts | 1 hour | Pass rate improvement > 10%, security scan clean | Quality drops > 15% during soak |
| Red | Patterns, Strategies | 24 hours | Quality stable across 50+ interactions | Quality drops + user notified |

## Contract Rules

1. **Hard stops** (never auto-apply):
   - SecurityGateway-affecting changes
   - Prompts failing security scanner
   - Active user interaction (< 30s since last input from local or remote)
   - More than 3 changes in 24 hours without user acknowledgment
2. **Rollback**: Every change has a `prior_state` snapshot. Rollback restores prior state and publishes `ImprovementRolledBack` event.
3. **Soak monitoring**: Checked every 60 seconds. Quality measured as average of interactions during soak window vs. `quality_before`.
4. **User override**: Global toggle pauses all auto-apply. Queued changes are held until unpaused.
5. **Startup recovery**: On launch, check `cortex_changes` for `Soaking` status entries. If soak period expired while app was closed, evaluate quality from next few interactions before confirming or rolling back.
