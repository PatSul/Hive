use hive_remote::relay::{EncryptedEnvelope, RelayConfig, RelayFrame, RelayMode};

// ---------------------------------------------------------------------------
// 1. RelayFrame serialization roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_relay_frame_serialization() {
    let frame = RelayFrame::Register {
        session_token: "tok-123".into(),
        node_id: "node-abc".into(),
    };
    let json = serde_json::to_string(&frame).unwrap();
    assert!(json.contains("\"type\":\"register\""));

    let back: RelayFrame = serde_json::from_str(&json).unwrap();
    match back {
        RelayFrame::Register {
            session_token,
            node_id,
        } => {
            assert_eq!(session_token, "tok-123");
            assert_eq!(node_id, "node-abc");
        }
        other => panic!("Expected Register, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// 2. EncryptedEnvelope serialize/deserialize
// ---------------------------------------------------------------------------

#[test]
fn test_encrypted_envelope() {
    let envelope = EncryptedEnvelope {
        nonce: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        ciphertext: vec![0xDE, 0xAD, 0xBE, 0xEF],
        sender_fingerprint: "fp-abc123".into(),
    };
    let json = serde_json::to_string(&envelope).unwrap();
    let back: EncryptedEnvelope = serde_json::from_str(&json).unwrap();

    assert_eq!(back.nonce, envelope.nonce);
    assert_eq!(back.ciphertext, envelope.ciphertext);
    assert_eq!(back.sender_fingerprint, "fp-abc123");
}

// ---------------------------------------------------------------------------
// 3. Forward frame with payload
// ---------------------------------------------------------------------------

#[test]
fn test_forward_frame() {
    let frame = RelayFrame::Forward {
        to: Some("peer-42".into()),
        payload: EncryptedEnvelope {
            nonce: [0u8; 12],
            ciphertext: vec![1, 2, 3],
            sender_fingerprint: "sender-fp".into(),
        },
    };
    let json = serde_json::to_string(&frame).unwrap();
    assert!(json.contains("\"type\":\"forward\""));
    assert!(json.contains("\"to\":\"peer-42\""));

    let back: RelayFrame = serde_json::from_str(&json).unwrap();
    match back {
        RelayFrame::Forward { to, payload } => {
            assert_eq!(to, Some("peer-42".into()));
            assert_eq!(payload.sender_fingerprint, "sender-fp");
            assert_eq!(payload.ciphertext, vec![1, 2, 3]);
        }
        other => panic!("Expected Forward, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// 4. All relay frame variants serialize correctly
// ---------------------------------------------------------------------------

#[test]
fn test_all_relay_frames_serialize() {
    let dummy_envelope = EncryptedEnvelope {
        nonce: [0u8; 12],
        ciphertext: vec![],
        sender_fingerprint: "fp".into(),
    };

    let variants: Vec<(&str, RelayFrame)> = vec![
        (
            "register",
            RelayFrame::Register {
                session_token: "t".into(),
                node_id: "n".into(),
            },
        ),
        (
            "authenticate",
            RelayFrame::Authenticate {
                pairing_token: "p".into(),
            },
        ),
        (
            "create_room",
            RelayFrame::CreateRoom {
                room_id: "r".into(),
                encryption_key_fingerprint: "ekf".into(),
            },
        ),
        (
            "join_room",
            RelayFrame::JoinRoom {
                room_id: "r".into(),
                pairing_token: "p".into(),
            },
        ),
        ("leave_room", RelayFrame::LeaveRoom),
        (
            "forward",
            RelayFrame::Forward {
                to: None,
                payload: dummy_envelope,
            },
        ),
        ("ping", RelayFrame::Ping),
        ("pong", RelayFrame::Pong),
        (
            "error",
            RelayFrame::Error {
                code: 500,
                message: "Internal".into(),
            },
        ),
    ];

    for (expected_tag, frame) in &variants {
        let json = serde_json::to_string(frame).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let actual_tag = parsed["type"].as_str().unwrap_or("MISSING");
        assert_eq!(
            actual_tag, *expected_tag,
            "Frame {:?} should have type tag '{}'",
            frame, expected_tag
        );

        // Verify roundtrip
        let back: RelayFrame = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2, "Roundtrip mismatch for {:?}", frame);
    }
}

// ---------------------------------------------------------------------------
// 5. RelayConfig defaults
// ---------------------------------------------------------------------------

#[test]
fn test_relay_config_defaults() {
    let config = RelayConfig::default();
    assert!(config.relay_enabled, "Relay should be enabled by default");
    assert!(
        config.wan_relay_url.is_none(),
        "WAN relay URL should be None by default"
    );
    assert_eq!(config.relay_mode, RelayMode::Client);
    assert_eq!(config.lan_relay_port, 9482);
}

// ---------------------------------------------------------------------------
// 6. RelayConfig serialization roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_relay_config_serialization() {
    let config = RelayConfig {
        relay_enabled: false,
        wan_relay_url: Some("wss://relay.example.com/ws".into()),
        relay_mode: RelayMode::Both,
        lan_relay_port: 12345,
    };
    let json = serde_json::to_string(&config).unwrap();
    let back: RelayConfig = serde_json::from_str(&json).unwrap();

    assert!(!back.relay_enabled);
    assert_eq!(
        back.wan_relay_url,
        Some("wss://relay.example.com/ws".into())
    );
    assert_eq!(back.relay_mode, RelayMode::Both);
    assert_eq!(back.lan_relay_port, 12345);
}

// ---------------------------------------------------------------------------
// 7. RelayMode variants
// ---------------------------------------------------------------------------

#[test]
fn test_relay_mode_variants() {
    let modes = vec![
        (RelayMode::Client, "\"client\""),
        (RelayMode::Server, "\"server\""),
        (RelayMode::Both, "\"both\""),
    ];
    for (mode, expected_json) in modes {
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, expected_json);
        let back: RelayMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mode);
    }
}

// ---------------------------------------------------------------------------
// 8. Forward with broadcast (to = None)
// ---------------------------------------------------------------------------

#[test]
fn test_forward_broadcast() {
    let frame = RelayFrame::Forward {
        to: None,
        payload: EncryptedEnvelope {
            nonce: [255u8; 12],
            ciphertext: vec![0xCA, 0xFE],
            sender_fingerprint: "broadcast-fp".into(),
        },
    };
    let json = serde_json::to_string(&frame).unwrap();
    assert!(json.contains("\"to\":null"));

    let back: RelayFrame = serde_json::from_str(&json).unwrap();
    match back {
        RelayFrame::Forward { to, .. } => assert!(to.is_none()),
        other => panic!("Expected Forward, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// 9. Error frame
// ---------------------------------------------------------------------------

#[test]
fn test_error_frame() {
    let frame = RelayFrame::Error {
        code: 403,
        message: "Forbidden".into(),
    };
    let json = serde_json::to_string(&frame).unwrap();
    let back: RelayFrame = serde_json::from_str(&json).unwrap();
    match back {
        RelayFrame::Error { code, message } => {
            assert_eq!(code, 403);
            assert_eq!(message, "Forbidden");
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}
