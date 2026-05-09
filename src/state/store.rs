use crate::types::*;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Voice channel state for a user.
#[derive(Debug, Clone)]
pub struct VoicePeerState {
    pub peer_id: String,
    pub display_name: String,
    pub speaking: bool,
    pub muted: bool,
    pub deafened: bool,
}

/// Audio settings.
#[derive(Debug, Clone)]
pub struct AudioSettings {
    /// Selected input device name (None = system default)
    pub input_device: Option<String>,
    /// Selected output device name (None = system default)
    pub output_device: Option<String>,
    /// Input volume 0.0 - 2.0 (1.0 = normal)
    pub input_volume: f32,
    /// Output volume 0.0 - 2.0 (1.0 = normal)
    pub output_volume: f32,
    /// Noise suppression enabled
    pub noise_suppression: bool,
    /// VAD threshold 0.0 - 1.0 (lower = more sensitive)
    pub vad_threshold: f32,
    /// Echo cancellation enabled
    pub echo_cancellation: bool,
    /// Automatic gain control enabled
    pub auto_gain: bool,
    /// Available input devices (populated at runtime)
    pub available_inputs: Vec<String>,
    /// Available output devices (populated at runtime)
    pub available_outputs: Vec<String>,
    /// Mic test active
    pub mic_testing: bool,
    /// Current mic input level (0.0 - 1.0, updated during test)
    pub mic_level: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AudioSettingsPersist {
    input_device: Option<String>,
    output_device: Option<String>,
    input_volume: f32,
    output_volume: f32,
    noise_suppression: bool,
    vad_threshold: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            input_volume: 1.0,
            output_volume: 1.0,
            noise_suppression: true,
            vad_threshold: 0.5,
            echo_cancellation: true,
            auto_gain: true,
            available_inputs: Vec::new(),
            available_outputs: Vec::new(),
            mic_testing: false,
            mic_level: 0.0,
        }
    }
}

/// Persistent application state backed by sled.
pub struct AppState {
    db: sled::Db,
    pub profile: UserProfile,
    pub servers: Vec<Server>,
    pub messages: HashMap<String, Vec<ChatMessage>>,     // topic -> messages
    pub direct_messages: HashMap<String, Vec<DirectMessage>>, // peer_id -> messages
    pub peers: HashMap<String, UserProfile>,              // peer_id -> profile
    pub active_server: Option<usize>,
    pub active_channel: Option<usize>,
    pub active_dm_peer: Option<String>,
    pub input_buffer: String,
    pub voice_active: bool,
    pub voice_muted: bool,
    pub voice_deafened: bool,
    pub screen_sharing: bool,
    /// Peers currently in voice with us (peer_id -> state)
    pub voice_peers: HashMap<String, VoicePeerState>,
    /// Which voice channel we're connected to
    pub voice_channel_name: Option<String>,
    /// Audio settings
    pub audio: AudioSettings,
    /// Our listening addresses (for sharing with friends)
    pub listen_addrs: Vec<String>,
    /// Unread message count per topic
    pub unread: HashMap<String, u32>,
    /// Total unread count (for window title)
    pub total_unread: u32,
}

impl AppState {
    pub fn new(peer_id: String, display_name: String) -> Result<Self> {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("murmur");
        std::fs::create_dir_all(&data_dir)?;

        let db = sled::open(data_dir.join("state.db"))?;

        // Load saved servers or create defaults
        let servers: Vec<Server> = if let Some(data) = db.get("servers")? {
            bincode::deserialize(&data).unwrap_or_default()
        } else {
            let default = vec![Server::new("murmur-lobby".to_string())];
            db.insert("servers", bincode::serialize(&default)?)?;
            default
        };

        // Load display name override
        let display_name = if let Some(data) = db.get("display_name")? {
            String::from_utf8(data.to_vec()).unwrap_or(display_name)
        } else {
            db.insert("display_name", display_name.as_bytes())?;
            display_name
        };

        // Load cached peer profiles
        let mut peers: HashMap<String, UserProfile> = HashMap::new();
        if let Ok(Some(data)) = db.get("peers") {
            if let Ok(cached) = bincode::deserialize::<Vec<UserProfile>>(&data) {
                for mut p in cached {
                    p.status = UserStatus::Offline; // All cached peers start offline
                    peers.insert(p.peer_id.clone(), p);
                }
            }
        }

        Ok(Self {
            db,
            profile: UserProfile {
                peer_id,
                display_name,
                status: UserStatus::Online,
            },
            servers,
            messages: HashMap::new(),
            direct_messages: HashMap::new(),
            peers,
            active_server: Some(0),
            active_channel: Some(0),
            active_dm_peer: None,
            input_buffer: String::new(),
            voice_active: false,
            voice_muted: false,
            voice_deafened: false,
            screen_sharing: false,
            voice_peers: HashMap::new(),
            voice_channel_name: None,
            audio: AudioSettings::default(),
            listen_addrs: Vec::new(),
            unread: HashMap::new(),
            total_unread: 0,
        })
    }

    pub fn current_topic(&self) -> Option<String> {
        let server_idx = self.active_server?;
        let channel_idx = self.active_channel?;
        let server = self.servers.get(server_idx)?;
        let channel = server.channels.get(channel_idx)?;
        Some(server.topic_for_channel(&channel.id))
    }

    pub fn current_messages(&self) -> Vec<&ChatMessage> {
        if let Some(topic) = self.current_topic() {
            if let Some(msgs) = self.messages.get(&topic) {
                return msgs.iter().collect();
            }
        }
        vec![]
    }

