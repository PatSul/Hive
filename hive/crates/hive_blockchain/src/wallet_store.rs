use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey as SolanaSigningKey;
use k256::ecdsa::SigningKey as EvmSigningKey;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use tracing::info;

const AES_NONCE_LEN: usize = 12;

/// Supported blockchain networks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chain {
    Ethereum,
    Base,
    Solana,
}

impl Chain {
    /// Human-readable label for the chain.
    pub fn label(&self) -> &'static str {
        match self {
            Chain::Ethereum => "Ethereum Mainnet",
            Chain::Base => "Base Mainnet",
            Chain::Solana => "Solana Mainnet",
        }
    }

    /// EVM chain ID, or 0 for non-EVM chains.
    pub fn chain_id(&self) -> u64 {
        match self {
            Chain::Ethereum => 1,
            Chain::Base => 8453,
            Chain::Solana => 0,
        }
    }

    /// Whether this chain uses the EVM execution model.
    pub fn is_evm(&self) -> bool {
        matches!(self, Chain::Ethereum | Chain::Base)
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A stored wallet entry with encrypted private key material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletEntry {
    pub id: String,
    pub name: String,
    pub chain: Chain,
    pub address: String,
    pub encrypted_key: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

/// Manages a collection of wallets with encrypted key storage.
///
/// Wallets are indexed by their UUID. The store can be persisted to and
/// loaded from a JSON file on disk.
#[derive(Debug, Serialize, Deserialize)]
pub struct WalletStore {
    wallets: HashMap<String, WalletEntry>,
}

impl WalletStore {
    /// Create an empty wallet store.
    pub fn new() -> Self {
        Self {
            wallets: HashMap::new(),
        }
    }

    /// Add a wallet to the store. Returns the generated wallet ID.
    pub fn add_wallet(
        &mut self,
        name: String,
        chain: Chain,
        address: String,
        encrypted_key: Vec<u8>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let entry = WalletEntry {
            id: id.clone(),
            name,
            chain,
            address,
            encrypted_key,
            created_at: Utc::now(),
        };
        info!(wallet_id = %id, chain = ?chain, "wallet added to store");
        self.wallets.insert(id.clone(), entry);
        id
    }

    /// Remove a wallet by ID. Returns the removed entry if it existed.
    pub fn remove_wallet(&mut self, id: &str) -> Option<WalletEntry> {
        let removed = self.wallets.remove(id);
        if removed.is_some() {
            info!(wallet_id = %id, "wallet removed from store");
        }
        removed
    }

    /// List all wallets (without exposing encrypted keys in the returned references).
    pub fn list_wallets(&self) -> Vec<&WalletEntry> {
        self.wallets.values().collect()
    }

    /// Get a single wallet by ID.
    pub fn get_wallet(&self, id: &str) -> Option<&WalletEntry> {
        self.wallets.get(id)
    }

    /// Decrypt the private key for a specific wallet entry.
    pub fn decrypt_wallet_key(&self, id: &str, password: &str) -> Result<Vec<u8>> {
        let wallet = self
            .get_wallet(id)
            .ok_or_else(|| anyhow::anyhow!("wallet not found: {id}"))?;
        decrypt_key(&wallet.encrypted_key, password).context("failed to decrypt wallet key")
    }

    /// Number of wallets in the store.
    pub fn len(&self) -> usize {
        self.wallets.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.wallets.is_empty()
    }

    /// Persist the wallet store to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("failed to serialize wallet store")?;
        std::fs::write(path, json).context("failed to write wallet store file")?;

        // Restrict file permissions to owner-only on Unix (0o600 = rw-------).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .context("failed to set wallet store file permissions")?;
        }

        info!(path = %path.display(), count = self.wallets.len(), "wallet store saved");
        Ok(())
    }

    /// Load a wallet store from a JSON file. Returns an empty store if the file
    /// does not exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            info!(path = %path.display(), "wallet store file not found, starting empty");
            return Ok(Self::new());
        }
        let json = std::fs::read_to_string(path).context("failed to read wallet store file")?;
        let store: Self =
            serde_json::from_str(&json).context("failed to deserialize wallet store")?;
        info!(path = %path.display(), count = store.wallets.len(), "wallet store loaded");
        Ok(store)
    }
}

