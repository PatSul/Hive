# hive_remote Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the `hive_remote` crate providing session persistence, outbound relay, full remote control web UI, and QR-based cryptographic device pairing.

**Architecture:** New `hive_remote` crate (Approach B). Daemon as tokio task owns ChatService + agent orchestration. GUI becomes thin proxy. Web UI via embedded Preact SPA served by axum. Layered relay: LAN via hive_network, WAN via outbound WSS. QR pairing with X25519 + HKDF + AES-256-GCM.

**Tech Stack:** Rust, tokio, axum, tokio-tungstenite, x25519-dalek, aes-gcm, hkdf, qrcode, serde_json, Preact (web UI)

---

## Task 1: Scaffold hive_remote crate

**Files:**
- Create: `hive/crates/hive_remote/Cargo.toml`
- Create: `hive/crates/hive_remote/src/lib.rs`
- Modify: `hive/Cargo.toml` (workspace members)
- Modify: `hive/crates/hive_app/Cargo.toml` (add hive_remote dep)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "hive_remote"
version = "0.1.0"
edition = "2024"

[dependencies]
hive_core = { path = "../hive_core" }
hive_ai = { path = "../hive_ai" }
hive_agents = { path = "../hive_agents" }
hive_network = { path = "../hive_network" }

tokio = { version = "1", features = ["full"] }
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"] }
axum = { version = "0.8", features = ["ws"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
anyhow = "1"

# Crypto (Feature 4: QR Pairing)
x25519-dalek = { version = "2", features = ["static_secrets"] }
hkdf = "0.12"
aes-gcm = "0.10"
sha2 = "0.10"
rand = "0.8"
qrcode = "0.14"
base64 = "0.22"
hex = "0.4"

[dev-dependencies]
tokio = { version = "1", features = ["test-util", "macros"] }
tempfile = "3"
```

**Step 2: Create lib.rs with module stubs**

```rust
pub mod daemon;
pub mod session;
pub mod relay;
pub mod web_server;
pub mod web_api;
pub mod pairing;
pub mod qr;
pub mod protocol;

pub use daemon::HiveDaemon;
pub use session::SessionJournal;
pub use relay::RelayClient;
pub use pairing::PairingService;
```

**Step 3: Create empty module files**

Create each of these with a placeholder comment:
- `hive/crates/hive_remote/src/daemon.rs`
- `hive/crates/hive_remote/src/session.rs`
- `hive/crates/hive_remote/src/relay.rs`
- `hive/crates/hive_remote/src/web_server.rs`
- `hive/crates/hive_remote/src/web_api.rs`
- `hive/crates/hive_remote/src/pairing.rs`
- `hive/crates/hive_remote/src/qr.rs`
- `hive/crates/hive_remote/src/protocol.rs`

**Step 4: Add to workspace**

In `hive/Cargo.toml`, add `"crates/hive_remote"` to the workspace members list.

**Step 5: Add dep to hive_app**

In `hive/crates/hive_app/Cargo.toml`, add:
```toml
hive_remote = { path = "../hive_remote" }
```

**Step 6: Verify it compiles**

Run: `cargo check -p hive_remote`
Expected: Compiles with no errors (empty modules).

**Step 7: Commit**

```bash
git add hive/crates/hive_remote/ hive/Cargo.toml hive/crates/hive_app/Cargo.toml
git commit -m "feat: scaffold hive_remote crate with module stubs"
```

---

## Task 2: DaemonEvent protocol (protocol.rs)

**Files:**
- Create: `hive/crates/hive_remote/src/protocol.rs`
- Create: `hive/crates/hive_remote/tests/protocol_tests.rs`

**Step 1: Write the failing test**

```rust
// tests/protocol_tests.rs
use hive_remote::protocol::*;
use serde_json;

#[test]
fn test_daemon_event_serialization_roundtrip() {
    let event = DaemonEvent::SendMessage {
        conversation_id: "conv-123".into(),
        content: "Hello world".into(),
        model: "claude-sonnet-4-5-20250929".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let decoded: DaemonEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(json.contains("send_message"), true);
    match decoded {
        DaemonEvent::SendMessage { conversation_id, content, model } => {
            assert_eq!(conversation_id, "conv-123");
            assert_eq!(content, "Hello world");
            assert_eq!(model, "claude-sonnet-4-5-20250929");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_stream_chunk_event() {
    let event = DaemonEvent::StreamChunk {
        conversation_id: "conv-123".into(),
        chunk: "partial text".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let decoded: DaemonEvent = serde_json::from_str(&json).unwrap();
    match decoded {
        DaemonEvent::StreamChunk { conversation_id, chunk } => {
            assert_eq!(conversation_id, "conv-123");
            assert_eq!(chunk, "partial text");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_session_snapshot_serialization() {
    let snapshot = SessionSnapshot {
        active_conversation: Some("conv-456".into()),
        active_panel: "chat".into(),
        agent_runs: vec![AgentRunSummary {
            run_id: "run-001".into(),
            goal: "Refactor auth".into(),
            status: "executing".into(),
            cost_usd: 1.23,
            elapsed_ms: 5000,
        }],
        timestamp: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    let decoded: SessionSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.active_panel, "chat");
    assert_eq!(decoded.agent_runs.len(), 1);
    assert_eq!(decoded.agent_runs[0].run_id, "run-001");
}

#[test]
fn test_all_daemon_event_variants_serialize() {
    let events = vec![
        DaemonEvent::SendMessage {
            conversation_id: "c".into(),
            content: "hi".into(),
            model: "m".into(),
        },
        DaemonEvent::SwitchPanel("agents".into()),
        DaemonEvent::StartAgentTask {
            goal: "build feature".into(),
            orchestration_mode: "hivemind".into(),
        },
        DaemonEvent::CancelAgentTask { run_id: "r1".into() },
        DaemonEvent::StreamChunk {
            conversation_id: "c".into(),
            chunk: "tok".into(),
        },
        DaemonEvent::StreamComplete {
            conversation_id: "c".into(),
            prompt_tokens: 100,
            completion_tokens: 50,
            cost_usd: Some(0.005),
        },
        DaemonEvent::AgentStatus {
            run_id: "r1".into(),
            status: "executing".into(),
            detail: "Running architect role".into(),
        },
        DaemonEvent::StateSnapshot(SessionSnapshot {
            active_conversation: None,
            active_panel: "files".into(),
            agent_runs: vec![],
            timestamp: chrono::Utc::now(),
        }),
        DaemonEvent::PanelData {
            panel: "monitor".into(),
            data: serde_json::json!({"agents": []}),
        },
        DaemonEvent::Error {
            code: 500,
            message: "Internal error".into(),
        },
    ];
    for event in &events {
        let json = serde_json::to_string(event).unwrap();
        let _decoded: DaemonEvent = serde_json::from_str(&json).unwrap();
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_remote --test protocol_tests`
Expected: FAIL — `DaemonEvent` not defined yet.

**Step 3: Write protocol.rs implementation**

```rust
// src/protocol.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Events exchanged between GUI, daemon, and web clients.
/// Serialized as JSON over local WebSocket or relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    // ── Client → Daemon ────────────────────────────────────
    SendMessage {
        conversation_id: String,
        content: String,
        model: String,
    },
    SwitchPanel(String),
    StartAgentTask {
        goal: String,
        orchestration_mode: String,
    },
    CancelAgentTask {
        run_id: String,
    },

    // ── Daemon → Client ────────────────────────────────────
    StreamChunk {
        conversation_id: String,
        chunk: String,
    },
    StreamComplete {
        conversation_id: String,
        prompt_tokens: u32,
        completion_tokens: u32,
        cost_usd: Option<f64>,
    },
    AgentStatus {
        run_id: String,
        status: String,
        detail: String,
    },
    StateSnapshot(SessionSnapshot),
    PanelData {
        panel: String,
        data: serde_json::Value,
    },
    Error {
        code: u16,
        message: String,
    },

    // ── Bidirectional ──────────────────────────────────────
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub active_conversation: Option<String>,
    pub active_panel: String,
    pub agent_runs: Vec<AgentRunSummary>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunSummary {
    pub run_id: String,
    pub goal: String,
    pub status: String,
    pub cost_usd: f64,
    pub elapsed_ms: u64,
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_remote --test protocol_tests`
Expected: All 4 tests PASS.

**Step 5: Commit**

```bash
git add hive/crates/hive_remote/src/protocol.rs hive/crates/hive_remote/tests/protocol_tests.rs
git commit -m "feat(hive_remote): add DaemonEvent protocol with serde serialization"
```

---

## Task 3: Session journal (session.rs)

**Files:**
- Create: `hive/crates/hive_remote/src/session.rs`
- Create: `hive/crates/hive_remote/tests/session_tests.rs`

**Step 1: Write failing tests**

```rust
// tests/session_tests.rs
use hive_remote::session::SessionJournal;
use hive_remote::protocol::DaemonEvent;
use tempfile::tempdir;

#[tokio::test]
async fn test_journal_append_and_replay() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("journal.jsonl");

    let mut journal = SessionJournal::new(&path).unwrap();

    journal.append(&DaemonEvent::SendMessage {
        conversation_id: "c1".into(),
        content: "Hello".into(),
        model: "test".into(),
    }).unwrap();

    journal.append(&DaemonEvent::SwitchPanel("agents".into())).unwrap();

    let events = SessionJournal::replay(&path).unwrap();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn test_journal_replay_empty_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty.jsonl");
    std::fs::write(&path, "").unwrap();

    let events = SessionJournal::replay(&path).unwrap();
    assert_eq!(events.len(), 0);
}

#[tokio::test]
async fn test_journal_replay_corrupt_line_skipped() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("partial.jsonl");

    let mut journal = SessionJournal::new(&path).unwrap();
    journal.append(&DaemonEvent::Ping).unwrap();

    // Append a corrupt line
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, "{{broken json").unwrap();

    journal.append(&DaemonEvent::Pong).unwrap();

    let events = SessionJournal::replay(&path).unwrap();
    // Should recover 2 valid events, skip the corrupt one
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn test_journal_truncate() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("journal.jsonl");

    let mut journal = SessionJournal::new(&path).unwrap();
    journal.append(&DaemonEvent::Ping).unwrap();
    journal.append(&DaemonEvent::Pong).unwrap();

    journal.truncate().unwrap();

    let events = SessionJournal::replay(&path).unwrap();
    assert_eq!(events.len(), 0);
}

#[tokio::test]
async fn test_journal_nonexistent_path_creates_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sub/dir/journal.jsonl");

    let mut journal = SessionJournal::new(&path).unwrap();
    journal.append(&DaemonEvent::Ping).unwrap();

    assert!(path.exists());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_remote --test session_tests`
Expected: FAIL — `SessionJournal` not defined.

**Step 3: Implement session.rs**

```rust
// src/session.rs
use crate::protocol::DaemonEvent;
use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Append-only event journal for session persistence.
/// Each line is a JSON-serialized DaemonEvent.
/// On recovery, replay the journal to reconstruct state.
pub struct SessionJournal {
    path: PathBuf,
    writer: File,
}

impl SessionJournal {
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create journal dir: {}", parent.display()))?;
        }
        let writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("Failed to open journal: {}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
        })
    }

    pub fn append(&mut self, event: &DaemonEvent) -> Result<()> {
        let json = serde_json::to_string(event)?;
        writeln!(self.writer, "{}", json)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn replay(path: &Path) -> Result<Vec<DaemonEvent>> {
        if !path.exists() {
            return Ok(vec![]);
        }
        let file = File::open(path)
            .with_context(|| format!("Failed to open journal for replay: {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<DaemonEvent>(trimmed) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!("Skipping corrupt journal line: {}", e);
                    continue;
                }
            }
        }
        Ok(events)
    }

    pub fn truncate(&mut self) -> Result<()> {
        // Close current writer, truncate file, reopen
        drop(std::mem::replace(
            &mut self.writer,
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&self.path)?,
        ));
        // Reopen in append mode
        self.writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_remote --test session_tests`
Expected: All 5 tests PASS.

**Step 5: Commit**

```bash
git add hive/crates/hive_remote/src/session.rs hive/crates/hive_remote/tests/session_tests.rs
git commit -m "feat(hive_remote): add SessionJournal with append-only persistence and replay"
```

---

## Task 4: QR code generation (qr.rs)

**Files:**
- Create: `hive/crates/hive_remote/src/qr.rs`
- Create: `hive/crates/hive_remote/tests/qr_tests.rs`

**Step 1: Write failing tests**

```rust
// tests/qr_tests.rs
use hive_remote::qr::{generate_pairing_qr, PairingQrPayload};

#[test]
fn test_pairing_qr_payload_to_url() {
    let payload = PairingQrPayload {
        session_id: "abc-123".into(),
        public_key_b64: "dGVzdC1rZXktZGF0YQ".into(),
        lan_addr: Some("192.168.1.50:9481".into()),
        relay_url: Some("wss://relay.example.com".into()),
        version: 1,
    };
    let url = payload.to_url();
    assert!(url.starts_with("hive://pair?"));
    assert!(url.contains("id=abc-123"));
    assert!(url.contains("pk=dGVzdC1rZXktZGF0YQ"));
    assert!(url.contains("addr="));
    assert!(url.contains("relay="));
    assert!(url.contains("v=1"));
}

#[test]
fn test_pairing_qr_payload_from_url() {
    let url = "hive://pair?id=abc-123&pk=dGVzdC1rZXktZGF0YQ&addr=192.168.1.50:9481&relay=wss://relay.example.com&v=1";
    let payload = PairingQrPayload::from_url(url).unwrap();
    assert_eq!(payload.session_id, "abc-123");
    assert_eq!(payload.public_key_b64, "dGVzdC1rZXktZGF0YQ");
    assert_eq!(payload.lan_addr, Some("192.168.1.50:9481".into()));
    assert_eq!(payload.version, 1);
}

#[test]
fn test_pairing_qr_payload_minimal() {
    let payload = PairingQrPayload {
        session_id: "x".into(),
        public_key_b64: "key".into(),
        lan_addr: None,
        relay_url: None,
        version: 1,
    };
    let url = payload.to_url();
    assert!(!url.contains("addr="));
    assert!(!url.contains("relay="));

    let decoded = PairingQrPayload::from_url(&url).unwrap();
    assert_eq!(decoded.session_id, "x");
    assert_eq!(decoded.lan_addr, None);
}

#[test]
fn test_generate_qr_code_bytes() {
    let payload = PairingQrPayload {
        session_id: "test-session".into(),
        public_key_b64: "dGVzdA".into(),
        lan_addr: Some("192.168.1.1:9481".into()),
        relay_url: None,
        version: 1,
    };
    let svg = generate_pairing_qr(&payload).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_remote --test qr_tests`
Expected: FAIL.

**Step 3: Implement qr.rs**

```rust
// src/qr.rs
use anyhow::{anyhow, Result};
use qrcode::QrCode;
use qrcode::render::svg;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingQrPayload {
    pub session_id: String,
    pub public_key_b64: String,
    pub lan_addr: Option<String>,
    pub relay_url: Option<String>,
    pub version: u8,
}

impl PairingQrPayload {
    pub fn to_url(&self) -> String {
        let mut params = vec![
            format!("id={}", urlenc(&self.session_id)),
            format!("pk={}", urlenc(&self.public_key_b64)),
        ];
        if let Some(ref addr) = self.lan_addr {
            params.push(format!("addr={}", urlenc(addr)));
        }
        if let Some(ref relay) = self.relay_url {
            params.push(format!("relay={}", urlenc(relay)));
        }
        params.push(format!("v={}", self.version));
        format!("hive://pair?{}", params.join("&"))
    }

    pub fn from_url(url: &str) -> Result<Self> {
        let query = url
            .strip_prefix("hive://pair?")
            .ok_or_else(|| anyhow!("Invalid hive pairing URL"))?;

        let mut id = None;
        let mut pk = None;
        let mut addr = None;
        let mut relay = None;
        let mut version = 1u8;

        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or("");
            let val = parts.next().unwrap_or("");
            let decoded = urldec(val);
            match key {
                "id" => id = Some(decoded),
                "pk" => pk = Some(decoded),
                "addr" => addr = Some(decoded),
                "relay" => relay = Some(decoded),
                "v" => version = decoded.parse().unwrap_or(1),
                _ => {}
            }
        }

        Ok(Self {
            session_id: id.ok_or_else(|| anyhow!("Missing session id"))?,
            public_key_b64: pk.ok_or_else(|| anyhow!("Missing public key"))?,
            lan_addr: addr,
            relay_url: relay,
            version,
        })
    }
}

/// Generate an SVG QR code for the pairing payload.
pub fn generate_pairing_qr(payload: &PairingQrPayload) -> Result<String> {
    let url = payload.to_url();
    let code = QrCode::new(url.as_bytes())
        .map_err(|e| anyhow!("QR generation failed: {}", e))?;
    let svg_string = code
        .render::<svg::Color>()
        .min_dimensions(200, 200)
        .build();
    Ok(svg_string)
}

fn urlenc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

fn urldec(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(
                &s[i + 1..i + 3],
                16,
            ) {
                out.push(val);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_remote --test qr_tests`
Expected: All 4 tests PASS.

**Step 5: Commit**

```bash
git add hive/crates/hive_remote/src/qr.rs hive/crates/hive_remote/tests/qr_tests.rs
git commit -m "feat(hive_remote): add QR code generation and pairing URL encoding"
```

---

## Task 5: Cryptographic pairing (pairing.rs)

**Files:**
- Create: `hive/crates/hive_remote/src/pairing.rs`
- Create: `hive/crates/hive_remote/tests/pairing_tests.rs`

**Step 1: Write failing tests**

```rust
// tests/pairing_tests.rs
use hive_remote::pairing::*;
use tempfile::tempdir;

#[test]
fn test_keypair_generation() {
    let kp = PairingKeypair::generate();
    assert_eq!(kp.public_key_bytes().len(), 32);
}

#[test]
fn test_key_exchange_produces_shared_secret() {
    let initiator = PairingKeypair::generate();
    let joiner = PairingKeypair::generate();

    let initiator_shared = initiator.derive_shared_secret(joiner.public_key_bytes());
    let joiner_shared = joiner.derive_shared_secret(initiator.public_key_bytes());

    // Both sides derive the same shared secret
    assert_eq!(initiator_shared, joiner_shared);
}

#[test]
fn test_session_keys_derivation() {
    let initiator = PairingKeypair::generate();
    let joiner = PairingKeypair::generate();

    let shared = initiator.derive_shared_secret(joiner.public_key_bytes());
    let session_id = "test-session-001";

    let keys = SessionKeys::derive(&shared, session_id);

    assert_eq!(keys.session_key.len(), 32);
    assert_eq!(keys.pairing_token.len(), 32);
    assert_eq!(keys.device_id.len(), 16);

    // Same inputs produce same keys
    let keys2 = SessionKeys::derive(&shared, session_id);
    assert_eq!(keys.session_key, keys2.session_key);
    assert_eq!(keys.pairing_token, keys2.pairing_token);
}

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let initiator = PairingKeypair::generate();
    let joiner = PairingKeypair::generate();
    let shared = initiator.derive_shared_secret(joiner.public_key_bytes());
    let keys = SessionKeys::derive(&shared, "sess-1");

    let plaintext = b"Hello, encrypted world!";
    let encrypted = keys.encrypt(plaintext).unwrap();
    let decrypted = keys.decrypt(&encrypted).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_encrypt_different_nonces() {
    let initiator = PairingKeypair::generate();
    let joiner = PairingKeypair::generate();
    let shared = initiator.derive_shared_secret(joiner.public_key_bytes());
    let keys = SessionKeys::derive(&shared, "sess-1");

    let plaintext = b"same content";
    let enc1 = keys.encrypt(plaintext).unwrap();
    let enc2 = keys.encrypt(plaintext).unwrap();

    // Different nonces → different ciphertext
    assert_ne!(enc1, enc2);
}

#[test]
fn test_wrong_key_fails_decrypt() {
    let kp1 = PairingKeypair::generate();
    let kp2 = PairingKeypair::generate();
    let kp3 = PairingKeypair::generate();

    let shared_12 = kp1.derive_shared_secret(kp2.public_key_bytes());
    let shared_13 = kp1.derive_shared_secret(kp3.public_key_bytes());

    let keys_12 = SessionKeys::derive(&shared_12, "s");
    let keys_13 = SessionKeys::derive(&shared_13, "s");

    let encrypted = keys_12.encrypt(b"secret").unwrap();
    let result = keys_13.decrypt(&encrypted);
    assert!(result.is_err());
}

#[test]
fn test_paired_device_store_save_load() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("paired.json");

    let device = PairedDevice {
        device_id: "dev-001".into(),
        device_name: "My Phone".into(),
        public_key_fingerprint: "abc123".into(),
        pairing_token_hash: "hash".into(),
        paired_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
        permissions: DevicePermissions::default(),
    };

    let mut store = PairedDeviceStore::new();
    store.add(device.clone());
    store.save(&path).unwrap();

    let loaded = PairedDeviceStore::load(&path).unwrap();
    assert_eq!(loaded.devices().len(), 1);
    assert_eq!(loaded.devices()[0].device_id, "dev-001");
}

#[test]
fn test_paired_device_store_remove() {
    let mut store = PairedDeviceStore::new();
    store.add(PairedDevice {
        device_id: "dev-001".into(),
        device_name: "Phone".into(),
        public_key_fingerprint: "fp".into(),
        pairing_token_hash: "h".into(),
        paired_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
        permissions: DevicePermissions::default(),
    });
    assert_eq!(store.devices().len(), 1);

    store.remove("dev-001");
    assert_eq!(store.devices().len(), 0);
}

#[test]
fn test_device_permissions_default() {
    let perms = DevicePermissions::default();
    assert!(perms.can_chat);
    assert!(perms.can_control_agents);
    assert!(!perms.read_only);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_remote --test pairing_tests`
Expected: FAIL.

**Step 3: Implement pairing.rs**

```rust
// src/pairing.rs
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};
use std::path::Path;

/// Ephemeral X25519 keypair for pairing.
pub struct PairingKeypair {
    secret: StaticSecret,
    public: PublicKey,
}

impl PairingKeypair {
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn public_key_bytes(&self) -> &[u8; 32] {
        self.public.as_bytes()
    }

    pub fn derive_shared_secret(&self, other_public: &[u8; 32]) -> [u8; 32] {
        let other = PublicKey::from(*other_public);
        let shared = self.secret.diffie_hellman(&other);
        *shared.as_bytes()
    }
}

/// Derived session keys from shared secret.
pub struct SessionKeys {
    pub session_key: [u8; 32],
    pub pairing_token: [u8; 32],
    pub device_id: [u8; 16],
    cipher: Aes256Gcm,
}

impl SessionKeys {
    pub fn derive(shared_secret: &[u8; 32], session_id: &str) -> Self {
        let hkdf = Hkdf::<Sha256>::new(Some(session_id.as_bytes()), shared_secret);

        let mut session_key = [0u8; 32];
        let mut pairing_token = [0u8; 32];
        let mut device_id = [0u8; 16];

        hkdf.expand(b"hive-session-key", &mut session_key)
            .expect("valid length");
        hkdf.expand(b"hive-pairing-token", &mut pairing_token)
            .expect("valid length");
        hkdf.expand(b"hive-device-id", &mut device_id)
            .expect("valid length");

        let cipher = Aes256Gcm::new_from_slice(&session_key)
            .expect("valid key length");

        Self {
            session_key,
            pairing_token,
            device_id,
            cipher,
        }
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        rand::RngCore::fill_bytes(&mut OsRng, &mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self.cipher.encrypt(nonce, plaintext)
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;
        let mut output = Vec::with_capacity(12 + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(anyhow!("Ciphertext too short"));
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        self.cipher.decrypt(nonce, ciphertext)
            .map_err(|e| anyhow!("Decryption failed: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_name: String,
    pub public_key_fingerprint: String,
    pub pairing_token_hash: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub permissions: DevicePermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePermissions {
    pub can_chat: bool,
    pub can_control_agents: bool,
    pub can_modify_settings: bool,
    pub can_access_files: bool,
    pub read_only: bool,
}

impl Default for DevicePermissions {
    fn default() -> Self {
        Self {
            can_chat: true,
            can_control_agents: true,
            can_modify_settings: false,
            can_access_files: false,
            read_only: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PairedDeviceStore {
    devices: Vec<PairedDevice>,
}

impl PairedDeviceStore {
    pub fn new() -> Self {
        Self { devices: vec![] }
    }

    pub fn add(&mut self, device: PairedDevice) {
        self.devices.retain(|d| d.device_id != device.device_id);
        self.devices.push(device);
    }

    pub fn remove(&mut self, device_id: &str) {
        self.devices.retain(|d| d.device_id != device_id);
    }

    pub fn get(&self, device_id: &str) -> Option<&PairedDevice> {
        self.devices.iter().find(|d| d.device_id == device_id)
    }

    pub fn devices(&self) -> &[PairedDevice] {
        &self.devices
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let json = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p hive_remote --test pairing_tests`
Expected: All 9 tests PASS.

**Step 5: Commit**

```bash
git add hive/crates/hive_remote/src/pairing.rs hive/crates/hive_remote/tests/pairing_tests.rs
git commit -m "feat(hive_remote): add X25519 pairing, HKDF key derivation, AES-256-GCM E2E encryption"
```

---

## Task 6: Relay protocol types (relay.rs)

**Files:**
- Create: `hive/crates/hive_remote/src/relay.rs`
- Create: `hive/crates/hive_remote/tests/relay_tests.rs`

**Step 1: Write failing tests**

```rust
// tests/relay_tests.rs
use hive_remote::relay::*;

#[test]
fn test_relay_frame_serialization() {
    let frame = RelayFrame::Register {
        session_token: "tok-123".into(),
        node_id: "peer-abc".into(),
    };
    let json = serde_json::to_string(&frame).unwrap();
    let decoded: RelayFrame = serde_json::from_str(&json).unwrap();
    match decoded {
        RelayFrame::Register { session_token, node_id } => {
            assert_eq!(session_token, "tok-123");
            assert_eq!(node_id, "peer-abc");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_encrypted_envelope() {
    let envelope = EncryptedEnvelope {
        nonce: [1u8; 12],
        ciphertext: vec![0xDE, 0xAD, 0xBE, 0xEF],
        sender_fingerprint: "fp-001".into(),
    };
    let json = serde_json::to_string(&envelope).unwrap();
    let decoded: EncryptedEnvelope = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.nonce, [1u8; 12]);
    assert_eq!(decoded.ciphertext, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn test_forward_frame() {
    let frame = RelayFrame::Forward {
        to: Some("peer-xyz".into()),
        payload: EncryptedEnvelope {
            nonce: [0u8; 12],
            ciphertext: vec![1, 2, 3],
            sender_fingerprint: "me".into(),
        },
    };
    let json = serde_json::to_string(&frame).unwrap();
    assert!(json.contains("forward"));
    assert!(json.contains("peer-xyz"));
}

#[test]
fn test_all_relay_frames_serialize() {
    let frames: Vec<RelayFrame> = vec![
        RelayFrame::Register { session_token: "t".into(), node_id: "n".into() },
        RelayFrame::Authenticate { pairing_token: "p".into() },
        RelayFrame::CreateRoom { room_id: "r".into(), encryption_key_fingerprint: "f".into() },
        RelayFrame::JoinRoom { room_id: "r".into(), pairing_token: "p".into() },
        RelayFrame::LeaveRoom,
        RelayFrame::Forward {
            to: None,
            payload: EncryptedEnvelope {
                nonce: [0u8; 12],
                ciphertext: vec![],
                sender_fingerprint: "s".into(),
            },
        },
        RelayFrame::Ping,
        RelayFrame::Pong,
        RelayFrame::Error { code: 404, message: "Not found".into() },
    ];
    for frame in &frames {
        let json = serde_json::to_string(frame).unwrap();
        let _: RelayFrame = serde_json::from_str(&json).unwrap();
    }
}

#[test]
fn test_relay_config_defaults() {
    let config = RelayConfig::default();
    assert!(config.relay_enabled);
    assert!(config.wan_relay_url.is_none());
    assert!(matches!(config.relay_mode, RelayMode::Client));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_remote --test relay_tests`
Expected: FAIL.

**Step 3: Implement relay.rs**

```rust
// src/relay.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayFrame {
    // Auth
    Register {
        session_token: String,
        node_id: String,
    },
    Authenticate {
        pairing_token: String,
    },

    // Rooms
    CreateRoom {
        room_id: String,
        encryption_key_fingerprint: String,
    },
    JoinRoom {
        room_id: String,
        pairing_token: String,
    },
    LeaveRoom,

    // Data (E2E encrypted)
    Forward {
        to: Option<String>,
        payload: EncryptedEnvelope,
    },

    // Control
    Ping,
    Pong,
    Error {
        code: u16,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub sender_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub relay_enabled: bool,
    pub wan_relay_url: Option<String>,
    pub relay_mode: RelayMode,
    pub lan_relay_port: u16,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            relay_enabled: true,
            wan_relay_url: None,
            relay_mode: RelayMode::Client,
            lan_relay_port: 9482,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayMode {
    Client,
    Server,
    Both,
}
```

**Step 4: Run tests**

Run: `cargo test -p hive_remote --test relay_tests`
Expected: All 5 tests PASS.

**Step 5: Commit**

```bash
git add hive/crates/hive_remote/src/relay.rs hive/crates/hive_remote/tests/relay_tests.rs
git commit -m "feat(hive_remote): add relay protocol types — frames, encrypted envelope, config"
```

---

## Task 7: Add MessageKind::RelayRequest/RelayResponse to hive_network

**Files:**
- Modify: `hive/crates/hive_network/src/message.rs`
- Modify: `hive/crates/hive_network/src/discovery.rs` (add relay_capable flag)

**Step 1: Write failing test**

Add to existing hive_network tests:

```rust
#[test]
fn test_relay_message_kinds() {
    let envelope = Envelope::new(
        PeerId::generate(),
        None,
        MessageKind::RelayRequest,
        serde_json::json!({"target": "peer-xyz", "payload": "encrypted"}),
    );
    let json = envelope.to_json().unwrap();
    let decoded = Envelope::from_json(&json).unwrap();
    assert_eq!(decoded.kind, MessageKind::RelayRequest);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_network`
Expected: FAIL — `RelayRequest` not a variant of `MessageKind`.

**Step 3: Add relay variants to MessageKind**

In `hive/crates/hive_network/src/message.rs`, add after `StateSync`:

```rust
    // ── Relay network ───────────────────────────────────────────
    RelayRequest,
    RelayResponse,
```

And in `dispatch_key()`, add:

```rust
    Self::RelayRequest => "relay_request".to_string(),
    Self::RelayResponse => "relay_response".to_string(),
```

**Step 4: Add relay_capable to Announcement**

In `hive/crates/hive_network/src/discovery.rs`, add to `Announcement` struct:

```rust
    #[serde(default)]
    pub relay_capable: bool,
```

**Step 5: Run tests**

Run: `cargo test -p hive_network`
Expected: All existing tests PASS + new test PASS.

**Step 6: Commit**

```bash
git add hive/crates/hive_network/src/message.rs hive/crates/hive_network/src/discovery.rs
git commit -m "feat(hive_network): add RelayRequest/RelayResponse message kinds and relay_capable discovery"
```

---

## Task 8: HiveDaemon core (daemon.rs)

**Files:**
- Create: `hive/crates/hive_remote/src/daemon.rs`
- Create: `hive/crates/hive_remote/tests/daemon_tests.rs`

**Step 1: Write failing tests**

```rust
// tests/daemon_tests.rs
use hive_remote::daemon::{HiveDaemon, DaemonConfig};
use hive_remote::protocol::{DaemonEvent, SessionSnapshot};
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_daemon_config_defaults() {
    let config = DaemonConfig::default();
    assert_eq!(config.local_port, 9480);
    assert_eq!(config.web_port, 9481);
    assert_eq!(config.shutdown_grace_secs, 30);
}

#[tokio::test]
async fn test_daemon_event_dispatch() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let daemon = HiveDaemon::new(config).unwrap();

    // Should produce a snapshot on request
    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.active_panel, "chat");
    assert!(snapshot.agent_runs.is_empty());
}

#[tokio::test]
async fn test_daemon_handles_switch_panel() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };
    let mut daemon = HiveDaemon::new(config).unwrap();
    daemon.handle_event(DaemonEvent::SwitchPanel("agents".into())).await;

    let snapshot = daemon.get_snapshot();
    assert_eq!(snapshot.active_panel, "agents");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_remote --test daemon_tests`
Expected: FAIL.

**Step 3: Implement daemon.rs**

```rust
// src/daemon.rs
use crate::protocol::{AgentRunSummary, DaemonEvent, SessionSnapshot};
use crate::session::SessionJournal;
use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub data_dir: PathBuf,
    pub local_port: u16,
    pub web_port: u16,
    pub shutdown_grace_secs: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hive");
        Self {
            data_dir,
            local_port: 9480,
            web_port: 9481,
            shutdown_grace_secs: 30,
        }
    }
}

pub struct HiveDaemon {
    config: DaemonConfig,
    journal: SessionJournal,
    active_panel: String,
    active_conversation: Option<String>,
    agent_runs: Vec<AgentRunSummary>,
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl HiveDaemon {
    pub fn new(config: DaemonConfig) -> Result<Self> {
        let journal_path = config.data_dir.join("session_journal.jsonl");
        let journal = SessionJournal::new(&journal_path)?;
        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            config,
            journal,
            active_panel: "chat".into(),
            active_conversation: None,
            agent_runs: vec![],
            event_tx,
        })
    }

    pub fn get_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            active_conversation: self.active_conversation.clone(),
            active_panel: self.active_panel.clone(),
            agent_runs: self.agent_runs.clone(),
            timestamp: Utc::now(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    pub async fn handle_event(&mut self, event: DaemonEvent) {
        // Journal the event
        if let Err(e) = self.journal.append(&event) {
            tracing::error!("Failed to journal event: {}", e);
        }

        match &event {
            DaemonEvent::SwitchPanel(panel) => {
                self.active_panel = panel.clone();
            }
            DaemonEvent::SendMessage { conversation_id, .. } => {
                self.active_conversation = Some(conversation_id.clone());
                // TODO: Forward to ChatService when wired
            }
            DaemonEvent::StartAgentTask { goal, orchestration_mode } => {
                let run_id = uuid::Uuid::new_v4().to_string();
                self.agent_runs.push(AgentRunSummary {
                    run_id: run_id.clone(),
                    goal: goal.clone(),
                    status: "planning".into(),
                    cost_usd: 0.0,
                    elapsed_ms: 0,
                });
                // TODO: Spawn actual orchestration task
            }
            DaemonEvent::CancelAgentTask { run_id } => {
                if let Some(run) = self.agent_runs.iter_mut().find(|r| r.run_id == *run_id) {
                    run.status = "cancelled".into();
                }
            }
            _ => {}
        }

        // Broadcast to all connected clients (GUI + web)
        let _ = self.event_tx.send(event);
    }

    pub async fn replay_journal(&mut self) -> Result<()> {
        let events = SessionJournal::replay(self.journal.path())?;
        for event in events {
            // Replay without re-journaling
            match &event {
                DaemonEvent::SwitchPanel(panel) => {
                    self.active_panel = panel.clone();
                }
                DaemonEvent::SendMessage { conversation_id, .. } => {
                    self.active_conversation = Some(conversation_id.clone());
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p hive_remote --test daemon_tests`
Expected: All 3 tests PASS.

**Step 5: Commit**

```bash
git add hive/crates/hive_remote/src/daemon.rs hive/crates/hive_remote/tests/daemon_tests.rs
git commit -m "feat(hive_remote): add HiveDaemon core with event handling, journaling, and snapshots"
```

---

## Task 9: Web API (web_api.rs + web_server.rs)

**Files:**
- Create: `hive/crates/hive_remote/src/web_api.rs`
- Create: `hive/crates/hive_remote/src/web_server.rs`
- Create: `hive/crates/hive_remote/tests/web_api_tests.rs`

**Step 1: Write failing tests**

```rust
// tests/web_api_tests.rs
use axum::body::Body;
use axum::http::{Request, StatusCode};
use hive_remote::web_server::build_router;
use hive_remote::daemon::{HiveDaemon, DaemonConfig};
use tempfile::tempdir;
use tower::ServiceExt;
use std::sync::Arc;
use tokio::sync::RwLock;

async fn test_daemon() -> Arc<RwLock<HiveDaemon>> {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.into_path(),
        ..DaemonConfig::default()
    };
    Arc::new(RwLock::new(HiveDaemon::new(config).unwrap()))
}

#[tokio::test]
async fn test_get_state_returns_snapshot() {
    let daemon = test_daemon().await;
    let app = build_router(daemon);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/state")
                .header("X-Hive-Token", "test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_post_chat_sends_message() {
    let daemon = test_daemon().await;
    let app = build_router(daemon);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat")
                .header("Content-Type", "application/json")
                .header("X-Hive-Token", "test-token")
                .body(Body::from(
                    r#"{"conversation_id":"c1","content":"Hello","model":"test"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_panel_data() {
    let daemon = test_daemon().await;
    let app = build_router(daemon);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/panels/monitor")
                .header("X-Hive-Token", "test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_remote --test web_api_tests`
Expected: FAIL.

**Step 3: Implement web_api.rs**

```rust
// src/web_api.rs
use crate::daemon::HiveDaemon;
use crate::protocol::DaemonEvent;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type DaemonState = Arc<RwLock<HiveDaemon>>;

pub async fn get_state(
    State(daemon): State<DaemonState>,
) -> Json<serde_json::Value> {
    let daemon = daemon.read().await;
    let snapshot = daemon.get_snapshot();
    Json(serde_json::to_value(snapshot).unwrap_or_default())
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub conversation_id: String,
    pub content: String,
    pub model: String,
}

pub async fn send_message(
    State(daemon): State<DaemonState>,
    Json(req): Json<ChatRequest>,
) -> StatusCode {
    let mut daemon = daemon.write().await;
    daemon.handle_event(DaemonEvent::SendMessage {
        conversation_id: req.conversation_id,
        content: req.content,
        model: req.model,
    }).await;
    StatusCode::OK
}

pub async fn get_panel(
    State(daemon): State<DaemonState>,
    axum::extract::Path(panel_id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let _daemon = daemon.read().await;
    // Placeholder — panel data will be populated per-panel
    Json(serde_json::json!({
        "panel": panel_id,
        "data": {}
    }))
}

#[derive(Deserialize)]
pub struct AgentRequest {
    pub goal: String,
    pub orchestration_mode: String,
}

pub async fn agent_action(
    State(daemon): State<DaemonState>,
    Json(req): Json<AgentRequest>,
) -> Json<serde_json::Value> {
    let mut daemon = daemon.write().await;
    daemon.handle_event(DaemonEvent::StartAgentTask {
        goal: req.goal,
        orchestration_mode: req.orchestration_mode,
    }).await;
    Json(serde_json::json!({"status": "started"}))
}
```

**Step 4: Implement web_server.rs**

```rust
// src/web_server.rs
use crate::web_api::{self, DaemonState};
use axum::Router;
use axum::routing::{get, post};

pub fn build_router(daemon: DaemonState) -> Router {
    Router::new()
        .route("/api/state", get(web_api::get_state))
        .route("/api/chat", post(web_api::send_message))
        .route("/api/panels/{panel_id}", get(web_api::get_panel))
        .route("/api/agents", post(web_api::agent_action))
        .with_state(daemon)
}
```

**Step 5: Run tests**

Run: `cargo test -p hive_remote --test web_api_tests`
Expected: All 3 tests PASS.

**Step 6: Commit**

```bash
git add hive/crates/hive_remote/src/web_api.rs hive/crates/hive_remote/src/web_server.rs hive/crates/hive_remote/tests/web_api_tests.rs
git commit -m "feat(hive_remote): add axum web API — state, chat, panels, agent endpoints"
```

---

## Task 10: WebSocket event stream (web_server.rs extension)

**Files:**
- Modify: `hive/crates/hive_remote/src/web_server.rs`
- Modify: `hive/crates/hive_remote/src/web_api.rs`
- Create: `hive/crates/hive_remote/tests/websocket_tests.rs`

**Step 1: Write failing test**

```rust
// tests/websocket_tests.rs
use hive_remote::daemon::{HiveDaemon, DaemonConfig};
use hive_remote::protocol::DaemonEvent;
use hive_remote::web_server::build_router;
use tempfile::tempdir;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_websocket_receives_events() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.into_path(),
        ..DaemonConfig::default()
    };
    let daemon = Arc::new(RwLock::new(HiveDaemon::new(config).unwrap()));
    let app = build_router(daemon.clone());

    // Start server on random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Connect WebSocket
    let (mut ws, _) = tokio_tungstenite::connect_async(
        format!("ws://{}/ws", addr)
    ).await.unwrap();

    // Send an event through the daemon
    {
        let mut d = daemon.write().await;
        d.handle_event(DaemonEvent::SwitchPanel("agents".into())).await;
    }

    // Read from WebSocket — should get the event
    use futures_util::StreamExt;
    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ws.next()
    ).await;

    assert!(timeout.is_ok(), "Should receive message within 2 seconds");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hive_remote --test websocket_tests`
Expected: FAIL — `/ws` route not defined.

**Step 3: Add WebSocket handler to web_api.rs**

Add to `web_api.rs`:

```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(daemon): State<DaemonState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, daemon))
}

async fn handle_websocket(socket: WebSocket, daemon: DaemonState) {
    let (mut sender, mut receiver) = socket.split();

    // Send initial state snapshot
    {
        let d = daemon.read().await;
        let snapshot = d.get_snapshot();
        let event = DaemonEvent::StateSnapshot(snapshot);
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = sender.send(Message::Text(json.into())).await;
        }
    }

    // Subscribe to daemon events
    let mut rx = {
        let d = daemon.read().await;
        d.subscribe()
    };

    // Forward daemon events to WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Receive client messages and dispatch to daemon
    let daemon_clone = daemon.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(event) = serde_json::from_str::<DaemonEvent>(&text) {
                    let mut d = daemon_clone.write().await;
                    d.handle_event(event).await;
                }
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}
```

**Step 4: Add ws route to web_server.rs**

In `build_router`, add:
```rust
.route("/ws", get(web_api::websocket_handler))
```

**Step 5: Add futures-util to Cargo.toml**

```toml
futures-util = "0.3"
```

**Step 6: Run tests**

Run: `cargo test -p hive_remote --test websocket_tests`
Expected: PASS.

**Step 7: Commit**

```bash
git add hive/crates/hive_remote/
git commit -m "feat(hive_remote): add WebSocket event stream with bidirectional daemon communication"
```

---

## Task 11: Embedded web UI shell

**Files:**
- Create: `hive/crates/hive_remote/web/index.html`
- Create: `hive/crates/hive_remote/web/style.css`
- Create: `hive/crates/hive_remote/web/app.js`
- Modify: `hive/crates/hive_remote/src/web_server.rs` (serve static)

**Step 1: Create index.html**

Minimal SPA shell that connects to WebSocket and renders chat. Full Preact app. Must be <100KB total.

**Step 2: Create style.css**

Theme matching desktop Hive: `#00D4FF` accent, dark background, consistent spacing.

**Step 3: Create app.js**

Preact-based SPA with:
- WebSocket connection to daemon
- Chat panel (send/receive messages, streaming)
- Agent monitor (show agent runs, status, cost)
- Panel navigation sidebar
- Responsive layout for mobile

**Step 4: Embed in binary**

Modify `web_server.rs` to serve via `include_str!` or `rust-embed`:

```rust
pub async fn serve_index() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../web/index.html"))
}

pub async fn serve_css() -> (axum::http::header::HeaderMap, &'static str) {
    let mut headers = axum::http::header::HeaderMap::new();
    headers.insert("content-type", "text/css".parse().unwrap());
    (headers, include_str!("../web/style.css"))
}

pub async fn serve_js() -> (axum::http::header::HeaderMap, &'static str) {
    let mut headers = axum::http::header::HeaderMap::new();
    headers.insert("content-type", "application/javascript".parse().unwrap());
    (headers, include_str!("../web/app.js"))
}
```

Add routes:
```rust
.route("/", get(serve_index))
.route("/style.css", get(serve_css))
.route("/app.js", get(serve_js))
```

**Step 5: Verify it loads**

Run the web server and check `http://localhost:9481/` renders the UI.

**Step 6: Commit**

```bash
git add hive/crates/hive_remote/web/ hive/crates/hive_remote/src/web_server.rs
git commit -m "feat(hive_remote): add embedded web UI shell — Preact SPA with chat and agent monitor"
```

---

## Task 12: Wire hive_remote into hive_app

**Files:**
- Modify: `hive/crates/hive_app/src/main.rs`
- Modify: `hive/crates/hive_core/src/config.rs` (add remote config fields)
- Modify: `hive/crates/hive_core/src/session.rs` (extend SessionState)

**Step 1: Add remote config to HiveConfig**

In `hive/crates/hive_core/src/config.rs`, add to `HiveConfig` struct:

```rust
    // Remote control
    pub remote_enabled: bool,
    pub remote_local_port: u16,
    pub remote_web_port: u16,
    pub remote_auto_start: bool,
```

With defaults: `false`, `9480`, `9481`, `false`.

**Step 2: Extend SessionState**

In `hive/crates/hive_core/src/session.rs`, add:

```rust
    pub daemon_active: bool,
    pub last_agent_runs: Vec<String>,  // run_ids of in-progress tasks
```

**Step 3: Spawn daemon in main.rs**

In `hive/crates/hive_app/src/main.rs`, after config load:

```rust
// Spawn hive_remote daemon if enabled
if config.remote_enabled {
    let daemon_config = hive_remote::daemon::DaemonConfig {
        data_dir: hive_core::config::HiveConfig::base_dir(),
        local_port: config.remote_local_port,
        web_port: config.remote_web_port,
        shutdown_grace_secs: 30,
    };
    // Daemon runs on its own tokio runtime
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let daemon = hive_remote::daemon::HiveDaemon::new(daemon_config).unwrap();
            let daemon = std::sync::Arc::new(tokio::sync::RwLock::new(daemon));
            let router = hive_remote::web_server::build_router(daemon);
            let listener = tokio::net::TcpListener::bind(
                format!("0.0.0.0:{}", config.remote_web_port)
            ).await.unwrap();
            tracing::info!("Remote control web UI at http://0.0.0.0:{}", config.remote_web_port);
            axum::serve(listener, router).await.unwrap();
        });
    });
}
```

**Step 4: Verify it builds**

Run: `cargo build -p hive_app`
Expected: Compiles.

**Step 5: Commit**

```bash
git add hive/crates/hive_app/src/main.rs hive/crates/hive_core/src/config.rs hive/crates/hive_core/src/session.rs
git commit -m "feat: wire hive_remote daemon into hive_app startup"
```

---

## Task 13: Integration test — full stack

**Files:**
- Create: `hive/crates/hive_remote/tests/integration_test.rs`

**Step 1: Write integration test**

```rust
// tests/integration_test.rs
//! Full-stack integration: daemon + web API + WebSocket + pairing
use hive_remote::daemon::{DaemonConfig, HiveDaemon};
use hive_remote::pairing::{PairingKeypair, SessionKeys, PairedDeviceStore};
use hive_remote::protocol::DaemonEvent;
use hive_remote::qr::PairingQrPayload;
use hive_remote::relay::{RelayFrame, EncryptedEnvelope};
use hive_remote::web_server::build_router;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_full_pairing_and_encrypted_communication() {
    // 1. Desktop generates keypair and QR
    let desktop_kp = PairingKeypair::generate();
    let session_id = uuid::Uuid::new_v4().to_string();

    let qr_payload = PairingQrPayload {
        session_id: session_id.clone(),
        public_key_b64: base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            desktop_kp.public_key_bytes(),
        ),
        lan_addr: Some("127.0.0.1:9481".into()),
        relay_url: None,
        version: 1,
    };

    // 2. Phone scans QR, generates own keypair
    let phone_kp = PairingKeypair::generate();

    // 3. Both derive shared secret
    let desktop_shared = desktop_kp.derive_shared_secret(phone_kp.public_key_bytes());
    let phone_shared = phone_kp.derive_shared_secret(desktop_kp.public_key_bytes());
    assert_eq!(desktop_shared, phone_shared);

    // 4. Both derive session keys
    let desktop_keys = SessionKeys::derive(&desktop_shared, &session_id);
    let phone_keys = SessionKeys::derive(&phone_shared, &session_id);

    // 5. Desktop encrypts a DaemonEvent
    let event = DaemonEvent::StreamChunk {
        conversation_id: "conv-1".into(),
        chunk: "Hello from desktop".into(),
    };
    let json = serde_json::to_vec(&event).unwrap();
    let encrypted = desktop_keys.encrypt(&json).unwrap();

    // 6. Phone decrypts it
    let decrypted = phone_keys.decrypt(&encrypted).unwrap();
    let decoded: DaemonEvent = serde_json::from_slice(&decrypted).unwrap();

    match decoded {
        DaemonEvent::StreamChunk { chunk, .. } => {
            assert_eq!(chunk, "Hello from desktop");
        }
        _ => panic!("Wrong event type"),
    }
}

#[tokio::test]
async fn test_daemon_journal_survives_restart() {
    let dir = tempdir().unwrap();
    let config = DaemonConfig {
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };

    // First session
    {
        let mut daemon = HiveDaemon::new(config.clone()).unwrap();
        daemon.handle_event(DaemonEvent::SwitchPanel("agents".into())).await;
        daemon.handle_event(DaemonEvent::SwitchPanel("monitor".into())).await;
    }

    // Second session — replay journal
    {
        let mut daemon = HiveDaemon::new(config).unwrap();
        daemon.replay_journal().await.unwrap();
        let snapshot = daemon.get_snapshot();
        assert_eq!(snapshot.active_panel, "monitor");
    }
}
```

**Step 2: Run integration tests**

Run: `cargo test -p hive_remote --test integration_test`
Expected: All tests PASS.

**Step 3: Commit**

```bash
git add hive/crates/hive_remote/tests/integration_test.rs
git commit -m "test(hive_remote): add full-stack integration tests — pairing + encryption + journal recovery"
```

---

## Task 14: Update Obsidian vault and MEMORY.md

**Files:**
- Update: `H:/WORK/AG/Obsidian/Hive/HiveCode/Architecture/Architecture Overview.md` (add hive_remote to crate graph)
- Update: `H:/WORK/AG/Obsidian/Hive/HiveCode/Roadmap/Roadmap.md` (mark Remote Control as in-progress)

**Step 1: Add hive_remote to Architecture Overview**

Add `hive_remote` to the crate list and dependency graph.

**Step 2: Update Roadmap**

Mark "P2P federation refinement" and "Remote Control" as in-progress under Q3.

**Step 3: Commit**

```bash
git add hive/docs/plans/
git commit -m "docs: update architecture and roadmap for hive_remote"
```

---

## Summary

| Task | Component | Tests | Estimated Time |
|------|-----------|-------|----------------|
| 1 | Scaffold crate | 0 (compile check) | 5 min |
| 2 | DaemonEvent protocol | 4 | 10 min |
| 3 | Session journal | 5 | 10 min |
| 4 | QR code generation | 4 | 10 min |
| 5 | Cryptographic pairing | 9 | 15 min |
| 6 | Relay protocol types | 5 | 10 min |
| 7 | hive_network relay kinds | 1 | 5 min |
| 8 | HiveDaemon core | 3 | 15 min |
| 9 | Web API (axum) | 3 | 15 min |
| 10 | WebSocket event stream | 1 | 10 min |
| 11 | Embedded web UI | 0 (manual) | 30 min |
| 12 | Wire into hive_app | 0 (compile) | 10 min |
| 13 | Integration tests | 2 | 10 min |
| 14 | Docs update | 0 | 5 min |
| **Total** | | **37 tests** | **~2.5 hours** |
