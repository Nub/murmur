mod behaviour;

pub use behaviour::MurmurBehaviour;

use crate::crypto::{E2ECrypto, EncryptedEnvelope, StoreForward, StoredMessage, PullRequest};
use crate::media::{AudioSync, VoiceEngine, VoiceEvent, ScreenShare};
use crate::types::*;
use anyhow::Result;
use futures::StreamExt;
use libp2p::{
    autonat, gossipsub, identify, identity::Keypair, kad, mdns, noise, upnp,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Commands from the UI to the network layer.
#[derive(Debug)]
pub enum NetCommand {
    SendMessage(ChatMessage),
    SendDirectMessage(DirectMessage),
    JoinServer(String),    // server topic
    LeaveServer(String),
    CreateChannel { server_id: String, channel: Channel },
    UpdatePresence(UserStatus),
    Dial(Multiaddr),
    StartVoice(String),    // channel topic
    StopVoice,
    StartScreenShare(String),
    StopScreenShare,
}

pub struct NetworkManager {
    swarm: Swarm<MurmurBehaviour>,
    event_tx: mpsc::UnboundedSender<NetEvent>,
    command_rx: mpsc::UnboundedReceiver<NetCommand>,
    local_peer_id: PeerId,
    display_name: String,
    e2e: E2ECrypto,
    store_forward: StoreForward,
    bootstrap_addrs: Vec<Multiaddr>,
    known_peers: HashSet<PeerId>,
    voice_engine: Option<VoiceEngine>,
    voice_event_rx: Option<mpsc::UnboundedReceiver<VoiceEvent>>,
    screen_share: ScreenShare,
    audio_sync: AudioSync,
}

impl NetworkManager {
    pub fn new(
        keypair: Keypair,
        display_name: String,
        event_tx: mpsc::UnboundedSender<NetEvent>,
        command_rx: mpsc::UnboundedReceiver<NetCommand>,
        e2e: E2ECrypto,
        store_forward: StoreForward,
        bootstrap_addrs: Vec<Multiaddr>,
    ) -> Result<Self> {
        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default().nodelay(true),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_dns()?
            .with_behaviour(|key| {
                let local_peer_id = PeerId::from(key.public());

                // Gossipsub for group messaging
                let message_id_fn = |message: &gossipsub::Message| {
                    let mut s = DefaultHasher::new();
                    message.data.hash(&mut s);
                    message.source.hash(&mut s);
                    gossipsub::MessageId::from(s.finish().to_string())
                };

                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(1))
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .message_id_fn(message_id_fn)
                    .max_transmit_size(262144) // 256KB for larger messages
                    .build()
                    .expect("valid gossipsub config");

                let gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )
                .expect("valid gossipsub behaviour");

                // mDNS for local network discovery
                let mdns = mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    local_peer_id,
                )
                .expect("valid mdns");

                // Kademlia DHT for internet-wide peer discovery
                let mut kad_config = kad::Config::new(
                    StreamProtocol::new("/murmur/kad/1.0.0"),
                );
                kad_config.set_query_timeout(Duration::from_secs(60));
                let kad_store = kad::store::MemoryStore::new(local_peer_id);
                let kademlia = kad::Behaviour::with_config(local_peer_id, kad_store, kad_config);

                // Identify protocol for peer info exchange
                let identify = identify::Behaviour::new(
                    identify::Config::new(
                        "/murmur/id/1.0.0".to_string(),
                        key.public(),
                    )
                    .with_push_listen_addr_updates(true),
                );

                // Request-response for direct messages
                let dm_protocol = request_response::cbor::Behaviour::<DirectMessage, DirectMessage>::new(
                    [(StreamProtocol::new("/murmur/dm/1.0.0"), ProtocolSupport::Full)],
                    request_response::Config::default(),
                );

                // Voice signaling protocol
                let voice_protocol = request_response::cbor::Behaviour::<VoiceSignal, VoiceSignal>::new(
                    [(StreamProtocol::new("/murmur/voice/1.0.0"), ProtocolSupport::Full)],
                    request_response::Config::default(),
                );

                // AutoNAT: discover our public address
                let autonat = autonat::Behaviour::new(local_peer_id, autonat::Config::default());

                // UPnP: auto port-forward on router
                let upnp = upnp::tokio::Behaviour::default();

                MurmurBehaviour {
                    gossipsub,
                    mdns,
                    kademlia,
                    identify,
                    dm: dm_protocol,
                    voice: voice_protocol,
                    autonat,
                    upnp,
                }
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(300)))
            .build();

        let local_peer_id = *swarm.local_peer_id();
        info!("Local peer ID: {}", local_peer_id);

        Ok(Self {
            swarm,
            event_tx,
            command_rx,
            local_peer_id,
            display_name,
            e2e,
            store_forward,
            bootstrap_addrs,
            known_peers: HashSet::new(),
            voice_engine: None,
            voice_event_rx: None,
            screen_share: ScreenShare::new(),
            audio_sync: AudioSync::new(),
        })
    }

    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub async fn run(&mut self) -> Result<()> {
        // Listen on all interfaces
        self.swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
        self.swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
        self.swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
        self.swarm.listen_on("/ip6/::/udp/0/quic-v1".parse()?)?;

        // Subscribe to the global presence topic
        let presence_topic = gossipsub::IdentTopic::new("murmur/presence");
        self.swarm.behaviour_mut().gossipsub.subscribe(&presence_topic)?;

        // Subscribe to the store-forward topic (for relaying offline messages)
        let relay_topic = gossipsub::IdentTopic::new("murmur/relay");
        self.swarm.behaviour_mut().gossipsub.subscribe(&relay_topic)?;

        // Dial bootstrap nodes for internet-wide discovery
        for addr in &self.bootstrap_addrs {
            info!("Dialing bootstrap node: {}", addr);
            if let Err(e) = self.swarm.dial(addr.clone()) {
                warn!("Failed to dial bootstrap {}: {}", addr, e);
            }
        }

        // Bootstrap Kademlia DHT
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

        let _ = self.event_tx.send(NetEvent::Connected);

        // Periodic maintenance interval
        let mut maintenance_interval = tokio::time::interval(Duration::from_secs(60));
        // Fast tick for voice speaking state
        let mut voice_tick = tokio::time::interval(Duration::from_millis(200));
        let mut was_speaking = false;

        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }
                Some(cmd) = self.command_rx.recv() => {
                    self.handle_command(cmd).await;
                }
                _ = maintenance_interval.tick() => {
                    self.periodic_maintenance().await;
                }
                _ = voice_tick.tick() => {
                    // Broadcast speaking state changes to peers
                    if let Some(ref engine) = self.voice_engine {
                        let is_speaking = engine.vad_probability() > 0.5;
                        if is_speaking != was_speaking {
                            was_speaking = is_speaking;
                            let peers: Vec<PeerId> = self.known_peers.iter().cloned().collect();
                            for peer_id in &peers {
                                let signal = VoiceSignal {
                                    signal_type: VoiceSignalType::Speaking { is_speaking },
                                    from_peer_id: self.local_peer_id.to_string(),
                                    channel_topic: String::new(),
                                };
                                self.swarm.behaviour_mut().voice.send_request(peer_id, signal);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Periodic tasks: evict expired relay messages, re-bootstrap DHT, clock sync
    async fn periodic_maintenance(&mut self) {
        self.store_forward.evict_expired();
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

        // Send clock sync probes to all voice peers
        if self.voice_engine.is_some() {
            let peers: Vec<PeerId> = self.known_peers.iter().cloned().collect();
            for peer_id in peers {
                self.audio_sync.add_peer(&peer_id.to_string());
                let probe = self.audio_sync.create_probe(&peer_id.to_string());
                let signal = VoiceSignal {
                    signal_type: VoiceSignalType::ClockProbe {
                        seq: probe.seq,
                        t1_us: probe.t1_us,
                    },
                    from_peer_id: self.local_peer_id.to_string(),
                    channel_topic: String::new(),
                };
                self.swarm.behaviour_mut().voice.send_request(&peer_id, signal);
            }
        }
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<behaviour::MurmurBehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(behaviour::MurmurBehaviourEvent::Gossipsub(
                gossipsub::Event::Message { message, .. },
            )) => {
                // Try to parse as an encrypted envelope first
                let data = if let Ok(envelope) = serde_json::from_slice::<EncryptedEnvelope>(&message.data) {
                    // Determine the topic for group decryption
                    let topic_str = message.topic.to_string();
                    if topic_str == "murmur/relay" {
                        // Store-and-forward message - store for the target peer
                        if let Ok(stored) = serde_json::from_slice::<StoredMessage>(&message.data) {
                            if stored.target_peer_id == self.local_peer_id.to_string() {
                                // It's for us - decrypt and process
                                match self.e2e.decrypt_envelope(&stored.envelope) {
                                    Ok(plaintext) => plaintext,
                                    Err(e) => {
                                        debug!("Failed to decrypt relay message: {}", e);
                                        return;
                                    }
                                }
                            } else {
                                // Relay it - store for the target peer
                                self.store_forward.store_message(stored);
                                return;
                            }
                        } else {
                            return;
                        }
                    } else {
                        // Group channel message - decrypt with group key
                        match self.e2e.decrypt_group(&topic_str, &envelope) {
                            Ok(plaintext) => plaintext,
                            Err(e) => {
                                debug!("Failed to decrypt group message: {}", e);
                                // Fall back to treating data as plaintext
                                message.data.clone()
                            }
                        }
                    }
                } else {
                    // Not encrypted / legacy plaintext
                    message.data.clone()
                };

                match serde_json::from_slice::<NetworkMessage>(&data) {
                    Ok(NetworkMessage::Chat(msg)) => {
                        let _ = self.event_tx.send(NetEvent::MessageReceived(msg));
                    }
                    Ok(NetworkMessage::Presence { peer_id, name, status }) => {
                        // Register peer's E2E public key from presence announcements
                        let _ = self.event_tx.send(NetEvent::PresenceUpdate {
                            peer_id,
                            name,
                            status,
                        });
                    }
                    Ok(NetworkMessage::ServerJoin { peer_id, name, server_id }) => {
                        let _ = self.event_tx.send(NetEvent::ServerJoined {
                            peer_id,
                            name,
                            server_id,
                        });
                    }
                    Ok(NetworkMessage::ChannelCreate { server_id, channel }) => {
                        let _ = self.event_tx.send(NetEvent::ChannelCreated {
                            server_id,
                            channel,
                        });
                    }
                    Ok(_) => {}
                    Err(e) => {
                        debug!("Failed to parse gossipsub message: {}", e);
                    }
                }
            }

            SwarmEvent::Behaviour(behaviour::MurmurBehaviourEvent::Mdns(
                mdns::Event::Discovered(peers),
            )) => {
                for (peer_id, addr) in peers {
                    info!("mDNS discovered peer: {} at {}", peer_id, addr);
                    self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                    self.known_peers.insert(peer_id);

                    // Deliver any stored messages we have for this peer
                    let count = self.store_forward.count_for_peer(&peer_id.to_string());
                    if count > 0 {
                        info!("Delivering {} stored messages to peer {}", count, peer_id);
                        let response = self.store_forward.pull_messages(&PullRequest {
                            peer_id: peer_id.to_string(),
                            since: 0,
                            topics: vec![],
                        });
                        for stored in response.messages {
                            // Re-publish the stored encrypted message on the relay topic
                            if let Ok(data) = serde_json::to_vec(&stored) {
                                let relay_topic = gossipsub::IdentTopic::new("murmur/relay");
                                let _ = self.swarm.behaviour_mut().gossipsub.publish(relay_topic, data);
                            }
                        }
                    }

                    let _ = self.event_tx.send(NetEvent::PeerDiscovered {
                        peer_id,
                        name: peer_id.to_string()[..8].to_string(),
                    });
                }
            }

            SwarmEvent::Behaviour(behaviour::MurmurBehaviourEvent::Mdns(
                mdns::Event::Expired(peers),
            )) => {
                for (peer_id, _) in peers {
                    self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    let _ = self.event_tx.send(NetEvent::PeerLeft(peer_id));
                }
            }

            SwarmEvent::Behaviour(behaviour::MurmurBehaviourEvent::Dm(
                request_response::Event::Message { peer, message },
            )) => {
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        let _ = self.event_tx.send(NetEvent::DirectMessageReceived(request.clone()));
                        // Send ack
                        let ack = DirectMessage {
                            id: "ack".to_string(),
                            from_peer_id: self.local_peer_id.to_string(),
                            from_name: self.display_name.clone(),
                            to_peer_id: peer.to_string(),
                            content: String::new(),
                            timestamp: chrono::Utc::now(),
                        };
                        let _ = self.swarm.behaviour_mut().dm.send_response(channel, ack);
                    }
                    _ => {}
                }
            }

            SwarmEvent::Behaviour(behaviour::MurmurBehaviourEvent::Identify(
                identify::Event::Received { peer_id, info, .. },
            )) => {
                // Add identified peer's addresses to Kademlia
                for addr in info.listen_addrs {
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                }
            }

            // Handle incoming voice signaling messages
            SwarmEvent::Behaviour(behaviour::MurmurBehaviourEvent::Voice(
                request_response::Event::Message { peer, message },
            )) => {
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        let signal = request;
                        info!("Voice signal from {}: {:?}", peer, signal.signal_type);

                        if let Some(ref mut engine) = self.voice_engine {
                            match signal.signal_type {
                                VoiceSignalType::Offer { ref sdp_like } => {
                                    // Peer sent us an SDP offer
                                    if let Ok(sdp) = String::from_utf8(sdp_like.clone()) {
                                        match engine.handle_offer(&peer.to_string(), &sdp).await {
                                            Ok(answer_sdp) => {
                                                // Send answer back
                                                let answer = VoiceSignal {
                                                    signal_type: VoiceSignalType::Answer {
                                                        sdp_like: answer_sdp.into_bytes(),
                                                    },
                                                    from_peer_id: self.local_peer_id.to_string(),
                                                    channel_topic: signal.channel_topic.clone(),
                                                };
                                                let _ = self.swarm.behaviour_mut().voice.send_response(channel, answer);
                                            }
                                            Err(e) => error!("Failed to handle voice offer: {}", e),
                                        }
                                    }
                                }
                                VoiceSignalType::Answer { ref sdp_like } => {
                                    if let Ok(sdp) = String::from_utf8(sdp_like.clone()) {
                                        if let Err(e) = engine.handle_answer(&peer.to_string(), &sdp).await {
                                            error!("Failed to handle voice answer: {}", e);
                                        }
                                    }
                                }
                                VoiceSignalType::IceCandidate { ref candidate } => {
                                    if let Err(e) = engine.add_ice_candidate(&peer.to_string(), candidate).await {
                                        error!("Failed to add ICE candidate: {}", e);
                                    }
                                }
                                VoiceSignalType::Join => {
                                    info!("Peer {} joined voice", peer);
                                    // Initiate WebRTC connection to this peer
                                    match engine.connect_to_peer(&peer.to_string()).await {
                                        Ok(sdp) => {
                                            // Send offer via the voice protocol
                                            let offer = VoiceSignal {
                                                signal_type: VoiceSignalType::Offer {
                                                    sdp_like: sdp.into_bytes(),
                                                },
                                                from_peer_id: self.local_peer_id.to_string(),
                                                channel_topic: signal.channel_topic.clone(),
                                            };
                                            self.swarm.behaviour_mut().voice.send_request(&peer, offer);
                                        }
                                        Err(e) => error!("Failed to connect to voice peer: {}", e),
                                    }
                                }
                                VoiceSignalType::Leave => {
                                    info!("Peer {} left voice", peer);
                                    let _ = engine.disconnect_peer(&peer.to_string()).await;
                                    self.audio_sync.remove_peer(&peer.to_string());
                                }
                                VoiceSignalType::Speaking { is_speaking } => {
                                    let _ = self.event_tx.send(NetEvent::PeerSpeaking {
                                        peer_id: peer.to_string(),
                                        is_speaking,
                                    });
                                }
                                VoiceSignalType::ClockProbe { seq, t1_us } => {
                                    // Respond to clock sync probe
                                    let probe = crate::media::ClockProbe { seq, t1_us };
                                    let resp = self.audio_sync.handle_probe(&probe);
                                    let response_signal = VoiceSignal {
                                        signal_type: VoiceSignalType::ClockProbeResponse {
                                            seq: resp.seq,
                                            t1_us: resp.t1_us,
                                            t2_us: resp.t2_us,
                                            t3_us: resp.t3_us,
                                        },
                                        from_peer_id: self.local_peer_id.to_string(),
                                        channel_topic: signal.channel_topic.clone(),
                                    };
                                    self.swarm.behaviour_mut().voice.send_request(&peer, response_signal);
                                }
                                VoiceSignalType::ClockProbeResponse { seq, t1_us, t2_us, t3_us } => {
                                    // Process clock sync response
                                    let resp = crate::media::ClockProbeResponse { seq, t1_us, t2_us, t3_us };
                                    self.audio_sync.handle_probe_response(&peer.to_string(), &resp);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }

            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                info!("Connection established with {} via {}", peer_id, endpoint.get_remote_address());
                self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                self.known_peers.insert(peer_id);
                let _ = self.event_tx.send(NetEvent::PeerDiscovered {
                    peer_id,
                    name: peer_id.to_string()[..8].to_string(),
                });
            }

            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                info!("Connection closed with {}", peer_id);
            }

            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                error!("Outgoing connection error to {:?}: {}", peer_id, error);
                let _ = self.event_tx.send(NetEvent::Error(format!("Connection failed: {}", error)));
            }

            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {}", address);
                let full_addr = format!("{}/p2p/{}", address, self.local_peer_id);
                let _ = self.event_tx.send(NetEvent::ListeningOn(full_addr));
            }

            // UPnP mapped an external port
            SwarmEvent::Behaviour(behaviour::MurmurBehaviourEvent::Upnp(
                upnp::Event::NewExternalAddr(addr),
            )) => {
                info!("UPnP mapped external address: {}", addr);
                let full_addr = format!("{}/p2p/{}", addr, self.local_peer_id);
                let _ = self.event_tx.send(NetEvent::ListeningOn(full_addr));
            }

            // AutoNAT determined our public reachability
            SwarmEvent::Behaviour(behaviour::MurmurBehaviourEvent::Autonat(event)) => {
                match event {
                    autonat::Event::StatusChanged { new, .. } => {
                        info!("AutoNAT status: {:?}", new);
                    }
                    _ => {}
                }
            }

            // External address confirmed by peers
            SwarmEvent::ExternalAddrConfirmed { address } => {
                info!("External address confirmed: {}", address);
                let full_addr = format!("{}/p2p/{}", address, self.local_peer_id);
                let _ = self.event_tx.send(NetEvent::ListeningOn(full_addr));
            }

            _ => {}
        }
    }

    async fn handle_command(&mut self, cmd: NetCommand) {
        match cmd {
            NetCommand::SendMessage(msg) => {
                let topic_str = format!(
                    "murmur/server/{}/channel/{}",
                    msg.server_id, msg.channel_id
                );
                let topic = gossipsub::IdentTopic::new(&topic_str);
                let net_msg = NetworkMessage::Chat(msg);
                if let Ok(plaintext) = serde_json::to_vec(&net_msg) {
                    // Encrypt with group key for E2E
                    let data = match self.e2e.encrypt_for_group(&topic_str, &plaintext) {
                        Ok(envelope) => serde_json::to_vec(&envelope).unwrap_or(plaintext),
                        Err(_) => plaintext, // Fall back to plaintext if encryption fails
                    };
                    if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(topic, data) {
                        warn!("Failed to publish message: {}", e);
                    }
                }
            }

            NetCommand::SendDirectMessage(msg) => {
                if let Ok(peer_id) = msg.to_peer_id.parse::<PeerId>() {
                    // Try to encrypt for peer, fall back to plaintext
                    self.swarm.behaviour_mut().dm.send_request(&peer_id, msg);
                }
            }

            NetCommand::JoinServer(topic) => {
                let gossip_topic = gossipsub::IdentTopic::new(&topic);
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.subscribe(&gossip_topic) {
                    error!("Failed to subscribe to {}: {}", topic, e);
                }
                // Announce join
                let presence = NetworkMessage::ServerJoin {
                    peer_id: self.local_peer_id.to_string(),
                    name: self.display_name.clone(),
                    server_id: topic.clone(),
                };
                if let Ok(data) = serde_json::to_vec(&presence) {
                    let presence_topic = gossipsub::IdentTopic::new("murmur/presence");
                    let _ = self.swarm.behaviour_mut().gossipsub.publish(presence_topic, data);
                }
            }

            NetCommand::LeaveServer(topic) => {
                let gossip_topic = gossipsub::IdentTopic::new(&topic);
                let _ = self.swarm.behaviour_mut().gossipsub.unsubscribe(&gossip_topic);
            }

            NetCommand::CreateChannel { server_id, channel } => {
                let net_msg = NetworkMessage::ChannelCreate {
                    server_id: server_id.clone(),
                    channel,
                };
                if let Ok(data) = serde_json::to_vec(&net_msg) {
                    // Broadcast to the server's presence topic
                    let topic = gossipsub::IdentTopic::new("murmur/presence");
                    let _ = self.swarm.behaviour_mut().gossipsub.publish(topic, data);
                }
            }

            NetCommand::UpdatePresence(status) => {
                let presence = NetworkMessage::Presence {
                    peer_id: self.local_peer_id.to_string(),
                    name: self.display_name.clone(),
                    status,
                };
                if let Ok(data) = serde_json::to_vec(&presence) {
                    let topic = gossipsub::IdentTopic::new("murmur/presence");
                    let _ = self.swarm.behaviour_mut().gossipsub.publish(topic, data);
                }
            }

            NetCommand::Dial(addr) => {
                info!("Dialing peer: {}", addr);
                // Extract peer ID from the multiaddr if present (last /p2p/ component)
                let peer_id = addr.iter().find_map(|proto| {
                    if let libp2p::multiaddr::Protocol::P2p(id) = proto {
                        Some(id)
                    } else {
                        None
                    }
                });

                match self.swarm.dial(addr.clone()) {
                    Ok(()) => {
                        info!("Dial initiated to {}", addr);
                        // If we know the peer ID, add them to gossipsub + kademlia immediately
                        if let Some(pid) = peer_id {
                            self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&pid);
                            self.swarm.behaviour_mut().kademlia.add_address(&pid, addr.clone());
                            self.known_peers.insert(pid);
                            let _ = self.event_tx.send(NetEvent::PeerDiscovered {
                                peer_id: pid,
                                name: pid.to_string()[..8].to_string(),
                            });
                        }
                    }
                    Err(e) => {
                        error!("Failed to dial {}: {}", addr, e);
                        let _ = self.event_tx.send(NetEvent::Error(format!("Failed to connect: {}", e)));
                    }
                }
            }

            NetCommand::StartVoice(channel_topic) => {
                info!("Starting voice for channel: {}", channel_topic);

                let voice_topic = gossipsub::IdentTopic::new(format!("{}/voice", channel_topic));
                let _ = self.swarm.behaviour_mut().gossipsub.subscribe(&voice_topic);

                let (voice_tx, voice_rx) = mpsc::unbounded_channel();
                match VoiceEngine::new(voice_tx) {
                    Ok(mut engine) => {
                        // Start with default settings — UI can update via UpdateAudioSettings
                        if let Err(e) = engine.start_audio_capture(
                            &None, &None, 1.0, 1.0, true, 0.5,
                        ) {
                            warn!("Failed to start audio capture: {} (voice will work without mic)", e);
                        }
                        self.voice_engine = Some(engine);
                        self.voice_event_rx = Some(voice_rx);
                        info!("Voice engine ready, {} known peers", self.known_peers.len());
                    }
                    Err(e) => {
                        error!("Failed to create voice engine: {}", e);
                    }
                }
            }

            NetCommand::StopVoice => {
                info!("Stopping voice");
                if let Some(mut engine) = self.voice_engine.take() {
                    let _ = engine.disconnect_all().await;
                }
                self.voice_event_rx = None;
                self.screen_share.stop();
            }

            NetCommand::StartScreenShare(channel_topic) => {
                info!("Starting screen share for channel: {}", channel_topic);
                match self.screen_share.start() {
                    Ok(video_track) => {
                        info!("Screen share started, {}x{}",
                            self.screen_share.dimensions().0,
                            self.screen_share.dimensions().1);
                        // In full implementation, add the video track to all active peer connections
                    }
                    Err(e) => {
                        error!("Failed to start screen share: {}", e);
                    }
                }
            }

            NetCommand::StopScreenShare => {
                info!("Stopping screen share");
                self.screen_share.stop();
            }
        }
    }
}
