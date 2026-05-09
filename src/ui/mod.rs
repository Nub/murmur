mod components;
mod theme;

use crate::network::NetCommand;
use crate::state::AppState;
use crate::types::*;
use egui::{self, Color32, RichText, Vec2};
use theme::*;
use tokio::sync::mpsc;

pub struct MurmurApp {
    state: AppState,
    cmd_tx: mpsc::UnboundedSender<NetCommand>,
    event_rx: Option<mpsc::UnboundedReceiver<NetEvent>>,
    modal: Modal,
    modal_input: String,
    connected: bool,
    anim_tick: u64,
    typing_names: Vec<String>,
    mic_tester: Option<crate::media::MicTester>,
    reply_to_id: Option<String>,
    search_query: String,
    search_results: Vec<ChatMessage>,
    search_open: bool,
}

#[derive(PartialEq)]
enum Modal {
    None,
    CreateServer,
    JoinServer,
    CreateChannel,
    ChangeDisplayName,
    Settings,
    ConnectPeer,
}

impl MurmurApp {
    pub fn new(
        state: AppState,
        cmd_tx: mpsc::UnboundedSender<NetCommand>,
        event_rx: mpsc::UnboundedReceiver<NetEvent>,
    ) -> Self {
        Self {
            state,
            cmd_tx,
            event_rx: Some(event_rx),
            modal: Modal::None,
            modal_input: String::new(),
            connected: false,
            anim_tick: 0,
            typing_names: Vec::new(),
            mic_tester: None,
            reply_to_id: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_open: false,
        }
    }