impl Default for WalletStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate new private key material for the given chain and derive its public address.
pub fn generate_wallet_material(chain: Chain) -> Result<(Vec<u8>, String)> {
    let private_key: [u8; 32] = rand::random();
    let address = derive_wallet_address(chain, &private_key)?;
    Ok((private_key.to_vec(), address))
}

/// Import existing private key material for the given chain and derive its public address.
pub fn import_wallet_material(chain: Chain, input: &str) -> Result<(Vec<u8>, String)> {
    let private_key = parse_private_key(chain, input)?;
    let address = derive_wallet_address(chain, &private_key)?;
    Ok((private_key, address))
}

fn derive_wallet_address(chain: Chain, private_key: &[u8]) -> Result<String> {
    match chain {
        Chain::Ethereum | Chain::Base => derive_evm_address(private_key),
        Chain::Solana => derive_solana_address(private_key),
    }
}

fn derive_evm_address(private_key: &[u8]) -> Result<String> {
    let key_bytes = normalize_32_byte_key(private_key, "EVM private key")?;
    let signing_key = EvmSigningKey::from_bytes((&key_bytes).into())
        .map_err(|e| anyhow::anyhow!("invalid EVM private key: {e}"))?;
    let encoded = signing_key.verifying_key().to_encoded_point(false);
    let public_key = encoded.as_bytes();
    let hash = Keccak256::digest(&public_key[1..]);
    Ok(format!("0x{}", hex_encode(&hash[12..])))
}

fn derive_solana_address(private_key: &[u8]) -> Result<String> {
    let key_bytes = normalize_32_byte_key(private_key, "Solana private key")?;
    let signing_key = SolanaSigningKey::from_bytes(&key_bytes);
    Ok(bs58::encode(signing_key.verifying_key().as_bytes()).into_string())
}

fn parse_private_key(chain: Chain, input: &str) -> Result<Vec<u8>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("private key is required");
    }

    match chain {
        Chain::Ethereum | Chain::Base => {
            let bytes = decode_hex(trimmed)?;
            let key_bytes = normalize_32_byte_key(&bytes, "EVM private key")?;
            Ok(key_bytes.to_vec())
        }
        Chain::Solana => {
            let bytes = if looks_like_hex(trimmed) {
                decode_hex(trimmed)?
            } else {
                bs58::decode(trimmed)
                    .into_vec()
                    .map_err(|e| anyhow::anyhow!("invalid Solana private key: {e}"))?
            };

            if bytes.len() == 64 {
                return Ok(bytes[..32].to_vec());
            }

            let key_bytes = normalize_32_byte_key(&bytes, "Solana private key")?;
            Ok(key_bytes.to_vec())
        }
    }
}

fn normalize_32_byte_key(bytes: &[u8], label: &str) -> Result<[u8; 32]> {
    let slice = match bytes.len() {
        32 => bytes,
        64 => &bytes[..32],
        len => {
            anyhow::bail!("{label} must be 32 bytes (or 64 bytes for Solana keypairs), got {len}")
        }
    };

    let mut array = [0u8; 32];
    array.copy_from_slice(slice);
    Ok(array)
}

fn looks_like_hex(value: &str) -> bool {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    !stripped.is_empty()
        && stripped.len() % 2 == 0
        && stripped.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    if stripped.len() % 2 != 0 {
        anyhow::bail!("hex private key must contain an even number of characters");
    }

    let mut bytes = Vec::with_capacity(stripped.len() / 2);
    for chunk in stripped.as_bytes().chunks_exact(2) {
        let hi = hex_nibble(chunk[0] as char)?;
        let lo = hex_nibble(chunk[1] as char)?;
        bytes.push((hi << 4) | lo);
    }
    Ok(bytes)
}

