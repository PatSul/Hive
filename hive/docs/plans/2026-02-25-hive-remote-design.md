# hive_remote — Remote Control, Relay & Session Persistence

**Date:** 2026-02-25
**Status:** Approved
**Vault ref:** [[Architecture Overview]], [[Data Persistence]], [[Security Architecture]], [[Roadmap]]
**Roadmap alignment:** Q3 2026 — Collaboration (P2P federation refinement, team workspace features)

---

## Overview

A new `hive_remote` crate that gives Hive four capabilities inspired by Claude Code's remote control:

1. **Session persistence & background daemon** — Conversations and agent tasks survive app restarts
2. **Outbound relay (layered LAN + WAN)** — NAT-friendly networking via relay protocol
3. **Full remote control web UI** — Complete Hive experience from any browser
4. **QR-based cryptographic device pairing** — Persistent trust between Hive instances

## Architecture Decision: Approach B — New Crate

`hive_remote` is a dedicated crate, keeping `hive_network` focused on P2P node discovery and direct connections.

```
hive_remote/
  src/
    lib.rs          — Public API
    daemon.rs       — Background service (keeps sessions alive)
    relay.rs        — Outbound relay protocol (LAN + WAN)
    web_server.rs   — Embedded HTTP/WS server for remote UI
    web_api.rs      — REST + WebSocket API endpoints
    session.rs      — Enhanced session persistence & recovery
    pairing.rs      — QR-based cryptographic device pairing
    qr.rs           — QR code generation
  web/
    index.html      — SPA shell
    app.js          — Preact-based remote UI (~100KB total)
    style.css       — Matches desktop theme (#00D4FF accent)
```

### Crate Dependencies

```
hive_remote
  ├── hive_core     (config, SecurityGateway, SecureStorage)
  ├── hive_network  (P2P transport, LAN relay)
  ├── hive_ai       (AI streaming, provider routing)
  ├── hive_agents   (orchestration — HiveMind/Coordinator/Queen)
  ├── axum          (embedded web server)
  ├── tokio         (async runtime)
  ├── x25519-dalek  (key exchange)
  ├── aes-gcm       (E2E encryption)
  ├── hkdf          (key derivation)
  ├── qrcode        (QR generation)
  └── base64        (URL-safe encoding)
```

---

## Feature 1: Session Persistence & Background Daemon

### Problem

Current Hive sessions are ephemeral — close the app and lose agent progress, conversation position, and streaming state.

### Architecture

```
┌─────────────────────────────────────────────┐
│  hive_app (GUI)                             │
│  ├─ Connects to daemon on startup           │
│  ├─ Sends UI events (chat, panel switch)    │
│  └─ Receives state updates (agent progress) │
└──────────────┬──────────────────────────────┘
               │ Local WebSocket (127.0.0.1:9480)
┌──────────────▼──────────────────────────────┐
│  HiveDaemon (tokio runtime)                 │
│  ├─ ChatService (conversations, streaming)  │
│  ├─ AgentOrchestrator (HiveMind/Queen)      │
│  ├─ SessionManager (state snapshots)        │
│  └─ Persists to ~/.hive/                    │
└─────────────────────────────────────────────┘
```

### Design

1. **`hive_app` spawns the daemon** as a tokio task on startup.
2. **Daemon owns all stateful services**: ChatService, agent orchestration, AI streaming. The GUI becomes a thin view layer.
3. **State snapshots**: Every mutation gets journaled to `~/.hive/session_journal.jsonl` (append-only). On recovery, replay the journal.
4. **Graceful shutdown**: On app close, daemon flushes state and keeps running for 30 seconds (configurable) to finish in-flight agent tasks. If GUI reconnects within that window, seamless resume.
5. **Crash recovery**: On next launch, daemon replays journal, restores conversation position, resumes `in_progress` agent tasks.

### Event Protocol (GUI <-> Daemon)

```rust
enum DaemonEvent {
    // GUI -> Daemon
    SendMessage { conversation_id: String, content: String, model: String },
    SwitchPanel(Panel),
    StartAgentTask { task: String, orchestration: OrchestrationMode },
    CancelAgentTask { run_id: String },

    // Daemon -> GUI
    StreamChunk { conversation_id: String, chunk: String },
    StreamComplete { conversation_id: String, usage: TokenUsage },
    AgentStatus { run_id: String, status: SwarmStatus, detail: String },
    StateSnapshot(SessionSnapshot),
}

struct SessionSnapshot {
    active_conversation: Option<String>,
    active_panel: Panel,
    agent_runs: Vec<AgentRunSummary>,
    timestamp: DateTime<Utc>,
}
```