    fn poll_events(&mut self) {
        let events: Vec<NetEvent> = if let Some(ref mut rx) = self.event_rx {
            let mut evts = Vec::new();
            while let Ok(event) = rx.try_recv() {
                evts.push(event);
            }
            evts
        } else {
            Vec::new()
        };
        for event in events {
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: NetEvent) {
        match event {
            NetEvent::MessageReceived(msg) => self.state.add_message(msg),
            NetEvent::DirectMessageReceived(msg) => self.state.add_direct_message(msg),
            NetEvent::PeerDiscovered { peer_id, name } => {
                self.state.update_peer(peer_id.to_string(), name, UserStatus::Online);
            }
            NetEvent::PeerLeft(peer_id) => {
                if let Some(p) = self.state.peers.get_mut(&peer_id.to_string()) {
                    p.status = UserStatus::Offline;
                    self.state.persist_peers();
                }
            }
            NetEvent::PresenceUpdate { peer_id, name, status } => {
                self.state.update_peer(peer_id, name, status);
            }
            NetEvent::ServerJoined { peer_id, name, .. } => {
                self.state.update_peer(peer_id, name, UserStatus::Online);
            }
            NetEvent::ChannelCreated { server_id, channel } => {
                self.state.add_channel_to_server(&server_id, channel);
            }
            NetEvent::Connected => self.connected = true,
            NetEvent::ListeningOn(addr) => {
                if !self.state.listen_addrs.contains(&addr) {
                    self.state.listen_addrs.push(addr);
                }
            }
            NetEvent::PeerTyping { name, .. } => {
                if !self.typing_names.contains(&name) {
                    self.typing_names.push(name);
                }
            }
            NetEvent::PeerSpeaking { peer_id, is_speaking } => {
                if let Some(vp) = self.state.voice_peers.get_mut(&peer_id) {
                    vp.speaking = is_speaking;
                }
            }
            NetEvent::PeerVoiceJoined { peer_id, name } => {
                let display_name = self.state.peers.get(&peer_id)
                    .map(|p| p.display_name.clone())
                    .unwrap_or_else(|| name[..8.min(name.len())].to_string());
                self.state.voice_peers.insert(peer_id.clone(), crate::state::VoicePeerState {
                    peer_id, display_name, speaking: false, muted: false, deafened: false,
                });
            }
            NetEvent::PeerVoiceLeft { peer_id } => { self.state.voice_peers.remove(&peer_id); }
            NetEvent::MessageEdited { message_id, new_content, peer_id } => {
                self.state.edit_message(&message_id, &new_content, &peer_id);
            }
            NetEvent::MessageDeleted { message_id, peer_id } => {
                self.state.delete_message(&message_id, &peer_id);
            }
            NetEvent::MessageReaction { message_id, emoji, peer_id } => {
                self.state.toggle_reaction(&message_id, &emoji, &peer_id);
            }
            NetEvent::Error(_) => {}
        }
    }

    fn send_message(&mut self) {
        let content = self.state.input_buffer.trim().to_string();
        if content.is_empty() { return; }

        if content.starts_with('/') {
            self.handle_slash(&content);
            self.state.input_buffer.clear();
            return;
        }

        if let (Some(si), Some(ci)) = (self.state.active_server, self.state.active_channel) {
            if let Some(server) = self.state.servers.get(si) {
                if let Some(channel) = server.channels.get(ci) {
                    let mut msg = ChatMessage::new(
                        server.id.clone(), channel.id.clone(),
                        self.state.profile.peer_id.clone(),
                        self.state.profile.display_name.clone(),
                        content,
                    );
                    msg.reply_to = self.reply_to_id.take();
                    self.state.add_message(msg.clone());
                    let _ = self.cmd_tx.send(NetCommand::SendMessage(msg));
                }
            }
        }
        self.state.input_buffer.clear();
    }

    fn handle_slash(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        match parts[0] {
            "/name" if parts.len() > 1 => {
                self.state.set_display_name(parts[1].to_string());
                let _ = self.cmd_tx.send(NetCommand::UpdatePresence(self.state.profile.status));
            }
            "/connect" if parts.len() > 1 => {
                if let Ok(addr) = parts[1].parse() {
                    let _ = self.cmd_tx.send(NetCommand::Dial(addr));
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for MurmurApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme(ctx);
        self.poll_events();
        self.anim_tick = self.anim_tick.wrapping_add(1);

        // Clear typing every ~120 frames (~2 sec at 60fps)
        if self.anim_tick % 120 == 0 {
            self.typing_names.clear();
        }

        // Update mic test level
        if let Some(ref tester) = self.mic_tester {
            self.state.audio.mic_level = tester.level();
        }

        // Update window title with unread
        if self.state.total_unread > 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                format!("murmur ({}) - {}", self.state.total_unread, self.state.profile.display_name)
            ));
        }

        // Escape key
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.modal != Modal::None { self.modal = Modal::None; }
            else if self.search_open { self.search_open = false; }
            else { self.reply_to_id = None; }
        }

        // Ctrl+K for search
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) {
            self.search_open = !self.search_open;
        }

