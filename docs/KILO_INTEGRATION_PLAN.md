# Kilo.ai Integration Plan for Hive

**Status**: Research / Scaffolding (no code changes)
**Date**: 2026-03-24
**Scope**: Proposal for integrating Kilo.ai's open-source coding agent into Hive

---

## 1. What is Kilo?

Kilo is an open-source (Apache 2.0) AI coding agent that exposes a local HTTP REST API on
`localhost:4096`. Key characteristics:

- **Session-oriented**: coding work is organized into long-lived sessions with file system state
- **Multi-model**: routes to 500+ models via its `/provider` endpoint
- **MCP-native**: manages Model Context Protocol servers via `/mcp`
- **Terminal access**: spawns and manages PTY processes via `/pty`
- **SSE streaming**: real-time event streams at `/global/event` and `/session/{id}/event`
- **Forkable sessions**: `POST /session/{id}/fork` for parallel agent branches
- **ACP support**: Agent Client Protocol for richer editor-style integration
- **Auth**: HTTP Basic Auth when `KILO_SERVER_PASSWORD` is set
- **CORS**: permits localhost origins by default

---

## 2. Hive Architecture Recap

### Workspace crates relevant to this integration

```
hive_ai          — AI provider trait (AiProvider), model registry, routing, RAG, TTS
hive_agents      — Agent orchestration: Queen → Swarm → HiveMind/Coordinator → AiExecutor
                   Also: MCP client, skill system, collective memory, worktrees
hive_integrations — Third-party service clients (GitHub, Docker, Slack, IDE, etc.)
hive_a2a         — A2A (Agent-to-Agent) protocol — server + client for external agents
hive_terminal    — Shell, PTY, browser sandbox execution
hive_core        — Config, SQLite persistence, security gateway, channels, sessions
```

