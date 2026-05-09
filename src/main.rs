#![allow(dead_code, unused_imports)]
#![windows_subsystem = "windows"]

mod crypto;
mod identity;
mod media;
mod network;
mod state;
mod types;
mod ui;

use anyhow::Result;
use iced::Theme;
use network::{NetCommand, NetworkManager};
use state::AppState;
use tokio::sync::mpsc;
use tracing::info;
use types::NetEvent;
use ui::MurmurApp;

/// Default bootstrap nodes for internet-wide peer discovery.
/// These are well-known IPFS/libp2p bootstrap nodes that help with Kademlia DHT.
/// In production, you'd run your own bootstrap nodes.
const DEFAULT_BOOTSTRAP: &[&str] = &[
    // Add your own bootstrap node addresses here, e.g.:
    // "/ip4/1.2.3.4/tcp/4001/p2p/12D3KooW..."
    // "/dns4/bootstrap.murmur.example.com/tcp/4001/p2p/12D3KooW..."
];

fn main() {
    if let Err(e) = run() {
        // Write error to a crash log the user can find
        let crash_path = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("murmur")
            .join("crash.log");
        let msg = format!("murmur crashed: {:#}", e);
        let _ = std::fs::write(&crash_path, &msg);
        eprintln!("{}", msg);
    }
}

fn run() -> Result<()> {
    // Data directory
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("murmur");
    std::fs::create_dir_all(&data_dir)?;

    // Install panic hook that writes to crash log
    let panic_log_dir = data_dir.clone();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("PANIC: {}", info);
        let crash_path = panic_log_dir.join("crash.log");
        let _ = std::fs::write(&crash_path, &msg);
    }));

    // Set up file-based logging
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let file_appender = tracing_appender::rolling::daily(&log_dir, "murmur.log");
    tracing_subscriber::fmt()
        .with_writer(file_appender)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "murmur=info,libp2p=warn".into()),
        )
        .init();

    info!("Starting murmur");

    // Load or create persistent identity
    let identity = identity::Identity::load_or_create(&data_dir)?;
    let peer_id = identity.peer_id();
    let keypair = identity.keypair().clone();
    info!("Peer ID: {}", peer_id);

    // Initialize E2E crypto (uses same data dir for key storage)
    let crypto_db = sled::open(data_dir.join("crypto.db"))?;
    let e2e = crypto::E2ECrypto::new(crypto_db.clone())?;

    // Initialize store-and-forward relay
    let relay_db = sled::open(data_dir.join("relay.db"))?;
    let store_forward = crypto::StoreForward::new(relay_db)?;

    // Parse bootstrap addresses
    let bootstrap_addrs: Vec<libp2p::Multiaddr> = DEFAULT_BOOTSTRAP
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    // Also check for user-configured bootstrap nodes
    let config_bootstrap = data_dir.join("bootstrap.txt");
    let mut extra_addrs: Vec<libp2p::Multiaddr> = Vec::new();
    if config_bootstrap.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_bootstrap) {
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    if let Ok(addr) = line.parse() {
                        extra_addrs.push(addr);
                    }
                }
            }
        }
    }
    let all_bootstrap: Vec<_> = bootstrap_addrs
        .into_iter()
        .chain(extra_addrs)
        .collect();

    info!("Bootstrap nodes: {}", all_bootstrap.len());

    // Channels for network <-> UI communication
    let (net_event_tx, net_event_rx) = mpsc::unbounded_channel::<NetEvent>();
    let (net_cmd_tx, net_cmd_rx) = mpsc::unbounded_channel::<NetCommand>();

    // Generate display name from hostname or random
    let display_name = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| format!("anon-{}", &uuid::Uuid::new_v4().to_string()[..6]));

    // Start network on a background tokio runtime
    let net_event_tx_clone = net_event_tx.clone();
    let display_name_clone = display_name.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async move {
            let mut net_manager = NetworkManager::new(
                keypair,
                display_name_clone,
                net_event_tx_clone,
                net_cmd_rx,
                e2e,
                store_forward,
                all_bootstrap,
            )
            .expect("Failed to create network manager");

            info!("Network started with peer ID: {}", net_manager.local_peer_id());

            if let Err(e) = net_manager.run().await {
                tracing::error!("Network error: {}", e);
            }
        });
    });

    let mut state = AppState::new(peer_id.to_string(), display_name)?;

    // Test UI mode: populate fake data for visual testing
    let test_scene = std::env::var("MURMUR_TEST_UI").ok();
    if test_scene.is_some() {
        populate_test_data(&mut state);
    }

    // Run the iced GUI
    iced::application(
        MurmurApp::title,
        MurmurApp::update,
        MurmurApp::view,
    )
    .subscription(MurmurApp::subscription)
    .theme(|_app| Theme::Dark)
    .window_size((1200.0, 800.0))
    .run_with(move || MurmurApp::new(state, net_cmd_tx, net_event_rx, test_scene))?;

    Ok(())
}