        // ── Top bar ──
        egui::TopBottomPanel::top("topbar").frame(
            egui::Frame::new().fill(BG_DEEPEST).inner_margin(egui::Margin::symmetric(12, 6))
        ).show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Home
                if ui.add(egui::Button::new(RichText::new("dc").color(GREEN).size(13.0))
                    .min_size(Vec2::new(36.0, 36.0))
                    .fill(BG_ELEVATED)
                    .rounding(8.0)
                ).clicked() {}

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Servers
                for (i, server) in self.state.servers.clone().iter().enumerate() {
                    let active = self.state.active_server == Some(i);
                    let btn = egui::Button::new(
                        RichText::new(initials(&server.name)).size(13.0)
                            .color(if active { TEXT_BRIGHT } else { TEXT_MUTED })
                    )
                    .min_size(Vec2::new(36.0, 36.0))
                    .fill(BG_ELEVATED)
                    .rounding(if active { 8.0 } else { 10.0 })
                    .stroke(if active { egui::Stroke::new(1.5, TEXT_FAINT) } else { egui::Stroke::NONE });

                    let resp = ui.add(btn);
                    if resp.clicked() {
                        self.state.active_server = Some(i);
                        self.state.active_channel = Some(0);
                        self.state.active_dm_peer = None;
                        if let Some(topic) = self.state.current_topic() {
                            self.state.load_messages_for_topic(&topic);
                            self.state.mark_read(&topic);
                        }
                        let _ = self.cmd_tx.send(NetCommand::JoinServer(
                            self.state.current_topic().unwrap_or_default()
                        ));
                    }
                    resp.on_hover_text(&server.name);
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Add server
                if ui.add(egui::Button::new(RichText::new("+").size(18.0).color(TEXT_FAINT))
                    .min_size(Vec2::new(36.0, 36.0)).rounding(10.0)
                    .stroke(egui::Stroke::new(1.5, BORDER))
                ).clicked() {
                    self.modal = Modal::CreateServer;
                    self.modal_input.clear();
                }

                // Connect
                if ui.add(egui::Button::new(RichText::new("->").size(11.0).color(TEXT_FAINT))
                    .rounding(4.0)
                ).on_hover_text("Connect to Peer").clicked() {
                    self.modal = Modal::ConnectPeer;
                    self.modal_input.clear();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Settings
                    if ui.add(egui::Button::new(RichText::new("*").size(14.0).color(TEXT_FAINT))
                        .frame(false)
                    ).clicked() {
                        self.state.audio.available_inputs = crate::media::list_input_devices();
                        self.state.audio.available_outputs = crate::media::list_output_devices();
                        self.modal = Modal::Settings;
                    }

                    // Identity
                    ui.label(RichText::new(&self.state.profile.display_name).size(12.0).color(TEXT_DIM));
                    ui.label(RichText::new(initials(&self.state.profile.display_name)).size(11.0).color(TEXT_DIM));

                    // Green dot
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(6.0, 6.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 3.0, GREEN);
                });
            });
        });

        // ── Left sidebar ──
        egui::SidePanel::left("sidebar").default_width(240.0).frame(
            egui::Frame::new().fill(BG_BASE).inner_margin(0.0)
        ).show(ctx, |ui| {
            // Server name header
            let server_name = self.state.active_server
                .and_then(|i| self.state.servers.get(i))
                .map(|s| s.name.clone())
                .unwrap_or_default();
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.label(RichText::new(&server_name).size(14.0).strong().color(TEXT_BRIGHT));
            });
            ui.add_space(6.0);
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Channels
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.label(RichText::new("CHANNELS").size(10.0).color(TEXT_FAINT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("+").clicked() {
                            self.modal = Modal::CreateChannel;
                            self.modal_input.clear();
                        }
                    });
                });

                if let Some(si) = self.state.active_server {
                    if let Some(server) = self.state.servers.clone().get(si) {
                        for (i, channel) in server.channels.iter().enumerate() {
                            let active = self.state.active_channel == Some(i);
                            let topic = server.topic_for_channel(&channel.id);
                            let unread = self.state.unread.get(&topic).copied().unwrap_or(0);

                            ui.horizontal(|ui| {
                                ui.add_space(14.0);
                                let text = RichText::new(format!("# {}", channel.name))
                                    .size(13.0)
                                    .color(if active { TEXT_BRIGHT } else { TEXT_MUTED });

                                let btn = ui.add(egui::Button::new(text)
                                    .fill(if active { Color32::from_rgb(20, 20, 20) } else { Color32::TRANSPARENT })
                                    .frame(false)
                                );
                                if btn.clicked() {
                                    self.state.active_channel = Some(i);
                                    self.state.active_dm_peer = None;
                                    if let Some(t) = self.state.current_topic() {
                                        self.state.load_messages_for_topic(&t);
                                        self.state.mark_read(&t);
                                    }
                                }

                                if unread > 0 {
                                    let (r, _) = ui.allocate_exact_size(Vec2::new(5.0, 5.0), egui::Sense::hover());
                                    ui.painter().circle_filled(r.center(), 2.5, TEXT_BRIGHT);
                                }
                            });
                        }
                    }
                }

                // DMs
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.label(RichText::new("DIRECT").size(10.0).color(TEXT_FAINT));
                });

                for (pid, profile) in self.state.peers.clone() {
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(6.0, 6.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 3.0, status_color(profile.status));
                        ui.add_space(4.0);
                        if ui.add(egui::Button::new(
                            RichText::new(&profile.display_name).size(12.0).color(TEXT_MUTED)
                        ).frame(false)).clicked() {
                            self.state.active_dm_peer = Some(pid.clone());
                        }
                    });
                }
            });

            // User panel at bottom
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.add_space(4.0);
                egui::Frame::new().fill(Color32::from_rgb(10, 10, 10)).inner_margin(8.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(6.0, 6.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 3.0, status_color(self.state.profile.status));
                        ui.add_space(4.0);
                        ui.label(RichText::new(&self.state.profile.display_name).size(12.0).color(TEXT_NORMAL));
                    });
                });
            });
        });

        // ── Right panel (members or search) ──
        egui::SidePanel::right("right").default_width(200.0).frame(
            egui::Frame::new().fill(BG_BASE).inner_margin(8.0)
        ).show(ctx, |ui| {
            if self.search_open {
                ui.label(RichText::new("Search").size(13.0).color(TEXT_BRIGHT));
                ui.add_space(4.0);
                let resp = ui.text_edit_singleline(&mut self.search_query);
                if resp.changed() && self.search_query.len() >= 2 {
                    self.search_results = self.state.search_messages(&self.search_query);
                }
                ui.add_space(8.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for msg in &self.search_results.clone() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&msg.sender_name).size(11.0).color(TEXT_NORMAL));
                                ui.label(RichText::new(msg.timestamp.format("%H:%M").to_string()).size(9.0).color(TEXT_FAINT));
                            });
                            let preview = if msg.content.len() > 60 { format!("{}...", &msg.content[..60]) } else { msg.content.clone() };
                            ui.label(RichText::new(preview).size(11.0).color(TEXT_DIM));
                        });
                    }
                });
            } else {
                // Members
                let online = self.state.peers.values().filter(|p| p.status != UserStatus::Offline).count() + 1;
                ui.label(RichText::new(format!("ONLINE — {}", online)).size(10.0).color(TEXT_FAINT));
                ui.add_space(4.0);

                // Self
                ui.horizontal(|ui| {
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(6.0, 6.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 3.0, STATUS_ONLINE);
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.state.profile.display_name).size(11.0).color(TEXT_NORMAL));
                    ui.label(RichText::new("you").size(9.0).color(TEXT_FAINT));
                });

                for (_, profile) in &self.state.peers.clone() {
                    if profile.status == UserStatus::Offline { continue; }
                    ui.horizontal(|ui| {
                        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(6.0, 6.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 3.0, status_color(profile.status));
                        ui.add_space(4.0);
                        ui.label(RichText::new(&profile.display_name).size(11.0).color(TEXT_DIM));
                    });
                }
            }
        });

        // ── Central panel (messages + input) ──
        egui::CentralPanel::default().frame(
            egui::Frame::new().fill(BG_MAIN).inner_margin(0.0)
        ).show(ctx, |ui| {
            // Header
            let channel_name = self.state.active_server
                .and_then(|si| self.state.servers.get(si))
                .and_then(|s| self.state.active_channel.and_then(|ci| s.channels.get(ci)))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "general".into());

            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new("#").size(14.0).color(TEXT_MUTED));
                ui.label(RichText::new(&channel_name).size(14.0).strong().color(TEXT_BRIGHT));
                ui.separator();
                ui.label(RichText::new(format!("{} peers", self.state.peers.len() + 1)).size(11.0).color(TEXT_FAINT));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Search button
                    if ui.add(egui::Button::new(RichText::new("search").size(11.0).color(TEXT_FAINT)).frame(false)).clicked() {
                        self.search_open = !self.search_open;
                    }
                    // P2P badge
                    ui.horizontal(|ui| {
                        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(5.0, 5.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 2.5, GREEN);
                        ui.label(RichText::new("p2p").size(10.0).color(TEXT_FAINT));
                    });
                });
            });
            ui.separator();

            // Messages
            let messages = self.state.current_messages().into_iter().cloned().collect::<Vec<_>>();
            let available = ui.available_height() - 50.0; // reserve for input

            egui::ScrollArea::vertical().max_height(available).stick_to_bottom(true).show(ui, |ui| {
                // Welcome
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("#_").size(20.0).color(TEXT_FAINT).monospace());
                        ui.label(RichText::new(&channel_name).size(18.0).strong().color(TEXT_BRIGHT));
                        ui.label(RichText::new("Beginning of this channel. End-to-end encrypted.").size(12.0).color(TEXT_FAINT));
                    });
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Render messages
                let mut last_sender: Option<String> = None;
                for msg in &messages {
                    let is_cont = last_sender.as_ref() == Some(&msg.sender_peer_id);

                    if !is_cont {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            // Avatar
                            let (av_rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), egui::Sense::hover());
                            ui.painter().rect_filled(av_rect, 8.0, avatar_color(&msg.sender_name));
                            ui.painter().text(
                                av_rect.center(), egui::Align2::CENTER_CENTER,
                                initials(&msg.sender_name),
                                egui::FontId::proportional(12.0), TEXT_DIM,
                            );
                            ui.add_space(10.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&msg.sender_name).size(12.0).strong().color(TEXT_NORMAL));
                                    ui.label(RichText::new(msg.timestamp.format("%H:%M").to_string()).size(9.0).color(TEXT_FAINT));
                                    ui.label(RichText::new("e2e").size(8.0).color(Color32::from_rgb(30, 30, 30)));
                                    if msg.edited {
                                        ui.label(RichText::new("(edited)").size(9.0).color(TEXT_FAINT));
                                    }
                                });
                                // Rich text with basic markdown
                                render_message_text(ui, &msg.content);

                                // Reactions
                                if !msg.reactions.is_empty() {
                                    ui.horizontal(|ui| {
                                        for (emoji, peers) in &msg.reactions {
                                            if ui.small_button(format!("{} {}", emoji, peers.len())).clicked() {
                                                // Toggle reaction
                                            }
                                        }
                                    });
                                }
                            });
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.add_space(68.0); // align with text above
                            render_message_text(ui, &msg.content);
                        });
                    }
                    last_sender = Some(msg.sender_peer_id.clone());
                }
            });

            // Typing indicator
            if !self.typing_names.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    let dots = ".".repeat(((self.anim_tick / 30) % 4) as usize);
                    let who = if self.typing_names.len() == 1 {
                        format!("{} is typing{}", self.typing_names[0], dots)
                    } else {
                        format!("several people are typing{}", dots)
                    };
                    ui.label(RichText::new(who).size(10.0).color(TEXT_FAINT));
                });
            }

            // Reply banner
            if let Some(ref reply_id) = self.reply_to_id.clone() {
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    let preview = self.state.find_message(reply_id)
                        .map(|m| format!("Replying to {}", m.sender_name))
                        .unwrap_or_else(|| "Replying...".into());
                    ui.label(RichText::new(preview).size(11.0).color(TEXT_DIM));
                    if ui.small_button("x").clicked() {
                        self.reply_to_id = None;
                    }
                });
            }

            // Input
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                egui::Frame::new().fill(BG_ELEVATED).rounding(8.0).inner_margin(8.0).show(ui, |ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.state.input_buffer)
                            .desired_width(ui.available_width())
                            .hint_text(format!("message #{} — encrypted", channel_name))
                            .frame(false)
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.send_message();
                        resp.request_focus();
                    }
                });
                ui.add_space(20.0);
            });
            ui.add_space(16.0);
        });

        // ── Modal ──
        if self.modal != Modal::None {
            self.show_modal(ctx);
        }

        // Request repaint for animations
        ctx.request_repaint();
    }
}

