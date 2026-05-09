use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, Result};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

/// End-to-end encryption manager.
/// Uses X25519 for key exchange, HKDF-SHA256 for key derivation, AES-256-GCM for encryption.
/// Each session gets ephemeral keys for forward secrecy.
pub struct E2ECrypto {
    /// Our long-term static secret key
    static_secret: StaticSecret,
    /// Our long-term public key (shared with peers)
    pub public_key: PublicKey,
    /// Shared secrets derived with each peer (peer_id -> shared_secret)
    session_keys: HashMap<String, [u8; 32]>,
    /// Known peer public keys (peer_id -> public_key)
    peer_keys: HashMap<String, PublicKey>,
    /// Persistent storage for keys
    db: sled::Db,
}

/// An encrypted message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    /// Ephemeral public key for this message (forward secrecy)
    pub ephemeral_pubkey: [u8; 32],
    /// AES-256-GCM nonce (96 bits)
    pub nonce: [u8; 12],
    /// Encrypted ciphertext
    pub ciphertext: Vec<u8>,
    /// Sender's long-term public key (so recipient knows who sent it)
    pub sender_pubkey: [u8; 32],
}

/// A group encryption key, derived from a shared server secret.
/// For group channels, we use a symmetric key derived from the server topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupKey {
    pub server_id: String,
    pub key: [u8; 32],
    pub version: u64,
}

impl E2ECrypto {
    pub fn new(db: sled::Db) -> Result<Self> {
        // Load or generate static keypair
        let static_secret = if let Some(key_bytes) = db.get("e2e_static_secret")? {
            let bytes: [u8; 32] = key_bytes
                .as_ref()
                .try_into()
                .map_err(|_| anyhow!("invalid stored key"))?;
            StaticSecret::from(bytes)
        } else {
            let secret = StaticSecret::random_from_rng(OsRng);
            db.insert("e2e_static_secret", secret.to_bytes().as_slice())?;
            secret
        };

        let public_key = PublicKey::from(&static_secret);

        // Load known peer keys
        let mut peer_keys = HashMap::new();
        for entry in db.scan_prefix("peer_pubkey:") {
            if let Ok((key, val)) = entry {
                let peer_id = String::from_utf8_lossy(&key["peer_pubkey:".len()..]).to_string();
                if let Ok(bytes) = val.as_ref().try_into() {
                    let bytes: [u8; 32] = bytes;
                    peer_keys.insert(peer_id, PublicKey::from(bytes));
                }
            }
        }

        Ok(Self {
            static_secret,
            public_key,
            session_keys: HashMap::new(),
            peer_keys,
            db,
        })
    }

    /// Register a peer's public key.
    pub fn register_peer_key(&mut self, peer_id: &str, pubkey: [u8; 32]) -> Result<()> {
        let pk = PublicKey::from(pubkey);
        self.peer_keys.insert(peer_id.to_string(), pk);
        self.db
            .insert(format!("peer_pubkey:{}", peer_id), &pubkey)?;

        // Derive session key via X25519 + HKDF
        let shared_secret = self.static_secret.diffie_hellman(&pk);
        let hk = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
        let mut session_key = [0u8; 32];
        hk.expand(b"murmur-session-v1", &mut session_key)
            .map_err(|_| anyhow!("HKDF expand failed"))?;
        self.session_keys
            .insert(peer_id.to_string(), session_key);

        Ok(())
    }

    /// Encrypt a message for a specific peer (DM).
    /// Uses an ephemeral key for forward secrecy.
    pub fn encrypt_for_peer(&self, peer_id: &str, plaintext: &[u8]) -> Result<EncryptedEnvelope> {
        let peer_pubkey = self
            .peer_keys
            .get(peer_id)
            .ok_or_else(|| anyhow!("unknown peer: {}", peer_id))?;

        // Generate ephemeral keypair for forward secrecy
        let ephemeral_secret = EphemeralSecret::random_from_rng(OsRng);
        let ephemeral_pubkey = PublicKey::from(&ephemeral_secret);

        // Derive encryption key: ECDH(ephemeral, peer_static) + HKDF
        let shared = ephemeral_secret.diffie_hellman(peer_pubkey);
        let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut enc_key = [0u8; 32];
        hk.expand(b"murmur-msg-v1", &mut enc_key)
            .map_err(|_| anyhow!("HKDF expand failed"))?;

        // Encrypt with AES-256-GCM
        let cipher = Aes256Gcm::new_from_slice(&enc_key)?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow!("encryption failed: {}", e))?;

        Ok(EncryptedEnvelope {
            ephemeral_pubkey: ephemeral_pubkey.to_bytes(),
            nonce: nonce_bytes,
            ciphertext,
            sender_pubkey: self.public_key.to_bytes(),
        })
    }

    /// Decrypt a message sent to us.
    pub fn decrypt_envelope(&self, envelope: &EncryptedEnvelope) -> Result<Vec<u8>> {
        let ephemeral_pubkey = PublicKey::from(envelope.ephemeral_pubkey);

        // Derive decryption key: ECDH(our_static, ephemeral_pub) + HKDF
        let shared = self.static_secret.diffie_hellman(&ephemeral_pubkey);
        let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut dec_key = [0u8; 32];
        hk.expand(b"murmur-msg-v1", &mut dec_key)
            .map_err(|_| anyhow!("HKDF expand failed"))?;

        // Decrypt
        let cipher = Aes256Gcm::new_from_slice(&dec_key)?;
        let nonce = Nonce::from_slice(&envelope.nonce);

        cipher
            .decrypt(nonce, envelope.ciphertext.as_ref())
            .map_err(|e| anyhow!("decryption failed: {}", e))
    }

    /// Derive a group encryption key for a server/channel.
    /// All members of the channel derive the same key from the topic string.
    /// This is a simplified approach - a production system would use
    /// Sender Keys or MLS for group encryption.
    pub fn derive_group_key(&self, topic: &str) -> Result<[u8; 32]> {
        let hk = Hkdf::<Sha256>::new(Some(topic.as_bytes()), b"murmur-group-v1");
        let mut key = [0u8; 32];
        hk.expand(b"group-key", &mut key)
            .map_err(|_| anyhow!("HKDF expand failed"))?;
        Ok(key)
    }

    /// Encrypt a message for a group channel.
    pub fn encrypt_for_group(&self, topic: &str, plaintext: &[u8]) -> Result<EncryptedEnvelope> {
        let group_key = self.derive_group_key(topic)?;

        let cipher = Aes256Gcm::new_from_slice(&group_key)?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow!("encryption failed: {}", e))?;

        Ok(EncryptedEnvelope {
            ephemeral_pubkey: [0u8; 32], // Not used for group messages
            nonce: nonce_bytes,
            ciphertext,
            sender_pubkey: self.public_key.to_bytes(),
        })
    }

    /// Decrypt a group message.
    pub fn decrypt_group(&self, topic: &str, envelope: &EncryptedEnvelope) -> Result<Vec<u8>> {
        let group_key = self.derive_group_key(topic)?;

        let cipher = Aes256Gcm::new_from_slice(&group_key)?;
        let nonce = Nonce::from_slice(&envelope.nonce);

        cipher
            .decrypt(nonce, envelope.ciphertext.as_ref())
            .map_err(|e| anyhow!("decryption failed: {}", e))
    }

    /// Get our public key bytes for sharing with peers.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public_key.to_bytes()
    }
}
