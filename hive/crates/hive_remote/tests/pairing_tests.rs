use chrono::Utc;
use hive_remote::pairing::{
    DevicePermissions, PairedDevice, PairedDeviceStore, PairingKeypair, SessionKeys,
};

// ---------------------------------------------------------------------------
// 1. Keypair generation — 32-byte public key
// ---------------------------------------------------------------------------

#[test]
fn test_keypair_generation() {
    let kp = PairingKeypair::generate();
    let pk = kp.public_key_bytes();
    assert_eq!(pk.len(), 32);

    // Two key pairs should be different (vanishingly unlikely to collide)
    let kp2 = PairingKeypair::generate();
    assert_ne!(kp.public_key_bytes(), kp2.public_key_bytes());
}

// ---------------------------------------------------------------------------
// 2. Key exchange — both sides derive the same shared secret
// ---------------------------------------------------------------------------

#[test]
fn test_key_exchange_produces_shared_secret() {
    let alice = PairingKeypair::generate();
    let bob = PairingKeypair::generate();

    let secret_a = alice.derive_shared_secret(bob.public_key_bytes());
    let secret_b = bob.derive_shared_secret(alice.public_key_bytes());

    assert_eq!(
        secret_a, secret_b,
        "Both sides must derive the same shared secret"
    );
    // Shared secret should not be all zeros
    assert_ne!(secret_a, [0u8; 32]);
}

// ---------------------------------------------------------------------------
// 3. Session key derivation — correct lengths, deterministic
// ---------------------------------------------------------------------------

#[test]
fn test_session_keys_derivation() {
    let shared = [42u8; 32];
    let keys = SessionKeys::derive(&shared, "test-session-1");

    assert_eq!(keys.session_key.len(), 32);
    assert_eq!(keys.pairing_token.len(), 32);
    assert_eq!(keys.device_id.len(), 16);

    // Deterministic: same inputs produce same outputs
    let keys2 = SessionKeys::derive(&shared, "test-session-1");
    assert_eq!(keys.session_key, keys2.session_key);
    assert_eq!(keys.pairing_token, keys2.pairing_token);
    assert_eq!(keys.device_id, keys2.device_id);

    // Different session ID produces different keys
    let keys3 = SessionKeys::derive(&shared, "test-session-2");
    assert_ne!(keys.session_key, keys3.session_key);
}

// ---------------------------------------------------------------------------
// 4. Encrypt/decrypt roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let shared = [7u8; 32];
    let keys = SessionKeys::derive(&shared, "roundtrip-session");

    let plaintext = b"Hello, Hive Remote!";
    let encrypted = keys.encrypt(plaintext).unwrap();

    // Encrypted output must be longer than plaintext (nonce + tag overhead)
    assert!(encrypted.len() > plaintext.len());

    let decrypted = keys.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, plaintext);
}

// ---------------------------------------------------------------------------
// 5. Encrypt produces different ciphertexts (random nonces)
// ---------------------------------------------------------------------------

#[test]
fn test_encrypt_different_nonces() {
    let shared = [99u8; 32];
    let keys = SessionKeys::derive(&shared, "nonce-session");

    let plaintext = b"same input each time";
    let ct1 = keys.encrypt(plaintext).unwrap();
    let ct2 = keys.encrypt(plaintext).unwrap();

    // Same plaintext should produce different ciphertext due to random nonces
    assert_ne!(
        ct1, ct2,
        "Different encryptions should use different nonces"
    );

    // Both should decrypt to the original
    assert_eq!(keys.decrypt(&ct1).unwrap(), plaintext);
    assert_eq!(keys.decrypt(&ct2).unwrap(), plaintext);
}

// ---------------------------------------------------------------------------
// 6. Wrong key fails to decrypt
// ---------------------------------------------------------------------------

#[test]
fn test_wrong_key_fails_decrypt() {
    let shared_a = [1u8; 32];
    let shared_b = [2u8; 32];
    let keys_a = SessionKeys::derive(&shared_a, "session-a");
    let keys_b = SessionKeys::derive(&shared_b, "session-b");

    let plaintext = b"secret data";
    let encrypted = keys_a.encrypt(plaintext).unwrap();

    let result = keys_b.decrypt(&encrypted);
    assert!(result.is_err(), "Decryption with wrong key must fail");
}