impl MurmurApp {
    fn show_modal(&mut self, ctx: &egui::Context) {
        let (title, placeholder) = match self.modal {
            Modal::CreateServer => ("Create Server", "Server name..."),
            Modal::JoinServer => ("Join Server", "Server name or invite code..."),
            Modal::CreateChannel => ("Create Channel", "channel-name"),
            Modal::ChangeDisplayName => ("Change Name", "Display name..."),
            Modal::ConnectPeer => ("Connect to Peer", "/ip4/.../tcp/.../p2p/12D3K..."),
            Modal::Settings => ("Settings", ""),
            Modal::None => return,
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([400.0, if self.modal == Modal::Settings { 500.0 } else { 180.0 }])
            .show(ctx, |ui| {
                if self.modal == Modal::Settings {
                    self.show_settings(ui);
                    return;
                }

                ui.add_space(8.0);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.modal_input)
                        .hint_text(placeholder)
                        .desired_width(f32::INFINITY)
                );

                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.confirm_modal();
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.modal = Modal::None;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(match self.modal {
                            Modal::CreateServer => "Create",
                            Modal::JoinServer => "Join",
                            Modal::ConnectPeer => "Connect",
                            _ => "Save",
                        }).clicked() {
                            self.confirm_modal();
                        }
                    });
                });
            });
    }

    fn confirm_modal(&mut self) {
        let input = self.modal_input.trim().to_string();
        if input.is_empty() { self.modal = Modal::None; return; }

        match self.modal {
            Modal::CreateServer => {
                let server = Server::new_owned(input, self.state.profile.peer_id.clone());
                let topic = server.topic_for_channel("general");
                self.state.add_server(server);
                let _ = self.cmd_tx.send(NetCommand::JoinServer(topic));
            }
            Modal::JoinServer => {
                if let Some((name, addrs)) = Server::parse_invite(&input) {
                    let mut server = Server::new(name);
                    for addr_str in &addrs {
                        if let Ok(addr) = addr_str.parse() {
                            let _ = self.cmd_tx.send(NetCommand::Dial(addr));
                        }
                        server.add_peer("unknown", vec![addr_str.clone()]);
                    }
                    let topic = server.topic_for_channel("general");
                    self.state.add_server(server);
                    let _ = self.cmd_tx.send(NetCommand::JoinServer(topic));
                } else {
                    let server = Server::new(input);
                    let topic = server.topic_for_channel("general");
                    self.state.add_server(server);
                    let _ = self.cmd_tx.send(NetCommand::JoinServer(topic));
                }
            }
            Modal::CreateChannel => {
                if let Some(si) = self.state.active_server {
                    if let Some(server) = self.state.servers.get(si) {
                        let channel = Channel { id: input.to_lowercase().replace(' ', "-"), name: input.clone() };
                        let server_id = server.id.clone();
                        self.state.add_channel_to_server(&server_id, channel.clone());
                        let _ = self.cmd_tx.send(NetCommand::CreateChannel { server_id, channel });
                    }
                }
            }
            Modal::ChangeDisplayName => {
                self.state.set_display_name(input);
                let _ = self.cmd_tx.send(NetCommand::UpdatePresence(self.state.profile.status));
            }
            Modal::ConnectPeer => {
                if let Ok(addr) = input.parse() {
                    let _ = self.cmd_tx.send(NetCommand::Dial(addr));
                }
            }
            _ => {}
        }
        self.modal = Modal::None;
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Addresses
            ui.label(RichText::new("YOUR ADDRESSES").size(10.0).color(TEXT_FAINT));
            for addr in &self.state.listen_addrs.clone() {
                if ui.add(egui::Button::new(RichText::new(addr).size(10.0).color(TEXT_DIM)).frame(false)).clicked() {
                    ui.output_mut(|o| o.copied_text = addr.clone());
                }
            }
            ui.separator();

            // Volume
            ui.horizontal(|ui| {
                ui.label("Input Volume");
                ui.add(egui::Slider::new(&mut self.state.audio.input_volume, 0.0..=2.0).text(""));
            });
            ui.horizontal(|ui| {
                ui.label("Output Volume");
                ui.add(egui::Slider::new(&mut self.state.audio.output_volume, 0.0..=2.0).text(""));
            });
            ui.separator();

            // Noise suppression
            ui.checkbox(&mut self.state.audio.noise_suppression, "Noise Suppression (RNNoise)");
            ui.separator();

            // Mic test
            ui.horizontal(|ui| {
                ui.label("Mic Test");
                if self.state.audio.mic_testing {
                    if ui.button("Stop").clicked() {
                        if let Some(mut t) = self.mic_tester.take() { t.stop(); }
                        self.state.audio.mic_testing = false;
                        self.state.audio.mic_level = 0.0;
                    }
                } else {
                    if ui.button("Start").clicked() {
                        if let Ok(t) = crate::media::MicTester::start(&self.state.audio.input_device, self.state.audio.input_volume) {
                            self.mic_tester = Some(t);
                            self.state.audio.mic_testing = true;
                        }
                    }
                }
            });
            let level = self.state.audio.mic_level;
            let bar_color = if level > 0.7 { RED } else if level > 0.4 { YELLOW } else { GREEN };
            ui.add(egui::ProgressBar::new(level).fill(bar_color));

            ui.add_space(12.0);
            if ui.button("Close").clicked() {
                self.state.save_audio_settings();
                self.modal = Modal::None;
            }
        });
    }
}