    pub fn current_dm_messages(&self) -> Vec<&DirectMessage> {
        if let Some(ref peer_id) = self.active_dm_peer {
            if let Some(msgs) = self.direct_messages.get(peer_id) {
                return msgs.iter().collect();
            }
        }
        vec![]
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
        let topic = format!(
            "murmur/server/{}/channel/{}",
            msg.server_id, msg.channel_id
        );
        // Track unread if not from us and not the active channel
        let is_from_us = msg.sender_peer_id == self.profile.peer_id;
        let is_active = self.current_topic().as_ref() == Some(&topic);
        if !is_from_us && !is_active {
            *self.unread.entry(topic.clone()).or_insert(0) += 1;
            self.total_unread += 1;
        }
        self.messages.entry(topic.clone()).or_default().push(msg.clone());
        self.persist_messages(&topic);
    }

    /// Clear unread count for a topic (called when switching to that channel).
    pub fn mark_read(&mut self, topic: &str) {
        if let Some(count) = self.unread.remove(topic) {
            self.total_unread = self.total_unread.saturating_sub(count);
        }
    }

    pub fn add_direct_message(&mut self, msg: DirectMessage) {
        let peer_key = if msg.from_peer_id == self.profile.peer_id {
            msg.to_peer_id.clone()
        } else {
            msg.from_peer_id.clone()
        };
        self.direct_messages.entry(peer_key).or_default().push(msg);
    }

    pub fn add_server(&mut self, server: Server) {
        if !self.servers.iter().any(|s| s.id == server.id) {
            self.servers.push(server);
            self.persist_servers();
        }
    }

    pub fn add_channel_to_server(&mut self, server_id: &str, channel: Channel) {
        if let Some(server) = self.servers.iter_mut().find(|s| s.id == server_id) {
            if !server.channels.iter().any(|c| c.id == channel.id) {
                server.channels.push(channel);
                self.persist_servers();
            }
        }
    }

    fn persist_servers(&self) {
        if let Ok(data) = bincode::serialize(&self.servers) {
            let _ = self.db.insert("servers", data);
        }
    }

    fn persist_messages(&self, topic: &str) {
        if let Some(msgs) = self.messages.get(topic) {
            // Keep last 1000 messages per topic
            let to_save: Vec<_> = msgs.iter().rev().take(1000).cloned().collect();
            if let Ok(data) = bincode::serialize(&to_save) {
                let _ = self.db.insert(format!("msgs:{}", topic), data);
            }
        }
    }

    pub fn load_messages_for_topic(&mut self, topic: &str) {
        if self.messages.contains_key(topic) {
            return;
        }
        if let Ok(Some(data)) = self.db.get(format!("msgs:{}", topic)) {
            if let Ok(mut msgs) = bincode::deserialize::<Vec<ChatMessage>>(&data) {
                // Messages were stored in reverse order, fix that
                msgs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                self.messages.insert(topic.to_string(), msgs);
            }
        }
    }

    /// Load all saved message history for all server channels.
    pub fn load_all_history(&mut self) {
        let topics: Vec<String> = self.servers.iter().flat_map(|s| {
            s.channels.iter().map(move |c| s.topic_for_channel(&c.id))
        }).collect();
        for topic in topics {
            self.load_messages_for_topic(&topic);
        }
        // Also load DM history
        for entry in self.db.scan_prefix("msgs:") {
            if let Ok((key, _)) = entry {
                let topic = String::from_utf8_lossy(&key["msgs:".len()..]).to_string();
                self.load_messages_for_topic(&topic);
            }
        }
    }

    /// Update a peer's profile and persist to disk.
    pub fn update_peer(&mut self, peer_id: String, name: String, status: UserStatus) {
        let profile = self.peers.entry(peer_id.clone()).or_insert(UserProfile {
            peer_id: peer_id.clone(),
            display_name: name.clone(),
            status,
        });
        profile.display_name = name;
        profile.status = status;
        self.persist_peers();
    }

    pub fn persist_peers(&self) {
        let profiles: Vec<UserProfile> = self.peers.values().cloned().collect();
        if let Ok(data) = bincode::serialize(&profiles) {
            let _ = self.db.insert("peers", data);
        }
    }

    pub fn save_audio_settings(&self) {
        if let Ok(data) = serde_json::to_vec(&AudioSettingsPersist {
            input_device: self.audio.input_device.clone(),
            output_device: self.audio.output_device.clone(),
            input_volume: self.audio.input_volume,
            output_volume: self.audio.output_volume,
            noise_suppression: self.audio.noise_suppression,
            vad_threshold: self.audio.vad_threshold,
        }) {
            let _ = self.db.insert("audio_settings", data);
        }
    }

    pub fn load_audio_settings(&mut self) {
        if let Ok(Some(data)) = self.db.get("audio_settings") {
            if let Ok(s) = serde_json::from_slice::<AudioSettingsPersist>(&data) {
                self.audio.input_device = s.input_device;
                self.audio.output_device = s.output_device;
                self.audio.input_volume = s.input_volume;
                self.audio.output_volume = s.output_volume;
                self.audio.noise_suppression = s.noise_suppression;
                self.audio.vad_threshold = s.vad_threshold;
            }
        }
    }

    pub fn set_display_name(&mut self, name: String) {
        self.profile.display_name = name.clone();
        let _ = self.db.insert("display_name", name.as_bytes());
    }
}