// ---------------------------------------------------------------------------
// 7. PairedDeviceStore save/load roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_paired_device_store_save_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("devices.json");

    let mut store = PairedDeviceStore::default();
    let now = Utc::now();
    store.upsert(PairedDevice {
        device_id: "dev-001".into(),
        name: "My Phone".into(),
        public_key_b64: "AAAA".into(),
        paired_at: now,
        last_seen: now,
        permissions: DevicePermissions::default(),
    });
    store.upsert(PairedDevice {
        device_id: "dev-002".into(),
        name: "My Tablet".into(),
        public_key_b64: "BBBB".into(),
        paired_at: now,
        last_seen: now,
        permissions: DevicePermissions {
            can_chat: true,
            can_run_agents: true,
            can_view_files: true,
            can_execute_commands: false,
        },
    });

    store.save(&path).unwrap();

    let loaded = PairedDeviceStore::load(&path).unwrap();
    assert_eq!(loaded.devices.len(), 2);
    assert_eq!(loaded.devices[0].device_id, "dev-001");
    assert_eq!(loaded.devices[0].name, "My Phone");
    assert_eq!(loaded.devices[1].device_id, "dev-002");
    assert!(loaded.devices[1].permissions.can_run_agents);
}

// ---------------------------------------------------------------------------
// 8. PairedDeviceStore remove
// ---------------------------------------------------------------------------

#[test]
fn test_paired_device_store_remove() {
    let mut store = PairedDeviceStore::default();
    let now = Utc::now();
    store.upsert(PairedDevice {
        device_id: "dev-to-remove".into(),
        name: "Old Device".into(),
        public_key_b64: "CCCC".into(),
        paired_at: now,
        last_seen: now,
        permissions: DevicePermissions::default(),
    });
    store.upsert(PairedDevice {
        device_id: "dev-to-keep".into(),
        name: "New Device".into(),
        public_key_b64: "DDDD".into(),
        paired_at: now,
        last_seen: now,
        permissions: DevicePermissions::default(),
    });

    assert_eq!(store.devices.len(), 2);

    let removed = store.remove("dev-to-remove");
    assert!(removed);
    assert_eq!(store.devices.len(), 1);
    assert_eq!(store.devices[0].device_id, "dev-to-keep");

    // Removing a non-existent device returns false
    let removed_again = store.remove("dev-to-remove");
    assert!(!removed_again);
}

// ---------------------------------------------------------------------------
// 9. DevicePermissions defaults
// ---------------------------------------------------------------------------

#[test]
fn test_device_permissions_default() {
    let perms = DevicePermissions::default();
    assert!(perms.can_chat, "Default should allow chat");
    assert!(
        !perms.can_run_agents,
        "Default should NOT allow running agents"
    );
    assert!(perms.can_view_files, "Default should allow viewing files");
    assert!(
        !perms.can_execute_commands,
        "Default should NOT allow executing commands"
    );
}

// ---------------------------------------------------------------------------
// 10. Store upsert updates existing device
// ---------------------------------------------------------------------------

#[test]
fn test_paired_device_store_upsert_updates() {
    let mut store = PairedDeviceStore::default();
    let now = Utc::now();

    store.upsert(PairedDevice {
        device_id: "dev-x".into(),
        name: "Original Name".into(),
        public_key_b64: "KEY1".into(),
        paired_at: now,
        last_seen: now,
        permissions: DevicePermissions::default(),
    });
    assert_eq!(store.devices.len(), 1);
    assert_eq!(store.devices[0].name, "Original Name");

    // Upsert with same device_id should update, not duplicate
    store.upsert(PairedDevice {
        device_id: "dev-x".into(),
        name: "Updated Name".into(),
        public_key_b64: "KEY2".into(),
        paired_at: now,
        last_seen: now,
        permissions: DevicePermissions::default(),
    });
    assert_eq!(store.devices.len(), 1);
    assert_eq!(store.devices[0].name, "Updated Name");
    assert_eq!(store.devices[0].public_key_b64, "KEY2");
}

// ---------------------------------------------------------------------------
// 11. Store load from missing file returns empty
// ---------------------------------------------------------------------------

#[test]
fn test_paired_device_store_load_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.json");
    let store = PairedDeviceStore::load(&path).unwrap();
    assert!(store.devices.is_empty());
}

// ---------------------------------------------------------------------------
// 12. Store find
// ---------------------------------------------------------------------------

#[test]
fn test_paired_device_store_find() {
    let mut store = PairedDeviceStore::default();
    let now = Utc::now();
    store.upsert(PairedDevice {
        device_id: "find-me".into(),
        name: "Findable".into(),
        public_key_b64: "FFFF".into(),
        paired_at: now,
        last_seen: now,
        permissions: DevicePermissions::default(),
    });

    assert!(store.find("find-me").is_some());
    assert_eq!(store.find("find-me").unwrap().name, "Findable");
    assert!(store.find("not-there").is_none());
}

// ---------------------------------------------------------------------------
// 13. Decrypt with truncated data fails
// ---------------------------------------------------------------------------

#[test]
fn test_decrypt_truncated_data() {
    let shared = [5u8; 32];
    let keys = SessionKeys::derive(&shared, "trunc-session");

    // Less than 12 bytes (nonce size) must fail
    let result = keys.decrypt(&[0u8; 5]);
    assert!(result.is_err());
}
