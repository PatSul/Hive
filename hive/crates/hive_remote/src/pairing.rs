use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::Path;
use x25519_dalek::{PublicKey, StaticSecret};

// ---------------------------------------------------------------------------
// Key pair for X25519 Diffie-Hellman
// ---------------------------------------------------------------------------

/// An X25519 key pair used during device pairing.
pub struct PairingKeypair {
    secret: StaticSecret,
    public: PublicKey,
}

impl PairingKeypair {
    /// Generate a fresh random key pair using the OS CSPRNG.
    pub fn generate() -> Self {
        let bytes: [u8; 32] = rand::random();
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// The 32-byte public key suitable for sharing.
    pub fn public_key_bytes(&self) -> &[u8; 32] {
        self.public.as_bytes()
    }

    /// Perform X25519 Diffie-Hellman with the other party's public key.
    pub fn derive_shared_secret(&self, other_public: &[u8; 32]) -> [u8; 32] {
        let other = PublicKey::from(*other_public);
        *self.secret.diffie_hellman(&other).as_bytes()
    }
}

// ---------------------------------------------------------------------------
// Session key derivation (HKDF) + AES-256-GCM encryption
// ---------------------------------------------------------------------------

/// Derived session keys for an active pairing.
pub struct SessionKeys {
    pub session_key: [u8; 32],
    pub pairing_token: [u8; 32],
    pub device_id: [u8; 16],
    cipher: Aes256Gcm,
}

impl SessionKeys {
    /// Derive session keys from a shared secret and session identifier.
    ///
    /// Uses HKDF-SHA256 with the `session_id` as salt and expands into
    /// three key materials:
    /// - `session_key`  (32 bytes) — used for AES-256-GCM encryption
    /// - `pairing_token` (32 bytes) — used for mutual authentication
    /// - `device_id` (16 bytes) — a stable identifier for this pairing
    pub fn derive(shared_secret: &[u8; 32], session_id: &str) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(session_id.as_bytes()), shared_secret);

        // Expand into 80 bytes total: 32 + 32 + 16
        let mut okm = [0u8; 80];
        hk.expand(b"hive-remote-session-keys-v1", &mut okm)
            .expect("80 bytes is within HKDF-SHA256 limit");

        let mut session_key = [0u8; 32];
        let mut pairing_token = [0u8; 32];
        let mut device_id = [0u8; 16];

        session_key.copy_from_slice(&okm[0..32]);
        pairing_token.copy_from_slice(&okm[32..64]);
        device_id.copy_from_slice(&okm[64..80]);

        let cipher =
            Aes256Gcm::new_from_slice(&session_key).expect("32-byte key is valid for AES-256-GCM");

        Self {
            session_key,
            pairing_token,
            device_id,
            cipher,
        }
    }

    /// Encrypt `plaintext` with AES-256-GCM using a random 12-byte nonce.
    ///
    /// Output format: `[nonce (12 bytes) | ciphertext]`
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;

        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt data produced by [`Self::encrypt`].
    ///
    /// Expects input format: `[nonce (12 bytes) | ciphertext]`
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(anyhow!("Ciphertext too short (need at least nonce)"));
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow!("Decryption failed: {}", e))
    }
}

// ---------------------------------------------------------------------------
// Paired device registry (persisted to disk)
// ---------------------------------------------------------------------------

/// What a paired device is allowed to do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePermissions {
    pub can_chat: bool,
    pub can_run_agents: bool,
    pub can_view_files: bool,
    pub can_execute_commands: bool,
}

impl Default for DevicePermissions {
    fn default() -> Self {
        Self {
            can_chat: true,
            can_run_agents: false,
            can_view_files: true,
            can_execute_commands: false,
        }
    }
}

/// Metadata for a device that has completed the pairing handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub name: String,
    pub public_key_b64: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub permissions: DevicePermissions,
}

/// Persistent store for paired devices, serialized as JSON on disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PairedDeviceStore {
    pub devices: Vec<PairedDevice>,
}

impl PairedDeviceStore {
    /// Load the store from `path`, returning an empty store if the file
    /// does not exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read device store: {}", e))?;
        let store: Self = serde_json::from_str(&data)
            .map_err(|e| anyhow!("Failed to parse device store: {}", e))?;
        Ok(store)
    }

    /// Save the store to `path`, creating parent directories if needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow!("Failed to create store directory: {}", e))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow!("Failed to serialize device store: {}", e))?;
        std::fs::write(path, data).map_err(|e| anyhow!("Failed to write device store: {}", e))?;
        Ok(())
    }

    /// Add or update a paired device.
    pub fn upsert(&mut self, device: PairedDevice) {
        if let Some(existing) = self
            .devices
            .iter_mut()
            .find(|d| d.device_id == device.device_id)
        {
            *existing = device;
        } else {
            self.devices.push(device);
        }
    }

    /// Remove a device by its ID. Returns `true` if a device was removed.
    pub fn remove(&mut self, device_id: &str) -> bool {
        let before = self.devices.len();
        self.devices.retain(|d| d.device_id != device_id);
        self.devices.len() < before
    }

    /// Look up a device by ID.
    pub fn find(&self, device_id: &str) -> Option<&PairedDevice> {
        self.devices.iter().find(|d| d.device_id == device_id)
    }
}