### Key trait: `AiProvider` (`hive_ai::providers`)

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn provider_type(&self) -> ProviderType;
    fn name(&self) -> &str;
    async fn is_available(&self) -> bool;
    async fn get_models(&self) -> Vec<ModelInfo>;
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError>;
    async fn stream_chat(&self, request: &ChatRequest)
        -> Result<mpsc::Receiver<StreamChunk>, ProviderError>;
}
```

Every AI backend in Hive (Anthropic, Ollama, OpenAI, Groq, etc.) implements this trait. Providers
are registered in `AiService` as `HashMap<ProviderType, Arc<dyn AiProvider>>`.

### Key trait: `AiExecutor` (`hive_agents::hivemind`)

```rust
pub trait AiExecutor: Send + Sync {
    async fn execute(&self, request: &ChatRequest) -> Result<ChatResponse, String>;
}
```

Simpler interface used by `HiveMind`, `Coordinator`, and `Queen` for inner AI calls. Bridges the
agent orchestration layer to an actual provider.

### The agent hierarchy

```
Queen (SwarmConfig)
  └── Teams (TeamObjective → OrchestrationMode)
        ├── HiveMind (9 specialized roles: Architect, Coder, Reviewer, …)
        ├── Coordinator (dependency-ordered task dispatch to Personas)
        ├── NativeProvider (model's own multi-agent capability)
        └── SingleShot
```

### Existing protocol integrations for comparison

| Crate        | What it wraps        | Pattern                              |
|--------------|----------------------|--------------------------------------|
| `hive_a2a`   | Google A2A protocol  | New crate, `RemoteAgent`, `A2aClientService` |
| `hive_agents::mcp_client` | MCP (JSON-RPC 2.0) | Module in `hive_agents`, SSE + stdio transports |
| `hive_integrations::ide` | VS Code / IDE LSP | Module in `hive_integrations`, `IdeIntegrationService` |

---

## 3. Where a `hive_kilo` Crate Would Live

### Recommendation: New workspace crate `hive_kilo`

This mirrors how `hive_a2a` was structured — Kilo is a full agent protocol, not just a service
client, so it deserves its own crate rather than a module in `hive_integrations`.

```
hive/crates/hive_kilo/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── client.rs          — Low-level HTTP client wrapping all Kilo REST endpoints
    ├── session.rs         — KiloSession: lifecycle management (create/fork/close)
    ├── events.rs          — SSE event types for /global/event and /session/{id}/event
    ├── provider.rs        — KiloAiProvider: implements AiProvider (routes via Kilo's model router)
    ├── executor.rs        — KiloAiExecutor: implements AiExecutor (session-backed execution)
    ├── mcp_bridge.rs      — Bridge between Hive's McpClient and Kilo's /mcp endpoint
    ├── pty_bridge.rs      — Bridge between hive_terminal and Kilo's /pty endpoint
    ├── config.rs          — KiloConfig: base_url, password, timeouts, session policy
    └── error.rs           — KiloError enum
```

**Crate dependencies:**

```toml
[dependencies]
hive_ai  = { path = "../hive_ai" }      # AiProvider trait, types
hive_agents = { path = "../hive_agents" } # AiExecutor trait (optional — enables agent integration)
hive_core = { path = "../hive_core" }   # Config, security gateway

reqwest.workspace = true   # HTTP + SSE streaming
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
futures.workspace = true
async-trait.workspace = true
tracing.workspace = true
anyhow.workspace = true
thiserror.workspace = true
uuid.workspace = true
```

`hive_kilo` would then be added as an optional dependency of:
- `hive_ai` or `hive_app`: for `KiloAiProvider` registration in `AiService`
- `hive_agents`: for `KiloAiExecutor` use in swarm orchestration

---

## 4. Kilo API → Hive Architecture Mapping

### 4.1 Provider-level mapping (`KiloAiProvider`)

| Hive `AiProvider` method | Kilo endpoint | Notes |
|---|---|---|
| `is_available()` | `GET /config` | 2-second timeout; false if Kilo not running |
| `get_models()` | `GET /provider` | Returns Kilo's 500+ model list; map to `ModelInfo` with `ProviderType::Kilo` |
| `chat()` | `POST /session` → send message → `GET /session/{id}` until done | Stateless-appearing, but Kilo uses sessions internally |
| `stream_chat()` | `POST /session` → `GET /session/{id}/event` SSE | Map SSE chunks to `StreamChunk` |

For `chat()` and `stream_chat()`, the simplest mapping is **ephemeral sessions**: create a new Kilo
session per call, send the message, collect the response, close the session. A `session_pool`
optimization can be added later for reuse.

### 4.2 Executor-level mapping (`KiloAiExecutor`)

Implements `AiExecutor` for use inside `HiveMind` and `Coordinator`. Each `execute()` call:

1. Acquires or creates a `KiloSession`
2. Translates `ChatRequest` messages → Kilo's message format (including system prompt and tool defs)
3. Sends the message via `POST /session/{id}/chat`
4. Reads the response from the SSE stream or polls until done
5. Returns `ChatResponse` with content, usage approximation, and tool calls if any

For `HiveMind`'s 9-role pipeline, each role call is a separate `execute()` invocation. The session
can optionally be **shared** across roles (preserving file-system context) or **forked**:

```
HiveMind task starts
  → create root KiloSession
      → Architect role: execute on root session
      → Coder role: fork(root) → execute on fork
      → Reviewer role: fork(root) → execute on fork
      → ... each role gets its own fork
  → synthesize results
  → close all sessions
```

This maps well to Kilo's `POST /session/{id}/fork` capability and is a natural fit for parallel
agent branches in `SwarmPlan`.

### 4.3 MCP bridge (`KiloMcpBridge`)

Kilo manages MCP servers via `/mcp`. Hive already has its own MCP client
(`hive_agents::mcp_client`) for connecting to arbitrary MCP servers.

Two integration modes are possible:

**Mode A — Kilo as MCP gateway**: Instead of Hive connecting directly to MCP servers, Kilo manages
them and Hive queries Kilo's `/mcp` endpoint for available tools. `KiloMcpBridge` translates Kilo's
MCP list into `McpTool` objects that Hive's tool-use layer already understands.

**Mode B — Shared config**: Hive reads Kilo's MCP configuration and registers the same servers in
its own `McpClient`. Simpler but loses Kilo's MCP management.

Mode A is preferred: it lets Kilo manage server lifecycle while Hive benefits from its tools.

### 4.4 PTY bridge (`KiloPtyBridge`)

Kilo's `/pty` endpoint creates and manages terminal processes. Hive's `hive_terminal::executor`
already runs local shells. The bridge would:

- Create a Kilo PTY session mapped to a Hive `Shell` instance
- Forward writes from Hive's terminal UI to `POST /pty/{id}/write`
- Subscribe to `GET /pty/{id}/output` SSE to receive terminal output
- Map to the existing `hive_terminal` traits so the UI layer sees no difference

This is lower priority than the provider/executor integration — skip for initial implementation.

### 4.5 SSE event type hierarchy

Kilo's SSE events carry richer semantic meaning than Hive's current `StreamChunk`. A new type is
needed:

```rust
// hive_kilo::events
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KiloEvent {
    TextDelta { content: String },
    ToolCall { id: String, name: String, input: serde_json::Value },
    ToolResult { id: String, output: serde_json::Value },
    FileChange { path: String, kind: FileChangeKind },
    ThinkingDelta { content: String },
    Done { usage: Option<KiloUsage>, stop_reason: String },
    Error { message: String },
}
```

`KiloAiProvider::stream_chat()` maps `KiloEvent` → `StreamChunk` (dropping file change events
unless a richer callback is provided).

---

## 5. `ProviderType::Kilo` Addition

`hive_ai::types::ProviderType` would gain a new variant:

```rust
pub enum ProviderType {
    // ... existing ...
    Kilo,
}
```

`AiServiceConfig` would gain:

```rust
pub struct AiServiceConfig {
    // ... existing ...
    pub kilo_url: Option<String>,        // defaults to http://localhost:4096
    pub kilo_password: Option<String>,   // for KILO_SERVER_PASSWORD auth
    pub kilo_enabled: bool,
}
```

`AiService::new()` registers `KiloAiProvider` when `kilo_enabled` is true and Kilo is reachable.

---

## 6. Session Lifecycle Policy

Kilo sessions hold file-system state and consume resources. A clear policy is needed:

| Use case | Session policy |
|---|---|
| `AiProvider::chat()` (simple Q&A) | Ephemeral — create, use, close |
| `AiProvider::stream_chat()` | Ephemeral — create, stream, close |
| `HiveMind` task run | Rooted — one root session per task, roles run on forks |
| `Coordinator` wave | Shared root per plan, tasks fork from it |
| `Queen` swarm | Root per `TeamObjective`; teams are isolated |
| Persistent pair-programming | Explicit long-lived session, user-managed |

A `KiloSessionPool` with LRU eviction can be added later to amortize session creation cost.

---

## 7. Trait Implementations Summary

```
hive_kilo::KiloAiProvider
  implements: hive_ai::providers::AiProvider
  used by:    hive_ai::service::AiService (in providers HashMap)

hive_kilo::KiloAiExecutor
  implements: hive_agents::hivemind::AiExecutor
  used by:    HiveMind::new(), Coordinator::new(), Queen (via ArcExecutor wrapper)

hive_kilo::KiloMcpBridge
  implements: (no existing trait — exposes tools as Vec<McpTool>)
  used by:    hive_agents::tool_use (tool registration)

hive_kilo::KiloPtyBridge
  implements: (TBD — aligns with hive_terminal abstractions)
  used by:    hive_terminal::executor (future)
```

---

## 8. Integration Concerns

### 8.1 Two-level model routing

Kilo has its own 500+ model router. Hive's `ModelRouter` operates on `ProviderType`. When Hive
selects `ProviderType::Kilo`, it still needs to choose *which* Kilo model to use. Two options:

- **Passthrough**: `KiloConfig` has a `default_model` string that is sent to Kilo; Hive's router
  just picks Kilo and defers model selection to Kilo's own routing logic.
- **Explicit mapping**: Hive's `get_models()` fetches Kilo's model list and presents them as
  `ModelInfo { provider_type: Kilo, id: "kilo:provider/model", … }`. Users can select a specific
  Kilo-routed model.

The explicit mapping is cleaner and consistent with how other providers work.

### 8.2 Privacy mode

Kilo routes to cloud providers (OpenAI, Anthropic, etc.) under the hood. If Hive's `privacy_mode`
is enabled, `KiloAiProvider` should be skipped unless the user has configured Kilo with a
local-only model. The `get_models()` response can be filtered by checking model metadata.

### 8.3 Token accounting

Kilo abstracts the underlying provider, so raw token counts may not match what Hive's
`CostTracker` expects. `KiloUsage` (from the SSE `Done` event) should carry whatever Kilo reports.
Cost calculations will be approximate — Hive will not know which cloud provider Kilo internally
used unless Kilo reports it. Flag this in the UI as "via Kilo" rather than a specific provider.

### 8.4 Kilo availability as a hard dependency

Kilo must be installed and running locally. `is_available()` gracefully returns `false` when it is
not. `AiService` should not fail to initialize if Kilo is unavailable — it simply omits the
`Kilo` provider from the routing table. The UI should show Kilo as "offline" in the model browser.

### 8.5 ACP vs plain REST

ACP (Agent Client Protocol) offers richer editor-style integration — workspace introspection,
in-editor diagnostics, richer tool definitions. For the initial integration, use the plain REST API.
ACP can be layered on in a future `KiloAcpClient` once the REST baseline is proven.

### 8.6 Authentication

Kilo uses HTTP Basic Auth when `KILO_SERVER_PASSWORD` is set. `KiloClient` should:
1. Default to no auth (local-only, no password)
2. Attach `Authorization: Basic base64("kilo:{password}")` when `kilo_password` is configured
3. Store the password via `hive_core::secure_storage::SecureStorage` (AES-256-GCM)

### 8.7 Streaming chunk semantics gap

Hive's `StreamChunk` is `{ content, done, thinking, usage, tool_calls, stop_reason }`. Kilo events
include `FileChange` which has no equivalent. The bridge should:
- Forward text/thinking/tool_call events as `StreamChunk`
- Deliver file-change events via a parallel `KiloFileEvent` callback (for the UI to show diffs)
- Not drop file events silently — they are important for agent transparency

### 8.8 Circular dependency risk

`hive_kilo` depends on `hive_ai` (for `AiProvider` trait). If `hive_ai` imports `hive_kilo` to
register the provider, that creates a cycle. The solution: keep `hive_ai` free of `hive_kilo`.
Instead, `hive_app` (the top-level application crate) imports both and wires them together:

```rust
// hive_app — registration site, no cycle
use hive_ai::service::AiService;
use hive_kilo::provider::KiloAiProvider;

let kilo = KiloAiProvider::new(config.kilo_url, config.kilo_password);
service.register(ProviderType::Kilo, Arc::new(kilo));
```

---

## 9. Proposed `hive_kilo` Public API Sketch

```rust
// hive_kilo::config
pub struct KiloConfig {
    pub base_url: String,       // default: "http://localhost:4096"
    pub password: Option<String>,
    pub connect_timeout_secs: u64,
    pub default_model: Option<String>,
    pub session_policy: SessionPolicy,
}

pub enum SessionPolicy {
    AlwaysEphemeral,
    PooledLru { max_sessions: usize },
}

// hive_kilo::client
pub struct KiloClient { /* reqwest::Client + config */ }

impl KiloClient {
    pub async fn health(&self) -> bool;
    pub async fn list_models(&self) -> Result<Vec<KiloModel>, KiloError>;
    pub async fn create_session(&self, opts: CreateSessionOpts) -> Result<KiloSession, KiloError>;
    pub async fn fork_session(&self, id: &str) -> Result<KiloSession, KiloError>;
    pub async fn close_session(&self, id: &str) -> Result<(), KiloError>;
    pub async fn send_message(&self, id: &str, msg: KiloMessage) -> Result<(), KiloError>;
    pub async fn subscribe_events(&self, id: &str) -> Result<EventStream, KiloError>;
    pub async fn list_mcp_servers(&self) -> Result<Vec<KiloMcpServer>, KiloError>;
    pub async fn create_pty(&self, opts: PtyOpts) -> Result<KiloPty, KiloError>;
}

// hive_kilo::provider
pub struct KiloAiProvider { client: Arc<KiloClient> }

impl AiProvider for KiloAiProvider { /* ... */ }

// hive_kilo::executor
pub struct KiloAiExecutor { client: Arc<KiloClient>, config: KiloConfig }

impl AiExecutor for KiloAiExecutor { /* ... */ }

// hive_kilo::mcp_bridge
pub struct KiloMcpBridge { client: Arc<KiloClient> }

impl KiloMcpBridge {
    pub async fn available_tools(&self) -> Vec<McpTool>;
    pub async fn call_tool(&self, name: &str, input: serde_json::Value) -> Result<serde_json::Value, KiloError>;
}
```

---

## 10. Implementation Sequence

Suggested order if the team decides to proceed:

1. **`hive_kilo::client`** — raw HTTP wrapper, all endpoints, auth, error types
2. **`hive_kilo::provider` (`KiloAiProvider`)** — `AiProvider` impl with ephemeral sessions
3. **Wire into `hive_app`** — add `ProviderType::Kilo`, register in `AiService`, surface in model browser UI
4. **`hive_kilo::executor` (`KiloAiExecutor`)** — `AiExecutor` impl for HiveMind/Coordinator
5. **Session forking** — rooted session model for multi-role HiveMind tasks
6. **`hive_kilo::mcp_bridge`** — expose Kilo-managed MCP tools to Hive's tool registry
7. **SSE file-change events** — wire into UI for agent transparency
8. **`hive_kilo::pty_bridge`** — bridge Kilo PTY to `hive_terminal` (lower priority)
9. **ACP client** — richer editor integration (future)

---

## 11. Files to Create (No Existing Files Modified)

```
hive/crates/hive_kilo/Cargo.toml
hive/crates/hive_kilo/src/lib.rs
hive/crates/hive_kilo/src/config.rs
hive/crates/hive_kilo/src/error.rs
hive/crates/hive_kilo/src/client.rs
hive/crates/hive_kilo/src/session.rs
hive/crates/hive_kilo/src/events.rs
hive/crates/hive_kilo/src/provider.rs
hive/crates/hive_kilo/src/executor.rs
hive/crates/hive_kilo/src/mcp_bridge.rs
hive/crates/hive_kilo/src/pty_bridge.rs
```

**Existing files that will eventually need small additions** (but not now):

| File | Change needed |
|---|---|
| `hive/Cargo.toml` | Add `"crates/hive_kilo"` to `members` |
| `hive_ai/src/types.rs` | Add `ProviderType::Kilo` variant |
| `hive_ai/src/service.rs` | Add `kilo_url`/`kilo_password` to `AiServiceConfig`; register `KiloAiProvider` |
| `hive_core/src/config.rs` | Expose Kilo config fields from `HiveConfig` |

These changes are minimal and backwards-compatible — no existing logic is altered.