### Existing Infrastructure (from [[Data Persistence]])

Extends current `~/.hive/` structure:
- `session.json` — already tracks window size, active panel, crash recovery
- `memory.db` (WAL) — conversations, costs, logs with FTS5
- `network.json` — P2P config (extended for relay config)

New additions:
- `~/.hive/session_journal.jsonl` — append-only event journal
- `~/.hive/paired_devices.json` — encrypted device trust store

### What Changes in Existing Code

- **ChatService** moves from `hive_ui` entity → `hive_remote` daemon (GUI gets a proxy entity)
- **Agent execution** moves behind daemon (GUI calls `StartAgentTask`, gets callbacks)
- **SessionState** in `hive_core` extended with journal replay

---

## Feature 2: Outbound Relay (Layered LAN + WAN)

### Problem

`hive_network` uses direct WebSocket + UDP discovery. Works on LAN, fails behind NAT/firewalls.

### Architecture

```
LAN: Any Hive node acts as relay for peers (direct WebSocket)
WAN: Outbound-only WSS through external relay server

┌──────────┐  outbound   ┌──────────┐  outbound   ┌──────────┐
│ Hive A   │────────────►│ Relay    │◄────────────│ Hive B   │
│ (home)   │  WSS        │ (cloud)  │  WSS        │ (phone)  │
└──────────┘             └──────────┘             └──────────┘

NO inbound ports opened. Ever.
```

### Relay Protocol

Transport-agnostic envelope — any WebSocket server that forwards JSON between rooms can be a relay.

```rust
enum RelayFrame {
    // Auth
    Register { session_token: String, node_id: PeerId },
    Authenticate { pairing_token: String },

    // Rooms
    CreateRoom { room_id: String, encryption_key_fingerprint: String },
    JoinRoom { room_id: String, pairing_token: String },
    LeaveRoom,

    // Data (E2E encrypted — relay cannot read)
    Forward { to: Option<PeerId>, payload: EncryptedEnvelope },

    // Control
    Ping,
    Pong,
    Error { code: u16, message: String },
}

struct EncryptedEnvelope {
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
    sender_fingerprint: String,
}
```

### Key Decisions

1. **E2E encryption**: Relay sees only ciphertext. Shared key from QR pairing (X25519).
2. **Room-based routing**: Each session = one room. Paired devices join by room ID + pairing token.
3. **Outbound-only**: Nodes connect TO relay, never listen inbound.
4. **LAN relay mode**: Any Hive node relays for peers. Uses existing `hive_network` transport + new `MessageKind::RelayRequest/RelayResponse`.
5. **Reference relay**: Minimal Rust binary or Cloudflare Worker. ~200 lines. Room management + frame forwarding. No message inspection.

### Security (per [[Security Architecture]])

- All relay traffic passes through SecurityGateway validation
- Commands forwarded via relay are scanned for injection/jailbreak by HiveShield
- API keys never travel over relay (headers only, HTTPS)
- Private IPs blocked per existing SecurityGateway rules

### New hive_network Additions

```rust
MessageKind::RelayRequest   // Forward to peer X
MessageKind::RelayResponse  // Relayed from peer Y

// Config
pub relay_enabled: bool,
pub wan_relay_url: Option<String>,
pub relay_mode: RelayMode, // Client | Server | Both
```

---

## Feature 3: Full Remote Control (Web UI)

### Problem

No way to check on Hive from a phone or another machine. Agent tasks run invisibly.

### Architecture

```
Browser/Mobile
  └── Hive Web UI (Preact SPA, <100KB)
        │
        │ WSS (via relay or direct LAN)
        ▼
HiveDaemon
  └── WebAPI (axum)
        ├── GET  /api/state         → full snapshot
        ├── GET  /api/panels/:id    → panel data
        ├── POST /api/chat          → send message
        ├── POST /api/agents        → start/cancel agent task
        ├── WS   /ws                → real-time event stream
        └── GET  /                  → serve embedded web UI
```

### Connection Modes

- **LAN (direct)**: Browser → `https://192.168.x.x:9481/`. Discovered via QR or mDNS.
- **WAN (relayed)**: Browser → relay → daemon. Web UI served from relay static hosting or loaded locally first.

### Web UI

Embedded in Hive binary via `include_bytes!`. No Node.js, no runtime build step.