/// Render message text with basic inline markdown.
fn render_message_text(ui: &mut egui::Ui, text: &str) {
    // Simple inline markdown: **bold**, *italic*, `code`
    // For now, parse basic patterns
    let mut job = egui::text::LayoutJob::default();

    let mut i = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    while i < len {
        if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            // Bold
            let start = i + 2;
            if let Some(end) = text[start..].find("**") {
                job.append(&text[start..start + end], 0.0,
                    egui::TextFormat { color: TEXT_BRIGHT, font_id: egui::FontId::proportional(13.0), ..Default::default() });
                i = start + end + 2;
                continue;
            }
        }
        if bytes[i] == b'`' {
            // Code
            let start = i + 1;
            if let Some(end) = text[start..].find('`') {
                job.append(&text[start..start + end], 0.0,
                    egui::TextFormat {
                        color: TEXT_NORMAL,
                        font_id: egui::FontId::monospace(12.0),
                        background: Color32::from_rgb(26, 26, 26),
                        ..Default::default()
                    });
                i = start + end + 1;
                continue;
            }
        }
        if bytes[i] == b'*' && (i == 0 || bytes[i - 1] != b'*') {
            // Italic
            let start = i + 1;
            if let Some(end) = text[start..].find('*') {
                if start + end < len && (start + end + 1 >= len || bytes[start + end + 1] != b'*') {
                    job.append(&text[start..start + end], 0.0,
                        egui::TextFormat { color: TEXT_DIM, italics: true, font_id: egui::FontId::proportional(13.0), ..Default::default() });
                    i = start + end + 1;
                    continue;
                }
            }
        }

        // Regular text — accumulate until next special char
        let start = i;
        while i < len && bytes[i] != b'*' && bytes[i] != b'`' {
            i += 1;
        }
        if i > start {
            job.append(&text[start..i], 0.0,
                egui::TextFormat { color: TEXT_DIM, font_id: egui::FontId::proportional(13.0), ..Default::default() });
        }
    }

    ui.label(job);
}
