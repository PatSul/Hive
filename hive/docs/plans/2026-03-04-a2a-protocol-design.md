# A2A Protocol Integration Design

**Date:** 2026-03-04
**Status:** Approved
**Crate:** `hive_a2a` (new)

## Overview

Add Google's A2A (Agent-to-Agent) protocol support to Hive, enabling bidirectional interoperability with external AI agents. Hive exposes its multi-agent orchestration (HiveMind, Coordinator, Queen) as A2A skills, and can discover + delegate tasks to external A2A agents.

**Protocol version:** A2A v0.3 (Linux Foundation governance)
**Primary dependency:** `a2a-rs` crate (hexagonal architecture, Tokio-based)

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Scope | Both client + server | Full bidirectional A2A interop |
| Crate structure | New `hive_a2a` crate | Clean separation from existing agent logic |
| Protocol implementation | Use `a2a-rs` crate | Don't reinvent JSON-RPC, task lifecycle, streaming |
| Push notifications | Deferred to v2 | SSE streaming covers real-time use cases |
| Auth model | API key (v1), OAuth later | Simple, sufficient for initial adoption |

## Crate Structure

```
hive/crates/hive_a2a/
├── Cargo.toml
├── src/
│   ├── lib.rs              — Public API, re-exports
│   ├── agent_card.rs       — Build & serve Hive's Agent Card
│   ├── server.rs           — HTTP server (Axum), routes, middleware
│   ├── task_handler.rs     — A2A Task → Hive orchestrator bridge
│   ├── streaming.rs        — SSE stream: TaskEvent → A2A events
│   ├── client.rs           — Discover external Agent Cards, send tasks
│   ├── remote_agent.rs     — RemoteAgent wrapper for external A2A agents
│   ├── bridge.rs           — Type conversions: A2A ↔ hive_agents
│   ├── auth.rs             — API key validation, middleware
│   ├── config.rs           — A2A server/client configuration
│   └── error.rs            — A2A-specific error types
```

### Dependencies

- `a2a-rs` — Protocol types, JSON-RPC, Agent Card schema
- `axum` — HTTP server
- `tokio` — Async runtime (already in workspace)
- `reqwest` — HTTP client for outbound calls + discovery
- `tower` — Middleware (auth, rate limiting)
- `hive_agents` — Bridge target (HiveMind, Coordinator, Queen)
- `hive_core` — SecurityGateway for inbound validation

## Agent Card

Served at `GET /.well-known/agent-card.json`:

```json
{
  "name": "Hive",
  "description": "Multi-agent AI coding assistant with hierarchical orchestration",
  "provider": { "organization": "AIrglow Studio" },
  "url": "http://localhost:7420/a2a",
  "capabilities": {
    "streaming": true,
    "pushNotifications": false
  },
  "skills": [
    {
      "id": "hivemind",
      "name": "HiveMind Multi-Agent Pipeline",
      "description": "9-role orchestration: Architect, Coder, Reviewer, Tester, Debugger, Security, Documenter, OutputReviewer, TaskVerifier.",
      "inputModes": ["text"],
      "outputModes": ["text"]
    },
    {
      "id": "coordinator",
      "name": "Task Coordinator",
      "description": "Dependency-ordered parallel task execution for decomposable specs.",
      "inputModes": ["text"],
      "outputModes": ["text"]
    },
    {
      "id": "queen",
      "name": "Queen Swarm Orchestration",
      "description": "Multi-team swarm with cross-team learning for large goals.",
      "inputModes": ["text"],
      "outputModes": ["text"]
    },
    {
      "id": "single",
      "name": "Single Agent",
      "description": "One-shot AI call with a specific persona.",
      "inputModes": ["text"],
      "outputModes": ["text"]
    }
  ],
  "securitySchemes": {
    "apiKey": { "type": "apiKey", "in": "header", "name": "X-Hive-Key" }
  }
}
```

## Task Handler — Routing

```
Inbound SendMessage → extract skill_id → route:

  "hivemind"    → HiveMind::execute(task, provider, config)
  "coordinator" → Coordinator::execute(spec, provider, config)
  "queen"       → Queen::execute(goal, provider, config)
  "single"      → AiExecutor::execute_with_persona(...)
```

### Type Mapping (bridge.rs)

| A2A Type | Hive Type | Direction |
|---|---|---|
| `Message { role: "user", parts: [Text] }` | `String` (task description) | Inbound |
| `Task { status: Working }` | `TaskEvent::PhaseStarted` | Outbound |
| `TaskStatusUpdateEvent` | `TaskEvent::AgentStarted/Completed` | Outbound |
| `Artifact { parts: [Text] }` | `AgentOutput { content }` | Outbound |
| `Task { status: InputRequired }` | Steering message queue drain | Bidirectional |
| `Task { status: Failed }` | `OrchestratorError` | Outbound |

### Skill Inference (when skill_id not specified)

- Short messages → `single`
- "plan", "architect", "design + implement" → `hivemind`
- Dependency language ("then", "after", "steps") → `coordinator`
- "teams" or very large scope → `queen`

### Task Storage

In-memory `HashMap<String, Task>` for v1. Clients can `GetTask` by ID to poll.

## Streaming — SSE Bridge

HiveMind/Coordinator/Queen emit `TaskEvent` internally. Mapped to SSE:

