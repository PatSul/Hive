use hive_remote::qr::{generate_pairing_qr, PairingQrPayload};

// ---------------------------------------------------------------------------
// 1. PairingQrPayload::to_url — verify URL format
// ---------------------------------------------------------------------------

#[test]
fn test_pairing_qr_payload_to_url() {
    let payload = PairingQrPayload {
        session_id: "sess-abc-123".into(),
        public_key_b64: "dGVzdGtleQ==".into(),
        lan_addr: Some("192.168.1.100:9481".into()),
        relay_url: Some("wss://relay.hive.example/ws".into()),
        version: 1,
    };
    let url = payload.to_url();

    assert!(url.starts_with("hive://pair?"));
    assert!(url.contains("id=sess-abc-123"));
    assert!(url.contains("pk=dGVzdGtleQ%3D%3D"));
    assert!(url.contains("addr=192.168.1.100%3A9481"));
    assert!(url.contains("relay="));
    assert!(url.contains("v=1"));
}

// ---------------------------------------------------------------------------
// 2. PairingQrPayload::from_url — decode URL back
// ---------------------------------------------------------------------------

#[test]
fn test_pairing_qr_payload_from_url() {
    let original = PairingQrPayload {
        session_id: "sess-roundtrip".into(),
        public_key_b64: "AAAA/BBB+CCC=".into(),
        lan_addr: Some("10.0.0.5:9481".into()),
        relay_url: Some("wss://relay.example.com/ws".into()),
        version: 2,
    };
    let url = original.to_url();
    let decoded = PairingQrPayload::from_url(&url).unwrap();

    assert_eq!(decoded.session_id, original.session_id);
    assert_eq!(decoded.public_key_b64, original.public_key_b64);
    assert_eq!(decoded.lan_addr, original.lan_addr);
    assert_eq!(decoded.relay_url, original.relay_url);
    assert_eq!(decoded.version, original.version);
}

// ---------------------------------------------------------------------------
// 3. Minimal payload — no optional fields
// ---------------------------------------------------------------------------

#[test]
fn test_pairing_qr_payload_minimal() {
    let payload = PairingQrPayload {
        session_id: "minimal-id".into(),
        public_key_b64: "cHVia2V5".into(),
        lan_addr: None,
        relay_url: None,
        version: 1,
    };
    let url = payload.to_url();

    // Should NOT contain addr or relay parameters
    assert!(!url.contains("addr="));
    assert!(!url.contains("relay="));
    assert!(url.contains("id=minimal-id"));
    assert!(url.contains("pk=cHVia2V5"));

    // Roundtrip
    let decoded = PairingQrPayload::from_url(&url).unwrap();
    assert_eq!(decoded.session_id, "minimal-id");
    assert_eq!(decoded.public_key_b64, "cHVia2V5");
    assert!(decoded.lan_addr.is_none());
    assert!(decoded.relay_url.is_none());
}

// ---------------------------------------------------------------------------
// 4. generate_pairing_qr — SVG output contains <svg>
// ---------------------------------------------------------------------------

#[test]
fn test_generate_qr_code_bytes() {
    let payload = PairingQrPayload {
        session_id: "qr-test".into(),
        public_key_b64: "dGVzdA==".into(),
        lan_addr: None,
        relay_url: None,
        version: 1,
    };
    let svg = generate_pairing_qr(&payload).unwrap();

    assert!(svg.contains("<svg"), "SVG output should contain <svg tag");
    assert!(svg.contains("</svg>"), "SVG output should contain closing </svg> tag");
    // SVG should be non-trivial
    assert!(svg.len() > 100, "SVG should be reasonably sized");
}

// ---------------------------------------------------------------------------
// 5. Invalid URL prefix fails
// ---------------------------------------------------------------------------

#[test]
fn test_from_url_invalid_prefix() {
    let result = PairingQrPayload::from_url("https://example.com?id=foo&pk=bar");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// 6. Missing required field fails
// ---------------------------------------------------------------------------

#[test]
fn test_from_url_missing_session_id() {
    let result = PairingQrPayload::from_url("hive://pair?pk=abc&v=1");
    assert!(result.is_err());
}

#[test]
fn test_from_url_missing_public_key() {
    let result = PairingQrPayload::from_url("hive://pair?id=abc&v=1");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// 7. Special characters in payload survive roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_special_characters_roundtrip() {
    let payload = PairingQrPayload {
        session_id: "id with spaces & symbols!".into(),
        public_key_b64: "a+b/c=d==".into(),
        lan_addr: Some("[::1]:9481".into()),
        relay_url: Some("wss://relay.example.com/path?q=1&r=2".into()),
        version: 3,
    };
    let url = payload.to_url();
    let decoded = PairingQrPayload::from_url(&url).unwrap();

    assert_eq!(decoded.session_id, payload.session_id);
    assert_eq!(decoded.public_key_b64, payload.public_key_b64);
    assert_eq!(decoded.lan_addr, payload.lan_addr);
    assert_eq!(decoded.relay_url, payload.relay_url);
    assert_eq!(decoded.version, payload.version);
}
