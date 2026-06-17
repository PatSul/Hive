use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use std::fs;
use std::path::{Path, PathBuf};

const AES_NONCE_LEN: usize = 12;
const SALT_LEN: usize = 16;
const SALT_FILENAME: &str = "storage.salt";

/// Environment variable holding an optional user passphrase that is mixed into
/// the Argon2id key derivation.
///
/// # Migration semantics
///
/// * **Unset / empty (default):** key derivation is byte-for-byte identical to
///   the legacy scheme (`"hive-secure-storage-v2:{username}:{home}"`). Existing
///   `~/.hive/keys.enc` files decrypt unchanged — there is zero behavior change.
/// * **Set (non-empty):** the passphrase is appended to the derivation input
///   (`"hive-secure-storage-v2:{username}:{home}:{passphrase}"`), producing a
///   distinct key bound to a user secret. A local attacker with read access to
///   the home directory can no longer reconstruct the key without the
///   passphrase.
///
/// On **decrypt** the passphrase-derived key is tried first; if AEAD
/// authentication fails (e.g. the data was written before the passphrase was
/// set) it transparently falls back to the legacy no-passphrase key. On the
/// next **encrypt/save** the data is rewritten with the passphrase-derived key,
/// migrating it forward. If both keys fail, decryption returns a clear error —
/// data is never wiped or lost.
const VAULT_PASSPHRASE_ENV: &str = "HIVE_VAULT_PASSPHRASE";

/// Reads the optional vault passphrase from the environment, treating an unset
/// or empty value as "no passphrase" (the default, legacy-compatible path).
fn passphrase_from_env() -> Option<String> {
    match std::env::var(VAULT_PASSPHRASE_ENV) {
        Ok(p) if !p.is_empty() => Some(p),
        _ => None,
    }
}

/// Secure storage for API keys and sensitive data.
/// Uses AES-256-GCM encryption with a key derived via Argon2id from
/// machine-specific context, an optional user passphrase
/// (`HIVE_VAULT_PASSPHRASE`), and a persisted random salt.
pub struct SecureStorage {
    /// Primary cipher used for all encryption and the first decrypt attempt.
    /// Bound to the passphrase when one is set, otherwise the legacy key.
    cipher: Aes256Gcm,
    key_material: [u8; 32],
    /// Legacy (no-passphrase) cipher, present only when a passphrase is set.
    /// Used as a decrypt fallback so data written before the passphrase was set
    /// still loads (transparent migration on next save).
    legacy_cipher: Option<Aes256Gcm>,
}

impl SecureStorage {
    /// Create a new SecureStorage with a key derived via Argon2id.
    ///
    /// The salt is loaded from (or generated and saved to) `~/.hive/storage.salt`.
    /// The optional `HIVE_VAULT_PASSPHRASE` environment variable is mixed into
    /// the derivation when set; see [`VAULT_PASSPHRASE_ENV`] for migration
    /// semantics.
    pub fn new() -> Result<Self> {
        let salt_path = Self::default_salt_path()?;
        Self::with_salt_path(&salt_path)
    }

    /// Create a SecureStorage with a salt file at a custom path.
    /// Useful for testing without touching `~/.hive/`.
    ///
    /// Reads the optional `HIVE_VAULT_PASSPHRASE` environment variable at this
    /// construction boundary.
    pub fn with_salt_path(salt_path: &Path) -> Result<Self> {
        let salt = Self::load_or_create_salt(salt_path)?;
        let passphrase = passphrase_from_env();
        Self::from_salt_and_passphrase(&salt, passphrase.as_deref())
    }

    /// Construct from an explicit salt and optional passphrase, deriving the
    /// primary key (and, when a passphrase is set, the legacy fallback key).
    /// This is the single place that turns derivation inputs into ciphers, so
    /// tests can exercise the full encrypt/decrypt/migration logic without
    /// touching the real environment.
    fn from_salt_and_passphrase(
        salt: &[u8; SALT_LEN],
        passphrase: Option<&str>,
    ) -> Result<Self> {
        let key_material = derive_key(salt, passphrase)?;
        let mut storage = Self::from_key_material(key_material);
        // Only keep a legacy fallback cipher when a passphrase is actually set;
        // with no passphrase the primary key already *is* the legacy key.
        if passphrase.is_some() {
            let legacy_material = derive_key(salt, None)?;
            let legacy_key = Key::<Aes256Gcm>::from_slice(&legacy_material);
            storage.legacy_cipher = Some(Aes256Gcm::new(legacy_key));
        }
        Ok(storage)
    }

