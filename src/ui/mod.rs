mod components;
mod theme;

use crate::network::NetCommand;
use crate::state::AppState;
use crate::types::*;
use components::{initials, markdown_to_html, status_class};
use dioxus::prelude::*;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Shared app state wrapped for Dioxus
#[derive(Clone)]
pub struct AppContext {
    pub state: Arc<Mutex<AppState>>,
    pub cmd_tx: mpsc::UnboundedSender<NetCommand>,
    pub event_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<NetEvent>>>>,
}

pub fn launch(
    state: AppState,
    cmd_tx: mpsc::UnboundedSender<NetCommand>,
    event_rx: mpsc::UnboundedReceiver<NetEvent>,
) {
    let ctx = AppContext {
        state: Arc::new(Mutex::new(state)),
        cmd_tx,
        event_rx: Arc::new(Mutex::new(Some(event_rx))),
    };

    // Store in a static for Dioxus to access
    // (Dioxus 0.7 uses context providers)
    let cfg = dioxus::desktop::Config::new()
        .with_window(
            dioxus::desktop::WindowBuilder::new()
                .with_title("murmur")
                .with_inner_size(dioxus::desktop::LogicalSize::new(1200.0, 800.0)),
        )
        .with_custom_head(format!(r#"<style>{}</style>"#, theme::CSS));

    dioxus::LaunchBuilder::desktop()
        .with_cfg(cfg)
        .with_context(ctx)
        .launch(App);
}

#[component]
fn App() -> Element {
    let ctx = use_context::<AppContext>();

    // Poll network events periodically
    let state_ref = ctx.state.clone();
    let event_rx_ref = ctx.event_rx.clone();
    use_future(move || {
        let state = state_ref.clone();
        let rx_ref = event_rx_ref.clone();
        async move {
            // Take the receiver out
            let mut rx = {
                let mut guard = rx_ref.lock().unwrap();
                guard.take()
            };
            if let Some(ref mut rx) = rx {
                loop {
                    match rx.recv().await {
                        Some(event) => {
                            if let Ok(mut s) = state.lock() {
                                handle_net_event(&mut s, event);
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });

    // Get current state snapshot for rendering
    let state = ctx.state.lock().unwrap();
    let servers = state.servers.clone();
    let active_server = state.active_server;
    let active_channel = state.active_channel;
    let profile = state.profile.clone();
    let messages = state.current_messages().into_iter().cloned().collect::<Vec<_>>();
    let peers: Vec<(String, UserProfile)> = state.peers.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let connected = true; // TODO: track connection state
    let server_name = active_server
        .and_then(|i| servers.get(i))
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let channel_name = active_server
        .and_then(|si| servers.get(si))
        .and_then(|s| active_channel.and_then(|ci| s.channels.get(ci)))
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "general".to_string());
    let peer_count = peers.len() + 1;
    drop(state);

    rsx! {
        div { class: "app",
            // Topbar
            div { class: "topbar",
                button { class: "tb-server active", "dc" }
                div { class: "tb-sep" }
                for (i, server) in servers.iter().enumerate() {
                    button {
                        class: if active_server == Some(i) { "tb-server active" } else { "tb-server" },
                        onclick: {
                            let ctx = use_context::<AppContext>();
                            move |_| {
                                if let Ok(mut s) = ctx.state.lock() {
                                    s.active_server = Some(i);
                                    s.active_channel = Some(0);
                                    s.active_dm_peer = None;
                                }
                            }
                        },
                        "{initials(&server.name)}"
                    }
                }
                div { class: "tb-sep" }
                button { class: "tb-server tb-add", "+" }
                div { class: "tb-spacer" }
                span { class: "tb-dot" }
                div { class: "tb-identity",
                    span { "{initials(&profile.display_name)}" }
                    span { "{profile.display_name}" }
                }
            }

            // Main content row
            div { style: "display:flex; flex:1; overflow:hidden;",
                // Sidebar
                div { class: "sidebar",
                    div { class: "sidebar-header", "{server_name}" }
                    div { class: "channels",
                        div { class: "ch-section", "channels" }
                        if let Some(si) = active_server {
                            if let Some(server) = servers.get(si) {
                                for (i, channel) in server.channels.iter().enumerate() {
                                    button {
                                        class: if active_channel == Some(i) { "ch-item active" } else { "ch-item" },
                                        onclick: {
                                            let ctx = use_context::<AppContext>();
                                            move |_| {
                                                if let Ok(mut s) = ctx.state.lock() {
                                                    s.active_channel = Some(i);
                                                    s.active_dm_peer = None;
                                                    if let Some(topic) = s.current_topic() {
                                                        s.load_messages_for_topic(&topic);
                                                        s.mark_read(&topic);
                                                    }
                                                }
                                            }
                                        },
                                        span { class: "ch-prefix", "#" }
                                        "{channel.name}"
                                    }
                                }
                            }
                        }

                        div { class: "ch-section", style: "margin-top: 8px;", "direct" }
                        for (pid, peer) in peers.iter() {
                            button {
                                class: "ch-item",
                                span { class: "ch-prefix", "@" }
                                "{peer.display_name}"
                            }
                        }
                    }

                    // User panel
                    div { class: "user-panel",
                        span { class: "status-dot {status_class(profile.status)}" }
                        div {
                            div { class: "up-name", "{profile.display_name}" }
                            div { class: "up-status", "{profile.status}" }
                        }
                    }
                }

                // Main chat area
                div { class: "main",
                    div { class: "main-header",
                        span { style: "color:#333; font-size:13px;", "#" }
                        span { class: "mh-channel", " {channel_name}" }
                        div { class: "mh-sep" }
                        span { class: "mh-meta", "{peer_count} peers" }
                        div { class: "mh-spacer" }
                        div { class: "conn-badge",
                            span { class: "tb-dot" }
                            " p2p"
                        }
                    }

                    // Messages
                    div { class: "messages",
                        div { class: "welcome",
                            div { class: "w-icon", "#_" }
                            h2 { "{channel_name}" }
                            p { "Beginning of ",
                                code { "#{channel_name}" }
                                ". All messages are end-to-end encrypted."
                            }
                        }

                        div { class: "date-sep",
                            div { class: "line" }
                            span { class: "label", "TODAY" }
                            div { class: "line" }
                        }

                        for msg in messages.iter() {
                            {
                                let ini = initials(&msg.sender_name);
                                let name = msg.sender_name.clone();
                                let time = msg.timestamp.format("%H:%M").to_string();
                                let html = markdown_to_html(&msg.content);
                                let edited = msg.edited;
                                let reactions: Vec<(String, usize)> = msg.reactions.iter()
                                    .map(|(e, p)| (e.clone(), p.len()))
                                    .collect();

                                rsx! {
                                    div { class: "msg",
                                        div { class: "msg-avatar", "{ini}" }
                                        div {
                                            div { style: "display:flex; align-items:center;",
                                                span { class: "msg-name", "{name}" }
                                                span { class: "msg-time", "{time}" }
                                                span { class: "msg-e2e", "e2e" }
                                                if edited {
                                                    span { class: "msg-edited", "(edited)" }
                                                }
                                            }
                                            div {
                                                class: "msg-text",
                                                dangerous_inner_html: "{html}"
                                            }
                                            if !reactions.is_empty() {
                                                div { class: "reactions",
                                                    for (emoji, count) in reactions.iter() {
                                                        button { class: "reaction",
                                                            "{emoji} {count}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Input
                    div { class: "input-area",
                        div { class: "input-box",
                            input {
                                placeholder: "message #{channel_name} — encrypted",
                                onkeypress: {
                                    let ctx = use_context::<AppContext>();
                                    move |evt: KeyboardEvent| {
                                        if evt.key() == Key::Enter {
                                            if let Ok(mut s) = ctx.state.lock() {
                                                let content = s.input_buffer.trim().to_string();
                                                if content.is_empty() { return; }
                                                if let (Some(si), Some(ci)) = (s.active_server, s.active_channel) {
                                                    if let Some(server) = s.servers.get(si) {
                                                        if let Some(channel) = server.channels.get(ci) {
                                                            let msg = ChatMessage::new(
                                                                server.id.clone(),
                                                                channel.id.clone(),
                                                                s.profile.peer_id.clone(),
                                                                s.profile.display_name.clone(),
                                                                content,
                                                            );
                                                            s.add_message(msg.clone());
                                                            let _ = ctx.cmd_tx.send(NetCommand::SendMessage(msg));
                                                        }
                                                    }
                                                }
                                                s.input_buffer.clear();
                                            }
                                        }
                                    }
                                },
                                oninput: {
                                    let ctx = use_context::<AppContext>();
                                    move |evt: FormEvent| {
                                        if let Ok(mut s) = ctx.state.lock() {
                                            s.input_buffer = evt.value().to_string();
                                        }
                                    }
                                },
                            }
                        }
                    }
                }

                // Members panel
                div { class: "members",
                    div { class: "ch-section", "online — {peer_count}" }
                    div { class: "member",
                        div { class: "m-avatar", "{initials(&profile.display_name)}" }
                        div {
                            div { class: "m-name", "{profile.display_name}" }
                            div { class: "m-status", "you" }
                        }
                    }
                    for (pid, peer) in peers.iter() {
                        div { class: if peer.status == UserStatus::Offline { "member offline" } else { "member" },
                            div { class: "m-avatar", "{initials(&peer.display_name)}" }
                            div {
                                div { class: "m-name", "{peer.display_name}" }
                                div { class: "m-status", "{peer.status}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn handle_net_event(state: &mut AppState, event: NetEvent) {
    match event {
        NetEvent::MessageReceived(msg) => { state.add_message(msg); }
        NetEvent::DirectMessageReceived(msg) => { state.add_direct_message(msg); }
        NetEvent::PeerDiscovered { peer_id, name } => {
            state.update_peer(peer_id.to_string(), name, UserStatus::Online);
        }
        NetEvent::PeerLeft(peer_id) => {
            if let Some(profile) = state.peers.get_mut(&peer_id.to_string()) {
                profile.status = UserStatus::Offline;
                state.persist_peers();
            }
        }
        NetEvent::PresenceUpdate { peer_id, name, status } => {
            state.update_peer(peer_id, name, status);
        }
        NetEvent::ServerJoined { peer_id, name, .. } => {
            state.update_peer(peer_id, name, UserStatus::Online);
        }
        NetEvent::ChannelCreated { server_id, channel } => {
            state.add_channel_to_server(&server_id, channel);
        }
        NetEvent::MessageEdited { message_id, new_content, peer_id } => {
            state.edit_message(&message_id, &new_content, &peer_id);
        }
        NetEvent::MessageDeleted { message_id, peer_id } => {
            state.delete_message(&message_id, &peer_id);
        }
        NetEvent::MessageReaction { message_id, emoji, peer_id } => {
            state.toggle_reaction(&message_id, &emoji, &peer_id);
        }
        NetEvent::PeerTyping { .. } => {}
        NetEvent::PeerSpeaking { .. } => {}
        NetEvent::PeerVoiceJoined { .. } => {}
        NetEvent::PeerVoiceLeft { .. } => {}
        NetEvent::Connected => {}
        NetEvent::ListeningOn(addr) => {
            if !state.listen_addrs.contains(&addr) {
                state.listen_addrs.push(addr);
            }
        }
        NetEvent::Error(_) => {}
    }
}
