use crate::crypto::EncryptedEnvelope;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A stored message waiting to be delivered to an offline peer.
/// Messages are always encrypted - relaying peers cannot read them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    /// Unique message ID
    pub id: String,
    /// Target peer ID (who should receive this)
    pub target_peer_id: String,
    /// The encrypted envelope (opaque to relay peers)
    pub envelope: EncryptedEnvelope,
    /// Topic/channel this belongs to (for routing)
    pub topic: String,
    /// Unix timestamp when stored
    pub stored_at: i64,
    /// TTL in seconds (how long relay peers should hold this)
    pub ttl: u64,
    /// How many relay hops are left
    pub relay_hops: u8,
}

/// Request to retrieve stored messages for a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    /// The peer requesting their messages
    pub peer_id: String,
    /// Only fetch messages after this timestamp
    pub since: i64,
    /// Topics the peer is interested in
    pub topics: Vec<String>,
}

/// Response containing stored messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub messages: Vec<StoredMessage>,
}

/// Distributed store-and-forward message relay.
/// Peers voluntarily cache encrypted messages for offline peers.
/// When a peer comes online, they pull messages from nearby relay peers.
///
/// Security guarantees:
/// - Relay peers CANNOT read message contents (E2E encrypted)
/// - Messages are authenticated by sender's public key
/// - TTL prevents indefinite storage
/// - Hop limit prevents infinite relay chains
pub struct StoreForward {
    /// Messages we're holding for other peers (peer_id -> messages)
    stored: HashMap<String, Vec<StoredMessage>>,
    /// Maximum messages to store per peer
    max_per_peer: usize,
    /// Maximum total messages to store
    max_total: usize,
    /// Default TTL for stored messages (24 hours)
    default_ttl: u64,
    /// Persistent storage
    db: sled::Db,
}

impl StoreForward {
    pub fn new(db: sled::Db) -> Result<Self> {
        // Load any previously stored messages
        let mut stored: HashMap<String, Vec<StoredMessage>> = HashMap::new();
        for entry in db.scan_prefix("relay:") {
            if let Ok((key, val)) = entry {
                let peer_id =
                    String::from_utf8_lossy(&key["relay:".len()..]).to_string();
                if let Ok(msgs) = bincode::deserialize::<Vec<StoredMessage>>(&val) {
                    stored.insert(peer_id, msgs);
                }
            }
        }

        Ok(Self {
            stored,
            max_per_peer: 1000,
            max_total: 10000,
            default_ttl: 86400, // 24 hours
            db,
        })
    }

    /// Store a message for an offline peer.
    /// Returns true if the message was stored, false if storage is full.
    pub fn store_message(&mut self, msg: StoredMessage) -> bool {
        // Check total capacity
        let total: usize = self.stored.values().map(|v| v.len()).sum();
        if total >= self.max_total {
            self.evict_expired();
            let total: usize = self.stored.values().map(|v| v.len()).sum();
            if total >= self.max_total {
                return false;
            }
        }

        let peer_msgs = self
            .stored
            .entry(msg.target_peer_id.clone())
            .or_default();

        // Check per-peer capacity
        if peer_msgs.len() >= self.max_per_peer {
            return false;
        }

        // Don't store duplicates
        if peer_msgs.iter().any(|m| m.id == msg.id) {
            return true; // Already have it
        }

        let target = msg.target_peer_id.clone();
        peer_msgs.push(msg);
        self.persist_peer(&target);
        true
    }

    /// Retrieve and remove stored messages for a peer.
    pub fn pull_messages(&mut self, request: &PullRequest) -> PullResponse {
        let messages = if let Some(stored) = self.stored.get_mut(&request.peer_id) {
            let mut to_deliver = Vec::new();
            stored.retain(|msg| {
                if msg.stored_at >= request.since
                    && (request.topics.is_empty()
                        || request.topics.contains(&msg.topic))
                {
                    to_deliver.push(msg.clone());
                    false // Remove from storage
                } else {
                    true // Keep in storage
                }
            });
            self.persist_peer(&request.peer_id);
            to_deliver
        } else {
            Vec::new()
        };

        PullResponse { messages }
    }

    /// Get count of messages stored for a peer.
    pub fn count_for_peer(&self, peer_id: &str) -> usize {
        self.stored.get(peer_id).map(|v| v.len()).unwrap_or(0)
    }

    /// Evict expired messages across all peers.
    pub fn evict_expired(&mut self) {
        let now = chrono::Utc::now().timestamp();
        let mut changed = Vec::new();
        for (peer_id, msgs) in &mut self.stored {
            let before = msgs.len();
            msgs.retain(|m| now - m.stored_at < m.ttl as i64);
            if msgs.len() != before {
                changed.push(peer_id.clone());
            }
        }
        for peer_id in &changed {
            if let Some(msgs) = self.stored.get(peer_id) {
                self.persist_peer_inner(peer_id, msgs);
            }
        }
        self.stored.retain(|_, v| !v.is_empty());
    }

    /// Check if we should relay a message (has hops remaining).
    pub fn should_relay(msg: &StoredMessage) -> bool {
        msg.relay_hops > 0
    }

    /// Create a relayed copy with decremented hop count.
    pub fn relay_copy(msg: &StoredMessage) -> StoredMessage {
        StoredMessage {
            relay_hops: msg.relay_hops.saturating_sub(1),
            ..msg.clone()
        }
    }

    fn persist_peer(&self, peer_id: &str) {
        if let Some(msgs) = self.stored.get(peer_id) {
            self.persist_peer_inner(peer_id, msgs);
        }
    }

    fn persist_peer_inner(&self, peer_id: &str, msgs: &[StoredMessage]) {
        if let Ok(data) = bincode::serialize(msgs) {
            let _ = self.db.insert(format!("relay:{}", peer_id), data);
        }
    }
}