fn populate_test_data(state: &mut AppState) {
    use crate::state::VoicePeerState;
    use crate::types::*;

    // Add a second server
    state.add_server(Server::new("rust-dev".into()));
    state.add_server(Server::new("music-jam".into()));

    // Add channels to first server
    state.add_channel_to_server(&state.servers[0].id.clone(), Channel {
        id: "off-topic".into(),
        name: "off-topic".into(),
    });
    state.add_channel_to_server(&state.servers[0].id.clone(), Channel {
        id: "voice-chat".into(),
        name: "voice-chat".into(),
    });

    // Add fake peers
    let peers = vec![
        ("peer-alice-00001", "Alice", UserStatus::Online),
        ("peer-bob-000002", "Bob", UserStatus::Online),
        ("peer-carol-0003", "Carol", UserStatus::Away),
        ("peer-dave-00004", "Dave", UserStatus::DoNotDisturb),
        ("peer-eve-000005", "Eve", UserStatus::Offline),
    ];
    for (id, name, status) in &peers {
        state.peers.insert(id.to_string(), UserProfile {
            peer_id: id.to_string(),
            display_name: name.to_string(),
            status: *status,
        });
    }

    // Add fake messages
    let server_id = state.servers[0].id.clone();
    let msgs = vec![
        ("peer-alice-00001", "Alice", "Hey everyone! Just set up murmur on my machine."),
        ("peer-bob-000002", "Bob", "Welcome! The P2P connection is super fast on LAN."),
        ("peer-alice-00001", "Alice", "Yeah, no lag at all. How does the voice chat work?"),
        ("peer-bob-000002", "Bob", "Click the voice channel on the left, it uses WebRTC with RNNoise for noise cancellation."),
        ("peer-carol-0003", "Carol", "I've been using it for a week now, the E2E encryption is solid."),
        ("peer-alice-00001", "Alice", "That's awesome. Can we do screen sharing too?"),
        ("peer-bob-000002", "Bob", "Yep, there's a Share button when you're in voice."),
    ];
    for (pid, name, content) in msgs {
        state.add_message(ChatMessage::new(
            server_id.clone(),
            "general".into(),
            pid.into(),
            name.into(),
            content.into(),
        ));
    }

    // Set up voice state if scene calls for it
    if std::env::var("MURMUR_TEST_UI").ok().as_deref() == Some("voice") {
        state.voice_active = true;
        state.voice_channel_name = Some("General".into());
        state.voice_peers.insert("peer-alice-00001".into(), VoicePeerState {
            peer_id: "peer-alice-00001".into(),
            display_name: "Alice".into(),
            speaking: true,
            muted: false,
            deafened: false,
        });
        state.voice_peers.insert("peer-bob-000002".into(), VoicePeerState {
            peer_id: "peer-bob-000002".into(),
            display_name: "Bob".into(),
            speaking: false,
            muted: true,
            deafened: false,
        });
    }

    // Set up audio devices
    state.audio.available_inputs = vec!["Built-in Microphone".into(), "USB Headset Mic".into(), "Blue Yeti".into()];
    state.audio.available_outputs = vec!["Built-in Speakers".into(), "USB Headset".into(), "HDMI Audio".into()];
}