- **Preact** (~3KB) for components without weight
- **Vanilla CSS** matching desktop theme (#00D4FF accent)
- **Total budget**: < 100KB

### WebSocket Protocol

Same `DaemonEvent` from Feature 1, serialized as JSON:

```
Browser                          Daemon
   │── Subscribe ──────────────►│
   │◄─ StateSnapshot ───────────│  (full initial state)
   │── SendMessage ────────────►│
   │◄─ StreamChunk ─────────────│  (real-time tokens)
   │◄─ StreamComplete ──────────│
   │◄─ AgentStatus ─────────────│  (background updates)
   │── SwitchPanel ────────────►│
   │◄─ PanelData ───────────────│
```

### Authentication

All endpoints require a valid pairing token (from QR pairing, Feature 4).

```rust
fn auth_middleware(req: &Request) -> Result<(), AuthError> {
    let token = req.header("X-Hive-Token")
        .or_else(|| req.query("token"));
    verify_pairing_token(token?)
}
```

### Panel Rendering Strategy

- Daemon serializes panel data as JSON (same structs GPUI renders)
- Web UI has lightweight renderers per panel type
- **Chat**: Full parity (markdown, code blocks, streaming)
- **Agent/Monitor**: Full parity (progress, costs, logs)
- **Other panels**: Read-only initially, interactive incrementally

---

## Feature 4: QR-Based Cryptographic Device Pairing

### Problem

No secure way to link Hive instances across devices. Need trust establishment without accounts or passwords.

### Pairing Flow

```
Desktop (Initiator)                    Phone/Browser (Joiner)
1. Generate X25519 keypair + session ID
2. Encode QR: hive://pair?id=...&pk=...&addr=...&relay=...
3.                    ── scan ──►
                                       4. Decode QR, generate own keypair
                                       5. Connect via addr or relay
                                       6. Send PairRequest{joiner_pk}
7. X25519 DH → shared_secret = HKDF(DH)
8. Derive session_key + pairing_token
                                       9. Same derivation
10. Send PairConfirm{device_name}  ──►
                                       11. Decrypt OK = paired!
12. Store in paired_devices            13. Store in trusted_peers
```

### QR Payload

```
hive://pair?id=<session_uuid>&pk=<base64url_x25519_pub>&addr=<local_ip:port>&relay=<wss://relay.url>&v=1
```

~180 bytes — fits standard QR at medium error correction.

### Key Derivation

```rust
let shared_secret = initiator_secret.diffie_hellman(&joiner_public);
let hkdf = Hkdf::<Sha256>::new(Some(session_id.as_bytes()), shared_secret.as_bytes());

let mut session_key = [0u8; 32];    // AES-256-GCM for E2E
let mut pairing_token = [0u8; 32];  // API auth token
let mut device_id = [0u8; 16];      // Unique device ID

hkdf.expand(b"hive-session-key", &mut session_key);
hkdf.expand(b"hive-pairing-token", &mut pairing_token);
hkdf.expand(b"hive-device-id", &mut device_id);
```

### Trust Store

```rust
// ~/.hive/paired_devices.json (encrypted via SecureStorage)
struct PairedDevice {
    device_id: String,
    device_name: String,
    public_key_fingerprint: String,
    session_key: [u8; 32],
    pairing_token_hash: String,
    paired_at: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    permissions: DevicePermissions,
}

struct DevicePermissions {
    can_chat: bool,
    can_control_agents: bool,
    can_modify_settings: bool,
    can_access_files: bool,
    read_only: bool,
}
```

### Trust Management UI

Settings panel → "Paired Devices":
- List all paired devices (name, last seen, fingerprint)
- Per-device permission toggles
- Unpair button (revokes trust, rotates session key)
- "Pair New Device" button (shows QR code)

Also visible in Network panel alongside P2P peers.

### Security Properties

| Property | Mechanism |
|----------|-----------|
| No MITM | QR = secure out-of-band channel (physical proximity) |
| Forward secrecy | Ephemeral X25519 per pairing session |
| E2E encryption | Relay sees only AES-256-GCM ciphertext |
| Revocable trust | Unpair removes key material, invalidates token |
| Per-device permissions | Granular control per device |
| No accounts/passwords | Cryptographic identity only |

---

## Dependencies (New)

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = "0.24"
x25519-dalek = "2"
hkdf = "0.12"
aes-gcm = "0.10"
qrcode = "0.14"
base64 = "0.22"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "fs"] }
```

---

## Implementation Order

1. **Session persistence + daemon** (foundation — everything else builds on this)
2. **QR pairing** (needed before relay can authenticate)
3. **Outbound relay** (LAN first, then WAN)
4. **Web UI** (consumes all of the above)

---

## Open Questions

- Daemon port 9480/9481 — should these be configurable or auto-discovered?
- Reference relay hosting — ship as Docker image, Cloudflare Worker, or both?
- Web UI framework — Preact vs vanilla JS vs Leptos (Rust→WASM)?