```
PhaseStarted("architect")       → TaskStatusUpdateEvent { state: WORKING }
AgentStarted { role, model }    → TaskStatusUpdateEvent { state: WORKING }
AgentCompleted { role, output } → TaskArtifactUpdateEvent { append: true }
ConsensusReached { score }      → TaskStatusUpdateEvent { state: WORKING }
Completed { synthesis }         → TaskArtifactUpdateEvent { lastChunk: true }
                                → TaskStatusUpdateEvent { state: COMPLETED }
```

Implementation: `tokio::sync::broadcast` channel per task. Streaming endpoint subscribes and converts `TaskEvent` → SSE frames.

## Client — External Agent Discovery & Delegation

### Discovery

```rust
fn discover(url: &str) -> Result<AgentCard>
  // GET {url}/.well-known/agent-card.json
  // Cache in memory (TTL: 5 min)
```

### RemoteAgent

```rust
struct RemoteAgent {
    card: AgentCard,
    client: reqwest::Client,
    auth: Option<AuthConfig>,
}

impl RemoteAgent {
    async fn send_task(&self, message: &str, skill_id: Option<&str>) -> Result<Task>;
    async fn send_streaming(&self, message: &str) -> Result<impl Stream<Item = TaskEvent>>;
}
```

### Integration with Orchestrators

New task executor variant for Coordinator:

```rust
enum TaskExecutor {
    Local(PersonaKind),         // Existing: call AI provider
    External(RemoteAgent),      // New: delegate to A2A agent
}
```

### Agent Registry (~/.hive/a2a.toml)

```toml
[[agents]]
name = "CodeReview Bot"
url = "https://review-agent.example.com"
api_key = "their-key"
```

## Auth & Security

### Inbound (Server)

- API key via `X-Hive-Key` header, validated against config
- All task messages pass through `SecurityGateway::check_command()`
- Rate limiting via Tower middleware (configurable RPM)
- **Localhost-only by default** (`127.0.0.1:7420`)
- Must explicitly set `bind = "0.0.0.0"` to expose to network

### Outbound (Client)

- Per-agent API keys in `~/.hive/a2a.toml`
- HTTPS enforced for non-localhost URLs
- Private IP blocking (127.x, 10.x, 192.168.x, 169.254.x) for SSRF prevention

### Not in v1

OAuth 2.0, OIDC, JWT/JWKS for push notification verification.

## Configuration

```toml
[server]
enabled = true
bind = "127.0.0.1"
port = 7420
api_key = "your-secret-key"
max_concurrent_tasks = 10
rate_limit_rpm = 60

[server.defaults]
max_budget_usd = 1.00
max_time_seconds = 300
default_skill = "hivemind"

[client]
discovery_cache_ttl_seconds = 300
request_timeout_seconds = 60

[[agents]]
name = "Example Agent"
url = "https://agent.example.com"
api_key = "their-api-key"
```

## Error Handling

| Scenario | A2A Response |
|---|---|
| Unknown skill_id | `UnsupportedOperationError` |
| Budget exceeded | Task → `FAILED` "Budget limit reached" |
| AI provider error | Task → `FAILED` with detail |
| Bad API key | HTTP 401 |
| Rate limit exceeded | HTTP 429 + Retry-After |
| Task not found | `TaskNotFoundError` |
| INPUT_REQUIRED timeout (10min) | Task → `FAILED` "Steering timeout" |

## Data Flow

```
External Agent                    hive_a2a                         hive_agents
─────────────                    ─────────                        ───────────

GET /.well-known/agent-card.json → agent_card.rs → AgentCard JSON

POST /a2a (SendStreamingMessage) → server.rs
                                   → auth.rs (validate X-Hive-Key)
                                   → task_handler.rs
                                     → bridge.rs (A2A Message → task string)
                                     → route to skill_id
                                     → spawn orchestrator ──────→ HiveMind::execute()
                                     → broadcast channel ←────── TaskEvent stream
                                   → streaming.rs
                                     → TaskEvent → SSE frames
                                   ← SSE: TaskStatusUpdateEvent
                                   ← SSE: TaskArtifactUpdateEvent
                                   ← SSE: TaskStatusUpdateEvent { COMPLETED }

Hive (as client)                  hive_a2a                         External Agent
────────────────                  ─────────                        ──────────────

Coordinator needs external     → remote_agent.rs
                                   → discovery.rs (fetch Agent Card)
                                   → POST {url} SendStreamingMessage → External processes
                                   ← SSE events ←──────────────────── Results stream
                                   → bridge.rs (A2A → AgentOutput)
                                → Coordinator collects result
```

## Testing Strategy

1. **Unit tests** — Bridge type conversions, Agent Card construction, config parsing
2. **Integration tests** — Axum server in-process, JSON-RPC requests, task lifecycle
3. **Streaming tests** — SSE client connects, receives events in correct order
4. **Client tests** — Mock HTTP server, verify RemoteAgent behavior
5. **Security tests** — Bad keys rejected, private IPs blocked, rate limiting

## v2 Roadmap (Not In Scope)

- Push notifications (webhooks) for long-running tasks
- OAuth 2.0 / OIDC authentication
- File artifacts (code files, patches) in inputModes/outputModes
- UI panel showing available external agents
- Agent Card extended discovery with registries
- gRPC transport option
