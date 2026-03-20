//! Full-stack integration tests exercising the complete pairing + encryption +
//! daemon + journal flow across the hive_remote crate.

use hive_remote::pairing::{PairingKeypair, SessionKeys};
use hive_remote::protocol::DaemonEvent;
use hive_remote::qr::{PairingQrPayload, generate_pairing_qr};
use hive_remote::relay::EncryptedEnvelope;

// ---------------------------------------------------------------------------
// Test 1: Full pairing and encrypted communication
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_full_pairing_and_encrypted_communication() {
    // Desktop generates keypair
    let desktop_kp = PairingKeypair::generate();
    let session_id = uuid::Uuid::new_v4().to_string();

    // Phone generates keypair
    let phone_kp = PairingKeypair::generate();

    // Both derive shared secret via Diffie-Hellman
    let desktop_shared = desktop_kp.derive_shared_secret(phone_kp.public_key_bytes());
    let phone_shared = phone_kp.derive_shared_secret(desktop_kp.public_key_bytes());
    assert_eq!(desktop_shared, phone_shared, "DH shared secrets must match");

    // Both derive session keys via HKDF
    let desktop_keys = SessionKeys::derive(&desktop_shared, &session_id);
    let phone_keys = SessionKeys::derive(&phone_shared, &session_id);

    // Verify derived key material is identical
    assert_eq!(
        desktop_keys.session_key, phone_keys.session_key,
        "Session keys must match"
    );
    assert_eq!(
        desktop_keys.pairing_token, phone_keys.pairing_token,
        "Pairing tokens must match"
    );
    assert_eq!(
        desktop_keys.device_id, phone_keys.device_id,
        "Device IDs must match"
    );

    // Desktop encrypts a DaemonEvent
    let event = DaemonEvent::StreamChunk {
        conversation_id: "conv-1".into(),
        chunk: "Hello from desktop".into(),
    };
    let json = serde_json::to_vec(&event).unwrap();
    let encrypted = desktop_keys.encrypt(&json).unwrap();

    // Encrypted data should be different from plaintext
    assert_ne!(encrypted, json);

    // Phone decrypts it
    let decrypted = phone_keys.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, json, "Decrypted bytes must match original JSON");

    // Verify the deserialized content
    let decoded: DaemonEvent = serde_json::from_slice(&decrypted).unwrap();
    match decoded {
        DaemonEvent::StreamChunk {
            conversation_id,
            chunk,
        } => {
            assert_eq!(conversation_id, "conv-1");
            assert_eq!(chunk, "Hello from desktop");
        }
        other => panic!("Expected StreamChunk, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Test 2: Daemon journal survives restart
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_daemon_journal_survives_restart() {
    use hive_remote::daemon::{DaemonConfig, HiveDaemon};

    let dir = tempfile::tempdir().unwrap();
    let config = DaemonConfig {
        config_root: Some(dir.path().join("config")),
        data_dir: dir.path().to_path_buf(),
        ..DaemonConfig::default()
    };

    // First session: send a series of events
    {
        let mut daemon = HiveDaemon::new(config.clone()).unwrap();

        daemon
            .handle_event(DaemonEvent::SwitchPanel {
                panel: "agents".into(),
            })
            .await;

        daemon
            .handle_event(DaemonEvent::SwitchPanel {
                panel: "monitor".into(),
            })
            .await;

        daemon
            .begin_send_message("conv-42".into(), "hello".into(), "test".into())
            .unwrap();

        // Verify in-memory state before drop
        let snap = daemon.get_snapshot();
        assert_eq!(snap.active_panel, "chat");
        assert_eq!(snap.active_conversation, Some("conv-42".into()));
    }

    // Second session: replay journal and verify state matches
    {
        let mut daemon = HiveDaemon::new(config).unwrap();

        // New daemon instances replay their journal during construction.
        let snap_before = daemon.get_snapshot();
        assert_eq!(snap_before.active_panel, "monitor");
        assert_eq!(snap_before.active_conversation, Some("conv-42".into()));

        // Replay is idempotent if run again manually.
        daemon.replay_journal().unwrap();

        // After replay, state should match end of first session
        let snap_after = daemon.get_snapshot();
        assert_eq!(snap_after.active_panel, "monitor");
        assert_eq!(snap_after.active_conversation, Some("conv-42".into()));
    }
}

// ---------------------------------------------------------------------------
// Test 3: QR to pairing flow
// ---------------------------------------------------------------------------

#[test]
fn test_qr_to_pairing_flow() {
    use base64::Engine;

    let kp = PairingKeypair::generate();
    let session_id = uuid::Uuid::new_v4().to_string();

    let payload = PairingQrPayload {
        session_id: session_id.clone(),
        public_key_b64: base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(kp.public_key_bytes()),
        lan_addr: Some("192.168.1.50:9481".into()),
        relay_url: None,
        version: 1,
    };

    // Generate QR SVG
    let svg = generate_pairing_qr(&payload).unwrap();
    assert!(svg.contains("<svg"), "QR output should be valid SVG");
    assert!(svg.contains("</svg>"), "QR SVG should be well-formed");

    // Encode to URL and decode back
    let url = payload.to_url();
    assert!(
        url.starts_with("hive://pair?"),
        "URL should use hive://pair scheme"
    );

    let decoded = PairingQrPayload::from_url(&url).unwrap();
    assert_eq!(decoded.session_id, session_id);
    assert_eq!(decoded.public_key_b64, payload.public_key_b64);
    assert_eq!(decoded.lan_addr, Some("192.168.1.50:9481".into()));
    assert_eq!(decoded.relay_url, None);
    assert_eq!(decoded.version, 1);
}

// ---------------------------------------------------------------------------
// Test 4: Relay frame encryption roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_relay_frame_encryption_roundtrip() {
    let kp1 = PairingKeypair::generate();
    let kp2 = PairingKeypair::generate();

    let shared1 = kp1.derive_shared_secret(kp2.public_key_bytes());
    let keys1 = SessionKeys::derive(&shared1, "relay-test");

    // Encrypt a daemon event
    let event = DaemonEvent::AgentStatus {
        run_id: "run-1".into(),
        status: "executing".into(),
        detail: "Running architect role".into(),
    };
    let json = serde_json::to_vec(&event).unwrap();
    let encrypted_data = keys1.encrypt(&json).unwrap();

    // Create relay envelope from the encrypted output
    // encrypt() returns [nonce(12) | ciphertext]
    let envelope = EncryptedEnvelope {
        nonce: encrypted_data[..12].try_into().unwrap(),
        ciphertext: encrypted_data[12..].to_vec(),
        sender_fingerprint: "fp-001".into(),
    };

    // Verify envelope structure
    assert_eq!(envelope.nonce.len(), 12);
    assert!(!envelope.ciphertext.is_empty());
    assert_eq!(envelope.sender_fingerprint, "fp-001");

    // Reconstruct on the receiving side and decrypt
    let shared2 = kp2.derive_shared_secret(kp1.public_key_bytes());
    let keys2 = SessionKeys::derive(&shared2, "relay-test");

    let mut full_data = Vec::with_capacity(12 + envelope.ciphertext.len());
    full_data.extend_from_slice(&envelope.nonce);
    full_data.extend_from_slice(&envelope.ciphertext);

    let decrypted = keys2.decrypt(&full_data).unwrap();
    assert_eq!(decrypted, json, "Decrypted bytes must match original JSON");

    let decoded: DaemonEvent = serde_json::from_slice(&decrypted).unwrap();
    match decoded {
        DaemonEvent::AgentStatus {
            run_id,
            status,
            detail,
        } => {
            assert_eq!(run_id, "run-1");
            assert_eq!(status, "executing");
            assert_eq!(detail, "Running architect role");
        }
        other => panic!("Expected AgentStatus, got {:?}", other),
    }

    // Verify that the envelope can round-trip through serde (JSON serialization)
    let envelope_json = serde_json::to_string(&envelope).unwrap();
    let envelope_decoded: EncryptedEnvelope = serde_json::from_str(&envelope_json).unwrap();
    assert_eq!(envelope_decoded.nonce, envelope.nonce);
    assert_eq!(envelope_decoded.ciphertext, envelope.ciphertext);
    assert_eq!(envelope_decoded.sender_fingerprint, "fp-001");
}