fn hex_nibble(ch: char) -> Result<u8> {
    match ch {
        '0'..='9' => Ok((ch as u8) - b'0'),
        'a'..='f' => Ok((ch as u8) - b'a' + 10),
        'A'..='F' => Ok((ch as u8) - b'A' + 10),
        _ => anyhow::bail!("invalid hex character `{ch}`"),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

// ---------------------------------------------------------------------------
// Encryption helpers
// ---------------------------------------------------------------------------

/// Derive a 256-bit AES key from a password using Argon2id.
///
/// Uses a fixed salt for deterministic derivation (the same password always
/// produces the same key). Parameters: m=19456 KiB (~19 MB), t=2, p=1.
fn derive_key_from_password(password: &str) -> [u8; 32] {
    use argon2::{Algorithm, Argon2, Params, Version};

    let salt = b"hive-wallet-key-v1";
    let params = Params::new(19_456, 2, 1, Some(32)).expect("valid argon2 params");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .expect("argon2 key derivation");
    key
}

/// Encrypt `plaintext` using AES-256-GCM with a key derived from `password`.
///
/// The returned `Vec<u8>` contains `nonce || ciphertext` (12 bytes nonce followed
/// by the encrypted data including the authentication tag).
pub fn encrypt_key(plaintext: &[u8], password: &str) -> Result<Vec<u8>> {
    let key_bytes = derive_key_from_password(password);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let nonce_bytes: [u8; AES_NONCE_LEN] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypt `ciphertext` (produced by [`encrypt_key`]) using AES-256-GCM with a
/// key derived from `password`.
pub fn decrypt_key(ciphertext: &[u8], password: &str) -> Result<Vec<u8>> {
    if ciphertext.len() < AES_NONCE_LEN {
        anyhow::bail!("ciphertext too short (expected at least {AES_NONCE_LEN} bytes for nonce)");
    }

    let (nonce_bytes, encrypted) = ciphertext.split_at(AES_NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key_bytes = derive_key_from_password(password);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher
        .decrypt(nonce, encrypted)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;

    Ok(plaintext)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let plaintext = b"super-secret-private-key-bytes";
        let password = "hunter2";

        let encrypted = encrypt_key(plaintext, password).unwrap();
        let decrypted = decrypt_key(&encrypted, password).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_produces_different_ciphertexts() {
        let plaintext = b"same-key";
        let password = "same-password";

        let a = encrypt_key(plaintext, password).unwrap();
        let b = encrypt_key(plaintext, password).unwrap();

        // Random nonces should make each encryption unique.
        assert_ne!(a, b);
    }

    #[test]
    fn decrypt_wrong_password_fails() {
        let encrypted = encrypt_key(b"secret", "right-password").unwrap();
        let result = decrypt_key(&encrypted, "wrong-password");

        assert!(result.is_err());
    }

    #[test]
    fn decrypt_truncated_data_fails() {
        let result = decrypt_key(&[0u8; 5], "password");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_empty_data_fails() {
        let result = decrypt_key(&[], "password");
        assert!(result.is_err());
    }

    #[test]
    fn wallet_store_add_and_get() {
        let mut store = WalletStore::new();
        let id = store.add_wallet(
            "My Wallet".into(),
            Chain::Ethereum,
            "0xabc".into(),
            vec![1, 2, 3],
        );

        let wallet = store.get_wallet(&id).unwrap();
        assert_eq!(wallet.name, "My Wallet");
        assert_eq!(wallet.chain, Chain::Ethereum);
        assert_eq!(wallet.address, "0xabc");
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn wallet_store_remove() {
        let mut store = WalletStore::new();
        let id = store.add_wallet(
            "Temp".into(),
            Chain::Solana,
            "SoLaddr".into(),
            vec![4, 5, 6],
        );

        assert_eq!(store.len(), 1);
        let removed = store.remove_wallet(&id);
        assert!(removed.is_some());
        assert!(store.is_empty());
    }

    #[test]
    fn wallet_store_remove_nonexistent() {
        let mut store = WalletStore::new();
        assert!(store.remove_wallet("does-not-exist").is_none());
    }

    #[test]
    fn wallet_store_list() {
        let mut store = WalletStore::new();
        store.add_wallet("W1".into(), Chain::Ethereum, "0x1".into(), vec![]);
        store.add_wallet("W2".into(), Chain::Base, "0x2".into(), vec![]);
        store.add_wallet("W3".into(), Chain::Solana, "sol3".into(), vec![]);

        assert_eq!(store.list_wallets().len(), 3);
    }

    #[test]
    fn generate_wallet_material_returns_evm_address() {
        let (private_key, address) = generate_wallet_material(Chain::Ethereum).unwrap();
        assert_eq!(private_key.len(), 32);
        assert!(address.starts_with("0x"));
        assert_eq!(address.len(), 42);
    }

    #[test]
    fn import_wallet_material_derives_evm_address_from_hex() {
        let private_key = "0x4c0883a69102937d6231471b5dbb6204fe5129617082790f8b1a4d7a8b798f8f";
        let (_, address) = import_wallet_material(Chain::Ethereum, private_key).unwrap();
        assert_eq!(address.len(), 42);
        assert!(address.starts_with("0x"));
    }

    #[test]
    fn generate_wallet_material_returns_solana_address() {
        let (private_key, address) = generate_wallet_material(Chain::Solana).unwrap();
        assert_eq!(private_key.len(), 32);
        let decoded = bs58::decode(address).into_vec().unwrap();
        assert_eq!(decoded.len(), 32);
    }

    #[test]
    fn import_wallet_material_accepts_solana_base58_secret() {
        let secret = vec![7u8; 32];
        let encoded = bs58::encode(&secret).into_string();
        let (private_key, address) = import_wallet_material(Chain::Solana, &encoded).unwrap();
        assert_eq!(private_key, secret);
        let decoded = bs58::decode(address).into_vec().unwrap();
        assert_eq!(decoded.len(), 32);
    }

    #[test]
    fn import_wallet_material_rejects_invalid_length() {
        let error = import_wallet_material(Chain::Base, "0x1234").unwrap_err();
        assert!(error.to_string().contains("must be 32 bytes"));
    }

    #[test]
    fn wallet_store_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallets.json");

        let mut store = WalletStore::new();
        let encrypted = encrypt_key(b"private-key", "pw").unwrap();
        store.add_wallet(
            "Saved Wallet".into(),
            Chain::Base,
            "0xdef".into(),
            encrypted.clone(),
        );

        store.save_to_file(&path).unwrap();
        let loaded = WalletStore::load_from_file(&path).unwrap();

        assert_eq!(loaded.len(), 1);
        let wallet = loaded.list_wallets()[0];
        assert_eq!(wallet.name, "Saved Wallet");
        assert_eq!(wallet.chain, Chain::Base);
        assert_eq!(wallet.encrypted_key, encrypted);
    }

    #[test]
    fn wallet_store_load_missing_file_returns_empty() {
        let path = std::env::temp_dir().join("nonexistent-hive-wallet-store.json");
        let store = WalletStore::load_from_file(&path).unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn chain_properties() {
        assert_eq!(Chain::Ethereum.chain_id(), 1);
        assert_eq!(Chain::Base.chain_id(), 8453);
        assert_eq!(Chain::Solana.chain_id(), 0);

        assert!(Chain::Ethereum.is_evm());
        assert!(Chain::Base.is_evm());
        assert!(!Chain::Solana.is_evm());

        assert_eq!(Chain::Ethereum.label(), "Ethereum Mainnet");
    }

    #[test]
    fn chain_display() {
        assert_eq!(format!("{}", Chain::Solana), "Solana Mainnet");
    }

    #[test]
    fn chain_serde_round_trip() {
        let json = serde_json::to_string(&Chain::Base).unwrap();
        assert_eq!(json, "\"base\"");
        let parsed: Chain = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Chain::Base);
    }
}
