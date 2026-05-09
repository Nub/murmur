use chrono::{DateTime, Utc};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A user identity, derived from their libp2p keypair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub peer_id: String,
    pub display_name: String,
    pub status: UserStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserStatus {
    Online,
    Away,
    DoNotDisturb,
    Offline,
}

impl std::fmt::Display for UserStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserStatus::Online => write!(f, "Online"),
            UserStatus::Away => write!(f, "Away"),
            UserStatus::DoNotDisturb => write!(f, "DND"),
            UserStatus::Offline => write!(f, "Offline"),
        }
    }
}

/// A server (community) is identified by a topic hash.
/// Anyone who knows the server ID can join - truly decentralized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub channels: Vec<Channel>,
}

impl Server {
    pub fn new(name: String) -> Self {
        let id = format!("{:x}", sha2::Sha256::digest(name.as_bytes()))[..16].to_string();
        Self {
            id,
            name: name.clone(),
            channels: vec![Channel {
                id: "general".to_string(),
                name: "general".to_string(),
            }],
        }
    }

    pub fn topic_for_channel(&self, channel_id: &str) -> String {
        format!("murmur/server/{}/channel/{}", self.id, channel_id)
    }
}

use sha2::Digest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

/// A chat message sent over the P2P network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub server_id: String,
    pub channel_id: String,
    pub sender_peer_id: String,
    pub sender_name: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

impl ChatMessage {
    pub fn new(
        server_id: String,
        channel_id: String,
        sender_peer_id: String,
        sender_name: String,
        content: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            server_id,
            channel_id,
            sender_peer_id,
            sender_name,
            content,
            timestamp: Utc::now(),
        }
    }
}

/// A direct message between two peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectMessage {
    pub id: String,
    pub from_peer_id: String,
    pub from_name: String,
    pub to_peer_id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// Voice/screen share signaling between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSignal {
    pub signal_type: VoiceSignalType,
    pub from_peer_id: String,
    pub channel_topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoiceSignalType {
    /// Offer to start a voice/video stream
    Offer { sdp_like: Vec<u8> },
    /// Answer accepting the stream
    Answer { sdp_like: Vec<u8> },
    /// ICE candidate for NAT traversal
    IceCandidate { candidate: String },
    /// Peer joined voice channel
    Join,
    /// Peer left voice channel
    Leave,
    /// Screen share start
    ScreenShareStart,
    /// Screen share stop
    ScreenShareStop,
    /// Speaking state update (VAD-driven)
    Speaking { is_speaking: bool },
    /// Clock sync probe (NTP-style)
    ClockProbe { seq: u64, t1_us: u64 },
    /// Clock sync response
    ClockProbeResponse { seq: u64, t1_us: u64, t2_us: u64, t3_us: u64 },
}

/// Protocol messages exchanged over the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    Chat(ChatMessage),
    Direct(DirectMessage),
    Presence {
        peer_id: String,
        name: String,
        status: UserStatus,
    },
    ServerJoin {
        peer_id: String,
        name: String,
        server_id: String,
    },
    ServerLeave {
        peer_id: String,
        server_id: String,
    },
    ChannelCreate {
        server_id: String,
        channel: Channel,
    },
}

/// Events from the network layer to the UI.
#[derive(Debug, Clone)]
pub enum NetEvent {
    MessageReceived(ChatMessage),
    DirectMessageReceived(DirectMessage),
    PeerDiscovered {
        peer_id: PeerId,
        name: String,
    },
    PeerLeft(PeerId),
    PresenceUpdate {
        peer_id: String,
        name: String,
        status: UserStatus,
    },
    ServerJoined {
        peer_id: String,
        name: String,
        server_id: String,
    },
    ChannelCreated {
        server_id: String,
        channel: Channel,
    },
    PeerSpeaking { peer_id: String, is_speaking: bool },
    Connected,
    ListeningOn(String), // multiaddr string
    Error(String),
}
