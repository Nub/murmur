use base64::Engine;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerRole {
    Owner,
    Admin,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMember {
    pub peer_id: String,
    pub role: ServerRole,
    pub addrs: Vec<String>, // known multiaddrs for this peer
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub channels: Vec<Channel>,
    /// Known peer addresses for auto-connect
    #[serde(default)]
    pub peers: Vec<ServerMember>,
    /// Who created this server (peer ID)
    #[serde(default)]
    pub owner_peer_id: String,
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
            peers: Vec::new(),
            owner_peer_id: String::new(),
        }
    }

    pub fn new_owned(name: String, owner_peer_id: String) -> Self {
        let mut server = Self::new(name);
        server.owner_peer_id = owner_peer_id.clone();
        server.peers.push(ServerMember {
            peer_id: owner_peer_id,
            role: ServerRole::Owner,
            addrs: Vec::new(),
        });
        server
    }

    pub fn topic_for_channel(&self, channel_id: &str) -> String {
        format!("murmur/server/{}/channel/{}", self.id, channel_id)
    }

    /// Add or update a peer in this server's member list.
    pub fn add_peer(&mut self, peer_id: &str, addrs: Vec<String>) {
        if let Some(member) = self.peers.iter_mut().find(|m| m.peer_id == peer_id) {
            // Merge addresses
            for addr in addrs {
                if !member.addrs.contains(&addr) {
                    member.addrs.push(addr);
                }
            }
        } else {
            self.peers.push(ServerMember {
                peer_id: peer_id.to_string(),
                role: ServerRole::Member,
                addrs,
            });
        }
    }

    /// Get the role of a peer in this server.
    pub fn role_of(&self, peer_id: &str) -> ServerRole {
        self.peers.iter()
            .find(|m| m.peer_id == peer_id)
            .map(|m| m.role)
            .unwrap_or(ServerRole::Member)
    }

    /// Generate an invite code (base64 encoded server name + peer addresses).
    pub fn invite_code(&self) -> String {
        let addrs: Vec<&str> = self.peers.iter()
            .flat_map(|m| m.addrs.iter().map(|a| a.as_str()))
            .collect();
        let invite = format!("{}|{}", self.name, addrs.join(","));
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, invite.as_bytes())
    }

    /// Parse an invite code back into server name + peer addresses.
    pub fn parse_invite(code: &str) -> Option<(String, Vec<String>)> {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, code).ok()?;
        let s = String::from_utf8(bytes).ok()?;
        let mut parts = s.splitn(2, '|');
        let name = parts.next()?.to_string();
        let addrs: Vec<String> = parts.next()
            .map(|a| a.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect())
            .unwrap_or_default();
        Some((name, addrs))
    }
}

use sha2::Digest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

/// File attachment metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttachment {
    pub file_name: String,
    pub file_size: u64,
    pub mime_type: String,
    /// Whether we have the file data locally
    #[serde(default)]
    pub downloaded: bool,
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
    /// ID of the message this is replying to (if any)
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Whether this message has been edited
    #[serde(default)]
    pub edited: bool,
    /// Reactions: emoji -> list of peer IDs who reacted
    #[serde(default)]
    pub reactions: std::collections::HashMap<String, Vec<String>>,
    /// File attachment (if any)
    #[serde(default)]
    pub attachment: Option<FileAttachment>,
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
            reply_to: None,
            edited: false,
            reactions: std::collections::HashMap::new(),
            attachment: None,
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
    Typing {
        peer_id: String,
        name: String,
        channel_topic: String,
    },
    MessageEdit {
        message_id: String,
        new_content: String,
        peer_id: String,
    },
    MessageDelete {
        message_id: String,
        peer_id: String,
    },
    MessageReaction {
        message_id: String,
        emoji: String,
        peer_id: String,
    },
    /// File attachment metadata (actual data sent via request-response)
    FileShare {
        message_id: String,
        file_name: String,
        file_size: u64,
        mime_type: String,
        peer_id: String,
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
    PeerTyping { peer_id: String, name: String },
    MessageEdited { message_id: String, new_content: String, peer_id: String },
    MessageDeleted { message_id: String, peer_id: String },
    MessageReaction { message_id: String, emoji: String, peer_id: String },
    PeerVoiceJoined { peer_id: String, name: String },
    PeerVoiceLeft { peer_id: String },
    Connected,
    ListeningOn(String), // multiaddr string
    Error(String),
}