    /// Create a duplicate instance reusing the same derived key material.
    /// Avoids re-running Argon2 key derivation.
    ///
    /// The legacy fallback cipher is *not* duplicated: callers that obtain a
    /// `SecureStorage` via [`Self::duplicate`] (e.g. the config hot-reload
    /// watcher) only encrypt with the primary key, and any read path that needs
    /// migration fallback goes through a freshly constructed instance.
    pub fn duplicate(&self) -> Self {
        Self::from_key_material(self.key_material)
    }

    fn from_key_material(key_material: [u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(&key_material);
        let cipher = Aes256Gcm::new(key);
        Self {
            cipher,
            key_material,
            legacy_cipher: None,
        }
    }

    /// Encrypt a plaintext string, returning hex-encoded ciphertext.
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let nonce_bytes: [u8; AES_NONCE_LEN] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);

        Ok(hex::encode(result))
    }

    /// Decrypt a hex-encoded ciphertext string.
    ///
    /// Tries the primary (passphrase-aware) key first. When a passphrase is set
    /// and the primary key fails AEAD authentication, falls back to the legacy
    /// no-passphrase key so data written before the passphrase was configured
    /// still loads. If both keys fail, returns a clear error — no data is lost.
    pub fn decrypt(&self, hex_ciphertext: &str) -> Result<String> {
        let data = hex::decode(hex_ciphertext).context("Invalid hex")?;
        if data.len() < AES_NONCE_LEN {
            anyhow::bail!("Ciphertext too short");
        }

        let (nonce_bytes, ciphertext) = data.split_at(AES_NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);

        // Primary (passphrase-aware) key first.
        let primary_err = match self.cipher.decrypt(nonce, ciphertext) {
            Ok(plaintext) => {
                return String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8");
            }
            Err(e) => e,
        };

        // Legacy fallback: only present when a passphrase is set. This lets
        // keys.enc written before the passphrase was configured still decrypt
        // (such entries are migrated forward on the next save).
        if let Some(legacy) = &self.legacy_cipher
            && let Ok(plaintext) = legacy.decrypt(nonce, ciphertext)
        {
            return String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8");
        }

        // Both derivations failed — surface a clear error, never wipe data.
        Err(anyhow::anyhow!("Decryption failed: {primary_err}"))
    }

    /// Returns the default salt file path: `~/.hive/storage.salt`.
    fn default_salt_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".hive").join(SALT_FILENAME))
    }

    /// Load a salt from disk, or generate and persist a new one.
    fn load_or_create_salt(salt_path: &Path) -> Result<[u8; SALT_LEN]> {
        if let Ok(data) = fs::read(salt_path)
            && data.len() == SALT_LEN
        {
            let mut salt = [0u8; SALT_LEN];
            salt.copy_from_slice(&data);
            return Ok(salt);
        }
        // Salt file exists but has wrong length -- regenerate.

        let salt: [u8; SALT_LEN] = rand::random();

        // Ensure parent directory exists.
        if let Some(parent) = salt_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        fs::write(salt_path, salt)
            .with_context(|| format!("Failed to write salt file {}", salt_path.display()))?;

        Ok(salt)
    }

}

/// Build the Argon2id derivation input ("password") from machine-specific
/// context (username + home directory) and an optional user passphrase.
///
/// * **No passphrase:** `"hive-secure-storage-v2:{username}:{home}"` — byte-for-byte
///   identical to the legacy scheme.
/// * **With passphrase:** `"hive-secure-storage-v2:{username}:{home}:{passphrase}"`.
///
/// Kept deterministic so re-runs reproduce the same key.
fn derivation_input(username: &str, home: &str, passphrase: Option<&str>) -> String {
    match passphrase {
        Some(p) => format!("hive-secure-storage-v2:{username}:{home}:{p}"),
        None => format!("hive-secure-storage-v2:{username}:{home}"),
    }
}

