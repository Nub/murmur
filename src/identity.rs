use anyhow::{anyhow, Result};
use libp2p::identity::Keypair;
use std::path::PathBuf;
use tracing::info;

/// Persistent identity manager.
/// Saves the libp2p keypair to disk so the peer ID stays stable across restarts.
pub struct Identity {
    keypair: Keypair,
}

const KEY_FILE: &str = "identity.key";

impl Identity {
    /// Load existing keypair from disk, or generate and save a new one.
    pub fn load_or_create(data_dir: &PathBuf) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let key_path = data_dir.join(KEY_FILE);

        let keypair = if key_path.exists() {
            let bytes = std::fs::read(&key_path)?;
            match Keypair::from_protobuf_encoding(&bytes) {
                Ok(kp) => {
                    info!("Loaded existing identity from {}", key_path.display());
                    kp
                }
                Err(_) => {
                    // Corrupted or old format key file — regenerate
                    info!("Identity file corrupted, regenerating");
                    let kp = Keypair::generate_ed25519();
                    Self::save_keypair(&kp, &key_path)?;
                    kp
                }
            }
        } else {
            let kp = Keypair::generate_ed25519();
            Self::save_keypair(&kp, &key_path)?;
            info!("Generated new identity, saved to {}", key_path.display());
            kp
        };

        Ok(Self { keypair })
    }

    fn save_keypair(kp: &Keypair, path: &PathBuf) -> Result<()> {
        let bytes = kp
            .to_protobuf_encoding()
            .map_err(|e| anyhow!("failed to encode keypair: {}", e))?;
        std::fs::write(path, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    pub fn peer_id(&self) -> libp2p::PeerId {
        self.keypair.public().to_peer_id()
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        if let Ok(ed25519_kp) = self.keypair.clone().try_into_ed25519() {
            ed25519_kp.public().to_bytes()
        } else {
            [0u8; 32]
        }
    }
}