/// Run Argon2id over an explicit password and salt, producing a 256-bit key.
///
/// Parameters: Argon2id, m=19456 KiB (~19 MB), t=2 iterations, p=1 lane.
/// These are reasonable defaults that balance security and startup latency.
fn argon2_derive(password: &[u8], salt: &[u8; SALT_LEN]) -> Result<[u8; 32]> {
    let params = Params::new(19_456, 2, 1, Some(32))
        .map_err(|e| anyhow::anyhow!("Invalid Argon2 params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password, salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {e}"))?;

    Ok(key)
}

/// Derive a 256-bit key from the persisted salt, machine-specific context, and
/// an optional user passphrase. Pure with respect to the environment: the
/// passphrase is passed in explicitly so it can be unit-tested without touching
/// the real `HIVE_VAULT_PASSPHRASE` env var.
///
/// With `passphrase == None` the output is identical to the legacy scheme, so
/// existing `keys.enc` files decrypt unchanged.
fn derive_key(salt: &[u8; SALT_LEN], passphrase: Option<&str>) -> Result<[u8; 32]> {
    // Gather machine-specific input material.
    let username = whoami::username();
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let password = derivation_input(&username, &home, passphrase);
    argon2_derive(password.as_bytes(), salt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a SecureStorage whose salt lives in a temp directory.
    fn storage_in(dir: &Path) -> SecureStorage {
        let salt_path = dir.join(SALT_FILENAME);
        SecureStorage::with_salt_path(&salt_path).unwrap()
    }

    /// Helper: create a SecureStorage with an explicit passphrase, bypassing the
    /// environment entirely. This makes passphrase/migration tests deterministic
    /// and free of env-var races (no need to mutate the global process env).
    fn storage_in_with_passphrase(dir: &Path, passphrase: Option<&str>) -> SecureStorage {
        let salt_path = dir.join(SALT_FILENAME);
        let salt = SecureStorage::load_or_create_salt(&salt_path).unwrap();
        SecureStorage::from_salt_and_passphrase(&salt, passphrase).unwrap()
    }

    // ---- basic encrypt / decrypt (same as before) ----

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let plaintext = "sk-ant-api03-secret-key-12345";
        let encrypted = storage.encrypt(plaintext).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_produces_hex() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let encrypted = storage.encrypt("test").unwrap();
        assert!(hex::decode(&encrypted).is_ok());
    }

    #[test]
    fn encrypt_different_each_time() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let enc1 = storage.encrypt("same input").unwrap();
        let enc2 = storage.encrypt("same input").unwrap();
        // Different nonces produce different ciphertexts
        assert_ne!(enc1, enc2);
    }

    #[test]
    fn decrypt_invalid_hex() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let result = storage.decrypt("not-hex-zzz");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_too_short() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let result = storage.decrypt("aabb");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_tampered_ciphertext() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let encrypted = storage.encrypt("secret").unwrap();
        let mut bytes = hex::decode(&encrypted).unwrap();
        if bytes.len() > 15 {
            bytes[15] ^= 0xff;
        }
        let tampered = hex::encode(bytes);
        let result = storage.decrypt(&tampered);
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_empty_string() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let encrypted = storage.encrypt("").unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn encrypt_unicode() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let plaintext = "Hello \u{1f30d} \u{4e16}\u{754c} \u{0645}\u{0631}\u{062d}\u{0628}\u{0627}";
        let encrypted = storage.encrypt(plaintext).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_long_string() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(tmp.path());
        let plaintext = "x".repeat(10_000);
        let encrypted = storage.encrypt(&plaintext).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ---- Argon2 key derivation ----

    #[test]
    fn derive_key_deterministic_with_same_salt() {
        let salt: [u8; SALT_LEN] = [1u8; SALT_LEN];
        let password = b"test-password";
        let key1 = argon2_derive(password, &salt).unwrap();
        let key2 = argon2_derive(password, &salt).unwrap();
        assert_eq!(key1, key2, "Same salt + password must produce same key");
    }

    #[test]
    fn derive_key_different_salts_produce_different_keys() {
        let salt_a: [u8; SALT_LEN] = [1u8; SALT_LEN];
        let salt_b: [u8; SALT_LEN] = [2u8; SALT_LEN];
        let password = b"test-password";
        let key_a = argon2_derive(password, &salt_a).unwrap();
        let key_b = argon2_derive(password, &salt_b).unwrap();
        assert_ne!(key_a, key_b, "Different salts must produce different keys");
    }

    #[test]
    fn derive_key_different_passwords_produce_different_keys() {
        let salt: [u8; SALT_LEN] = [42u8; SALT_LEN];
        let key_a = argon2_derive(b"password-a", &salt).unwrap();
        let key_b = argon2_derive(b"password-b", &salt).unwrap();
        assert_ne!(
            key_a, key_b,
            "Different passwords must produce different keys"
        );
    }

    #[test]
    fn derived_key_is_32_bytes() {
        let salt: [u8; SALT_LEN] = [7u8; SALT_LEN];
        let key = argon2_derive(b"any", &salt).unwrap();
        assert_eq!(key.len(), 32);
    }

    // ---- salt file management ----

    #[test]
    fn salt_file_created_on_first_use() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join(SALT_FILENAME);
        assert!(!salt_path.exists());

        let _storage = SecureStorage::with_salt_path(&salt_path).unwrap();

        assert!(salt_path.exists());
        let data = fs::read(&salt_path).unwrap();
        assert_eq!(data.len(), SALT_LEN);
    }

    #[test]
    fn salt_file_reused_on_second_use() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join(SALT_FILENAME);

        // First use creates the salt.
        let storage1 = SecureStorage::with_salt_path(&salt_path).unwrap();
        let salt_bytes_1 = fs::read(&salt_path).unwrap();

        // Encrypt something.
        let encrypted = storage1.encrypt("persisted secret").unwrap();

        // Second use loads the same salt; decryption must succeed.
        let storage2 = SecureStorage::with_salt_path(&salt_path).unwrap();
        let salt_bytes_2 = fs::read(&salt_path).unwrap();
        assert_eq!(
            salt_bytes_1, salt_bytes_2,
            "Salt must be stable across loads"
        );

        let decrypted = storage2.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "persisted secret");
    }

    #[test]
    fn salt_file_in_nested_missing_dir() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join("a").join("b").join("c").join(SALT_FILENAME);
        assert!(!salt_path.exists());

        // Should create intermediate directories and succeed.
        let storage = SecureStorage::with_salt_path(&salt_path).unwrap();
        assert!(salt_path.exists());

        let encrypted = storage.encrypt("nested").unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "nested");
    }

    #[test]
    fn corrupt_salt_file_is_regenerated() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join(SALT_FILENAME);

        // Write a salt file with the wrong length.
        fs::write(&salt_path, b"too-short").unwrap();

        // Should regenerate a valid salt.
        let _storage = SecureStorage::with_salt_path(&salt_path).unwrap();
        let data = fs::read(&salt_path).unwrap();
        assert_eq!(data.len(), SALT_LEN);
    }

    // ---- round-trip with argon2-derived key (integration) ----

    #[test]
    fn roundtrip_with_argon2_derived_key() {
        let tmp = TempDir::new().unwrap();
        let salt_path = tmp.path().join(SALT_FILENAME);
        let storage = SecureStorage::with_salt_path(&salt_path).unwrap();

        let secrets = ["sk-ant-api03-very-secret", "", "short", &"A".repeat(5_000)];
        for secret in &secrets {
            let enc = storage.encrypt(secret).unwrap();
            let dec = storage.decrypt(&enc).unwrap();
            assert_eq!(&dec, secret);
        }
    }

    #[test]
    fn different_salt_files_cannot_cross_decrypt() {
        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();
        let storage_a = storage_in(tmp_a.path());
        let storage_b = storage_in(tmp_b.path());

        let encrypted = storage_a.encrypt("only for A").unwrap();
        // storage_b has a different random salt, so decryption must fail.
        let result = storage_b.decrypt(&encrypted);
        assert!(
            result.is_err(),
            "Different salts must prevent cross-decryption"
        );
    }

    // ---- vault passphrase (HIVE_VAULT_PASSPHRASE) ----
    //
    // These tests pass the passphrase explicitly via
    // `from_salt_and_passphrase` / `derive_key`, so they never touch the real
    // `HIVE_VAULT_PASSPHRASE` env var and are safe to run in parallel.

    /// (1) No passphrase: encrypt then decrypt round-trips — the unchanged
    /// default path.
    #[test]
    fn no_passphrase_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in_with_passphrase(tmp.path(), None);
        let plaintext = "sk-ant-no-passphrase";
        let encrypted = storage.encrypt(plaintext).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// (2) With passphrase: encrypt then decrypt round-trips.
    #[test]
    fn with_passphrase_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in_with_passphrase(tmp.path(), Some("correct horse battery"));
        let plaintext = "sk-ant-with-passphrase";
        let encrypted = storage.encrypt(plaintext).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// (3) MIGRATION: data encrypted with NO passphrase still decrypts after a
    /// passphrase is set — the legacy fallback succeeds. The same salt file is
    /// reused (a passphrase is a user secret, not a salt change).
    #[test]
    fn migration_legacy_data_decrypts_after_passphrase_set() {
        let tmp = TempDir::new().unwrap();

        // Encrypt with NO passphrase (the legacy / pre-upgrade state).
        let legacy = storage_in_with_passphrase(tmp.path(), None);
        let plaintext = "legacy-secret-value";
        let encrypted = legacy.encrypt(plaintext).unwrap();

        // Now a passphrase is configured; same salt file, passphrase added.
        let upgraded = storage_in_with_passphrase(tmp.path(), Some("new-passphrase"));

        // The passphrase-derived primary key cannot decrypt legacy data, so the
        // fallback to the legacy no-passphrase key must succeed.
        let decrypted = upgraded
            .decrypt(&encrypted)
            .expect("legacy data must decrypt via fallback after passphrase set");
        assert_eq!(decrypted, plaintext);

        // Transparent migration: re-encrypting with the passphrase instance and
        // decrypting again still round-trips (now via the primary key).
        let re_encrypted = upgraded.encrypt(plaintext).unwrap();
        assert_eq!(upgraded.decrypt(&re_encrypted).unwrap(), plaintext);
    }

    /// (4) Wrong passphrase: data encrypted with passphrase A fails to decrypt
    /// with passphrase B — cleanly (Err, no panic), and never silently returns
    /// a different plaintext. Uses the same salt so only the passphrase differs.
    #[test]
    fn wrong_passphrase_fails_cleanly() {
        let tmp = TempDir::new().unwrap();

        let storage_a = storage_in_with_passphrase(tmp.path(), Some("passphrase-A"));
        let plaintext = "guarded-by-passphrase-A";
        let encrypted = storage_a.encrypt(plaintext).unwrap();

        // storage_b shares the salt but uses a different passphrase. Its primary
        // key differs from A's, and its legacy-fallback key also differs from
        // A's primary key, so decryption must fail rather than yield junk.
        let storage_b = storage_in_with_passphrase(tmp.path(), Some("passphrase-B"));
        let result = storage_b.decrypt(&encrypted);
        assert!(
            result.is_err(),
            "Wrong passphrase must fail to decrypt, not silently return other data"
        );
    }

    /// (5) Determinism: same salt + same passphrase => same derived key, so a
    /// later run can decrypt data written by an earlier run.
    #[test]
    fn derivation_deterministic_with_passphrase() {
        let salt: [u8; SALT_LEN] = [9u8; SALT_LEN];
        let k1 = derive_key(&salt, Some("steady-passphrase")).unwrap();
        let k2 = derive_key(&salt, Some("steady-passphrase")).unwrap();
        assert_eq!(k1, k2, "Same salt + passphrase must produce same key");

        // And the no-passphrase derivation is likewise deterministic AND
        // distinct from the passphrase-derived key (proves the passphrase is
        // actually mixed in).
        let legacy1 = derive_key(&salt, None).unwrap();
        let legacy2 = derive_key(&salt, None).unwrap();
        assert_eq!(legacy1, legacy2, "No-passphrase derivation must be deterministic");
        assert_ne!(
            k1, legacy1,
            "Passphrase-derived key must differ from the legacy no-passphrase key"
        );
    }

    /// The default (no-passphrase) derivation input is byte-for-byte the legacy
    /// string, guaranteeing existing `keys.enc` files decrypt unchanged.
    #[test]
    fn default_derivation_input_matches_legacy_format() {
        let legacy = derivation_input("alice", "/home/alice", None);
        assert_eq!(legacy, "hive-secure-storage-v2:alice:/home/alice");

        let with_pass = derivation_input("alice", "/home/alice", Some("pw"));
        assert_eq!(with_pass, "hive-secure-storage-v2:alice:/home/alice:pw");
    }
}
