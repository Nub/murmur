mod components;
mod theme;

use self::components::{pad4, *};
use self::theme::C;
use crate::network::NetCommand;
use crate::state::AppState;
use crate::types::*;
use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    text_input, tooltip, Column, Row, Space,
};
use iced::{Color, Element, Length, Padding, Subscription, Task as IcedTask, Theme};
use tokio::sync::mpsc;

// ── Messages ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    // Input
    InputChanged(String),
    SendMessage,
    // Navigation
    SelectServer(usize),
    SelectChannel(usize),
    SelectDM(String),
    // Server management
    CreateServer,
    JoinServer,
    CreateChannel,
    ServerNameInput(String),
    ChannelNameInput(String),
    // Voice & screen
    ToggleVoice,
    ToggleMute,
    ToggleDeafen,
    ToggleScreenShare,
    // Network events
    NetEvent(NetEvent),
    // Presence
    SetStatus(UserStatus),
    // Settings
    ChangeDisplayName,
    DisplayNameInput(String),
    OpenSettings,
    // Audio settings
    SetInputDevice(String),
    SetOutputDevice(String),
    SetInputVolume(f32),
    SetOutputVolume(f32),
    ToggleNoiseSuppression,
    SetVadThreshold(f32),
    StartMicTest,
    StopMicTest,
    // Connect to peer
    ConnectToPeer,
    ConnectAddrInput(String),
    // Modal
    CloseModal,
    ConfirmModal,
    // Context menu
    OpenContextMenu(ContextMenuKind),
    CloseContextMenu,
    // Context menu actions
    CopyMessageContent(String),
    DeleteServer(usize),
    LeaveServer(usize),
    CopyPeerId(String),
    MuteChannel(usize),
    // Poll network events
    PollNetwork,
    // Tick (for periodic tasks and animations)
    Tick,
    AnimTick,
    // Clipboard
    CopyToClipboard(String),
    // No-op (for non-functional items)
    Noop,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContextMenuKind {
    Server(usize),
    Channel(usize),
    Member(String),
    Message(String), // message id
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveModal {
    None,
    CreateServer,
    JoinServer,
    CreateChannel,
    ChangeDisplayName,
    Settings,
    ConnectPeer,
}

// ── App state ───────────────────────────────────────────────────────────────

pub struct MurmurApp {
    state: AppState,
    net_cmd_tx: mpsc::UnboundedSender<NetCommand>,
    net_event_rx: Option<mpsc::UnboundedReceiver<NetEvent>>,
    modal: ActiveModal,
    modal_input: String,
    connected: bool,
    context_menu: Option<ContextMenuKind>,
    anim_tick: u64,
    typing_names: Vec<String>,
    mic_tester: Option<crate::media::MicTester>,
}

impl MurmurApp {
    pub fn new(
        state: AppState,
        net_cmd_tx: mpsc::UnboundedSender<NetCommand>,
        net_event_rx: mpsc::UnboundedReceiver<NetEvent>,
        test_scene: Option<String>,
    ) -> (Self, IcedTask<Message>) {
        // Determine initial modal based on test scene
        let modal = match test_scene.as_deref() {
            Some("settings") => ActiveModal::Settings,
            Some("create-server") => ActiveModal::CreateServer,
            _ => ActiveModal::None,
        };

        let app = Self {
            state,
            net_cmd_tx,
            net_event_rx: Some(net_event_rx),
            modal,
            modal_input: String::new(),
            connected: test_scene.is_some(), // Show as connected in test mode
            context_menu: None,
            anim_tick: 0,
            typing_names: Vec::new(),
            mic_tester: None,
        };
        (app, IcedTask::none())
    }

    pub fn title(&self) -> String {
        let peer_short = if self.state.profile.peer_id.len() >= 8 {
            &self.state.profile.peer_id[..8]
        } else {
            &self.state.profile.peer_id
        };
        format!("murmur - {} [{}]", self.state.profile.display_name, peer_short)
    }

    // ── Update ──────────────────────────────────────────────────────────

    pub fn update(&mut self, message: Message) -> IcedTask<Message> {
        // Close context menu on any click that isn't opening one
        match &message {
            Message::OpenContextMenu(_) | Message::CloseContextMenu => {}
            _ => {
                if self.context_menu.is_some() {
                    self.context_menu = None;
                }
            }
        }

        match message {
            Message::InputChanged(val) => {
                self.state.input_buffer = val;
            }

            Message::SendMessage => {
                let content = self.state.input_buffer.trim().to_string();
                if content.is_empty() {
                    return IcedTask::none();
                }
                if content.starts_with('/') {
                    return self.handle_command(&content);
                }

                if let Some(ref peer_id) = self.state.active_dm_peer.clone() {
                    let dm = DirectMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        from_peer_id: self.state.profile.peer_id.clone(),
                        from_name: self.state.profile.display_name.clone(),
                        to_peer_id: peer_id.clone(),
                        content,
                        timestamp: chrono::Utc::now(),
                    };
                    self.state.add_direct_message(dm.clone());
                    let _ = self.net_cmd_tx.send(NetCommand::SendDirectMessage(dm));
                } else if let (Some(si), Some(ci)) =
                    (self.state.active_server, self.state.active_channel)
                {
                    if let Some(server) = self.state.servers.get(si) {
                        if let Some(channel) = server.channels.get(ci) {
                            let msg = ChatMessage::new(
                                server.id.clone(),
                                channel.id.clone(),
                                self.state.profile.peer_id.clone(),
                                self.state.profile.display_name.clone(),
                                content,
                            );
                            self.state.add_message(msg.clone());
                            let _ = self.net_cmd_tx.send(NetCommand::SendMessage(msg));
                        }
                    }
                }
                self.state.input_buffer.clear();
            }

            Message::SelectServer(idx) => {
                self.state.active_server = Some(idx);
                self.state.active_channel = Some(0);
                self.state.active_dm_peer = None;
                self.subscribe_to_current_channel();
            }

            Message::SelectChannel(idx) => {
                self.state.active_channel = Some(idx);
                self.state.active_dm_peer = None;
                self.subscribe_to_current_channel();
            }

            Message::SelectDM(peer_id) => {
                self.state.active_dm_peer = Some(peer_id);
            }

            Message::CreateServer => {
                self.modal = ActiveModal::CreateServer;
                self.modal_input.clear();
            }

            Message::JoinServer => {
                self.modal = ActiveModal::JoinServer;
                self.modal_input.clear();
            }

            Message::CreateChannel => {
                self.modal = ActiveModal::CreateChannel;
                self.modal_input.clear();
            }

            Message::ChangeDisplayName => {
                self.modal = ActiveModal::ChangeDisplayName;
                self.modal_input = self.state.profile.display_name.clone();
            }

            Message::ConnectToPeer => {
                self.modal = ActiveModal::ConnectPeer;
                self.modal_input.clear();
            }
            Message::ConnectAddrInput(val) => {
                self.modal_input = val;
            }

            Message::OpenSettings => {
                // Enumerate audio devices
                self.state.audio.available_inputs = crate::media::list_input_devices();
                self.state.audio.available_outputs = crate::media::list_output_devices();
                self.modal = ActiveModal::Settings;
            }

            Message::SetInputDevice(name) => {
                self.state.audio.input_device = if name == "Default" { None } else { Some(name) };
            }
            Message::SetOutputDevice(name) => {
                self.state.audio.output_device = if name == "Default" { None } else { Some(name) };
            }
            Message::SetInputVolume(v) => {
                self.state.audio.input_volume = v;
            }
            Message::SetOutputVolume(v) => {
                self.state.audio.output_volume = v;
            }
            Message::ToggleNoiseSuppression => {
                self.state.audio.noise_suppression = !self.state.audio.noise_suppression;
            }
            Message::SetVadThreshold(v) => {
                self.state.audio.vad_threshold = v;
            }
            Message::StartMicTest => {
                match crate::media::MicTester::start(
                    &self.state.audio.input_device,
                    self.state.audio.input_volume,
                ) {
                    Ok(tester) => {
                        self.mic_tester = Some(tester);
                        self.state.audio.mic_testing = true;
                    }
                    Err(e) => {
                        tracing::error!("Mic test failed: {}", e);
                    }
                }
            }
            Message::StopMicTest => {
                if let Some(mut tester) = self.mic_tester.take() {
                    tester.stop();
                }
                self.state.audio.mic_testing = false;
                self.state.audio.mic_level = 0.0;
            }

            Message::ServerNameInput(val)
            | Message::ChannelNameInput(val)
            | Message::DisplayNameInput(val) => {
                self.modal_input = val;
            }

            Message::ConfirmModal => {
                let input = self.modal_input.trim().to_string();
                if input.is_empty() {
                    self.modal = ActiveModal::None;
                    return IcedTask::none();
                }
                match self.modal {
                    ActiveModal::CreateServer => {
                        let server = Server::new(input);
                        let topic = server.topic_for_channel("general");
                        self.state.add_server(server);
                        let _ = self.net_cmd_tx.send(NetCommand::JoinServer(topic));
                    }
                    ActiveModal::JoinServer => {
                        let server = Server::new(input);
                        let topic = server.topic_for_channel("general");
                        self.state.add_server(server);
                        let _ = self.net_cmd_tx.send(NetCommand::JoinServer(topic));
                    }
                    ActiveModal::CreateChannel => {
                        if let Some(si) = self.state.active_server {
                            if let Some(server) = self.state.servers.get(si) {
                                let channel = Channel {
                                    id: input.to_lowercase().replace(' ', "-"),
                                    name: input.clone(),
                                };
                                let server_id = server.id.clone();
                                self.state
                                    .add_channel_to_server(&server_id, channel.clone());
                                let _ = self.net_cmd_tx.send(NetCommand::CreateChannel {
                                    server_id,
                                    channel,
                                });
                            }
                        }
                    }
                    ActiveModal::ChangeDisplayName => {
                        self.state.set_display_name(input);
                        let _ = self.net_cmd_tx.send(NetCommand::UpdatePresence(
                            self.state.profile.status,
                        ));
                    }
                    ActiveModal::ConnectPeer => {
                        if let Ok(addr) = input.parse() {
                            let _ = self.net_cmd_tx.send(NetCommand::Dial(addr));
                        }
                    }
                    ActiveModal::None | ActiveModal::Settings => {}
                }
                self.modal = ActiveModal::None;
            }

            Message::CloseModal => {
                self.modal = ActiveModal::None;
            }

            Message::ToggleVoice => {
                self.state.voice_active = !self.state.voice_active;
                if self.state.voice_active {
                    // Set voice channel name
                    if let (Some(si), Some(ci)) = (self.state.active_server, self.state.active_channel) {
                        if let Some(server) = self.state.servers.get(si) {
                            if let Some(channel) = server.channels.get(ci) {
                                self.state.voice_channel_name = Some(channel.name.clone());
                            }
                        }
                    }
                    if let Some(topic) = self.state.current_topic() {
                        let _ = self.net_cmd_tx.send(NetCommand::StartVoice(topic));
                    }
                } else {
                    let _ = self.net_cmd_tx.send(NetCommand::StopVoice);
                    self.state.screen_sharing = false;
                    self.state.voice_muted = false;
                    self.state.voice_deafened = false;
                    self.state.voice_peers.clear();
                    self.state.voice_channel_name = None;
                }
            }

            Message::ToggleMute => {
                self.state.voice_muted = !self.state.voice_muted;
            }

            Message::ToggleDeafen => {
                self.state.voice_deafened = !self.state.voice_deafened;
                // Deafening also mutes
                if self.state.voice_deafened {
                    self.state.voice_muted = true;
                }
            }

            Message::ToggleScreenShare => {
                self.state.screen_sharing = !self.state.screen_sharing;
                if self.state.screen_sharing {
                    if let Some(topic) = self.state.current_topic() {
                        let _ = self.net_cmd_tx.send(NetCommand::StartScreenShare(topic));
                    }
                } else {
                    let _ = self.net_cmd_tx.send(NetCommand::StopScreenShare);
                }
            }

            Message::SetStatus(status) => {
                self.state.profile.status = status;
                let _ = self.net_cmd_tx.send(NetCommand::UpdatePresence(status));
            }

            Message::OpenContextMenu(kind) => {
                self.context_menu = Some(kind);
            }

            Message::CloseContextMenu => {
                self.context_menu = None;
            }

            // Context menu actions
            Message::CopyMessageContent(_content) => {
                // Clipboard integration would go here
            }
            Message::DeleteServer(idx) => {
                if idx < self.state.servers.len() {
                    self.state.servers.remove(idx);
                    if self.state.active_server == Some(idx) {
                        self.state.active_server = if self.state.servers.is_empty() {
                            None
                        } else {
                            Some(0)
                        };
                        self.state.active_channel = Some(0);
                    }
                }
            }
            Message::LeaveServer(idx) => {
                if let Some(server) = self.state.servers.get(idx) {
                    for channel in &server.channels {
                        let topic = server.topic_for_channel(&channel.id);
                        let _ = self.net_cmd_tx.send(NetCommand::LeaveServer(topic));
                    }
                }
                if idx < self.state.servers.len() {
                    self.state.servers.remove(idx);
                    if self.state.active_server == Some(idx) {
                        self.state.active_server = if self.state.servers.is_empty() {
                            None
                        } else {
                            Some(0)
                        };
                    }
                }
            }
            Message::CopyPeerId(_peer_id) => {}
            Message::MuteChannel(_idx) => {}

            Message::PollNetwork => {
                let events: Vec<NetEvent> = if let Some(ref mut rx) = self.net_event_rx {
                    let mut evts = Vec::new();
                    while let Ok(event) = rx.try_recv() {
                        evts.push(event);
                    }
                    evts
                } else {
                    Vec::new()
                };
                for event in events {
                    self.handle_net_event(event);
                }
            }

            Message::NetEvent(event) => {
                self.handle_net_event(event);
            }

            Message::Tick => {
                let _ = self
                    .net_cmd_tx
                    .send(NetCommand::UpdatePresence(self.state.profile.status));
            }

            Message::AnimTick => {
                self.anim_tick = self.anim_tick.wrapping_add(1);
                // Update mic test level
                if let Some(ref tester) = self.mic_tester {
                    self.state.audio.mic_level = tester.level();
                }
            }

            Message::CopyToClipboard(text) => {
                return iced::clipboard::write(text);
            }
            Message::Noop => {}
        }
        IcedTask::none()
    }

    // ── View ────────────────────────────────────────────────────────────

    pub fn view(&self) -> Element<Message> {
        // New layout: column![topbar, row![sidebar, main, members]]
        let content = column![
            self.view_topbar(),
            row![
                self.view_channel_sidebar(),
                self.view_main_area(),
                self.view_member_sidebar(),
            ].height(Length::Fill),
        ];

        let mut layers: Vec<Element<Message>> = vec![content.into()];

        // Context menu overlay
        if let Some(ref ctx) = self.context_menu {
            layers.push(
                // Clickable backdrop to close
                iced::widget::mouse_area(
                    container(self.view_context_menu(ctx))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(pad4(100.0, 0.0, 0.0, 300.0)),
                )
                .on_press(Message::CloseContextMenu)
                .into(),
            );
        }

        // Modal overlay
        if self.modal != ActiveModal::None {
            layers.push(
                container(self.view_modal())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .style(theme::modal_backdrop)
                    .into(),
            );
        }

        if layers.len() == 1 {
            layers.into_iter().next().unwrap()
        } else {
            let mut stack = iced::widget::Stack::new();
            for layer in layers {
                stack = stack.push(layer);
            }
            stack.into()
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::PollNetwork),
            iced::time::every(std::time::Duration::from_secs(30)).map(|_| Message::Tick),
            // Animation tick for typing indicator dots, etc.
            iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::AnimTick),
        ])
    }

    // ── Top bar (horizontal server strip + identity) ──────────────────

    fn view_topbar(&self) -> Element<Message> {
        let mut bar = Row::new().spacing(6).align_y(iced::Alignment::Center);

        // Home button
        bar = bar.push(
            button(
                container(text("dc").size(13).color(C::GREEN))
                    .width(36).height(36).center_x(36).center_y(36),
            )
            .on_press(Message::Noop)
            .style(theme::server_icon_active),
        );

        // Separator
        bar = bar.push(
            container(Space::new(1, 24))
                .style(|_t: &Theme| container::Style {
                    background: Some(C::BORDER.into()), ..Default::default()
                }),
        );

        // Server icons
        for (i, server) in self.state.servers.iter().enumerate() {
            let is_active = self.state.active_server == Some(i);
            bar = bar.push(server_icon_widget(&server.name, i, is_active, false));
        }

        // Separator + Add
        bar = bar.push(
            container(Space::new(1, 24))
                .style(|_t: &Theme| container::Style {
                    background: Some(C::BORDER.into()), ..Default::default()
                }),
        );
        bar = bar.push(
            button(
                container(text("+").size(18).color(C::TEXT_FAINT))
                    .width(36).height(36).center_x(36).center_y(36),
            )
            .on_press(Message::CreateServer)
            .style(theme::add_server_icon),
        );

        // Connect to peer button
        bar = bar.push(
            tooltip(
                button(text("->").size(11).color(C::TEXT_FAINT))
                    .on_press(Message::ConnectToPeer)
                    .style(theme::icon_button)
                    .padding(Padding::from([8, 10])),
                container(text("Connect to Peer").size(11).color(C::TEXT_DIM))
                    .padding(Padding::from([4, 8]))
                    .style(theme::tooltip_box),
                tooltip::Position::Bottom,
            )
            .gap(6),
        );

        // Spacer + identity on right
        bar = bar.push(horizontal_space());

        // Connection dot + avatar + name
        bar = bar.push(
            container(Space::new(6, 6))
                .style(|_t: &Theme| container::Style {
                    background: Some(C::GREEN.into()),
                    border: iced::Border { radius: 3.0.into(), ..Default::default() },
                    ..Default::default()
                }),
        );
        bar = bar.push(Space::with_width(4));

        let my_name = self.state.profile.display_name.clone();
        bar = bar.push(
            button(
                row![
                    avatar(&my_name, 28.0, None),
                    Space::with_width(6),
                    text(my_name).size(12).color(C::TEXT_DIM),
                ].align_y(iced::Alignment::Center),
            )
            .on_press(Message::ChangeDisplayName)
            .style(theme::icon_button)
            .padding(Padding::from([4, 8])),
        );

        // Settings gear
        bar = bar.push(
            button(text("*").size(14).color(C::TEXT_FAINT))
                .on_press(Message::OpenSettings)
                .style(theme::icon_button)
                .padding(Padding::from([6, 8])),
        );

        container(bar)
            .padding(Padding::from([6, 12]))
            .width(Length::Fill)
            .style(theme::topbar)
            .into()
    }

    // ── Channel sidebar ─────────────────────────────────────────────────

    fn view_channel_sidebar(&self) -> Element<Message> {
        let mut sidebar = Column::new().width(240);

        if let Some(si) = self.state.active_server {
            if let Some(server) = self.state.servers.get(si) {
                // Server name header (Discord-style, 48px tall with bottom shadow)
                sidebar = sidebar.push(
                    container(
                        text(server.name.clone())
                            .size(15)
                            .color(C::TEXT_BRIGHT),
                    )
                    .padding(Padding::from([13, 16]))
                    .width(Length::Fill)
                    .style(theme::server_name_header),
                );

                // Text channels section
                sidebar = sidebar.push(Space::with_height(2));
                sidebar =
                    sidebar.push(section_header("TEXT CHANNELS", Some(Message::CreateChannel)));

                for (i, channel) in server.channels.iter().enumerate() {
                    let is_active = self.state.active_channel == Some(i)
                        && self.state.active_dm_peer.is_none();
                    sidebar = sidebar.push(
                        container(channel_item(&channel.name, i, is_active, 0))
                            .padding(pad4(0.0, 8.0, 0.0, 8.0)),
                    );
                }

                // Voice channels section
                sidebar = sidebar.push(Space::with_height(8));
                sidebar = sidebar.push(section_header("VOICE CHANNELS", None));

                // Voice channel item (Discord-style: channel name, users listed below when connected)
                let vc_name_color = if self.state.voice_active {
                    Color::WHITE
                } else {
                    C::TEXT_MUTED
                };

                // Voice channel button
                sidebar = sidebar.push(
                    container(
                        button(
                            row![
                                text("<<").size(11).color(C::TEXT_FAINT),
                                Space::with_width(5),
                                text("General").size(15).color(vc_name_color),
                            ]
                            .align_y(iced::Alignment::Center),
                        )
                        .on_press(Message::ToggleVoice)
                        .width(Length::Fill)
                        .padding(Padding::from([6, 8]))
                        .style(if self.state.voice_active {
                            theme::channel_button_active
                        } else {
                            theme::channel_button
                        }),
                    )
                    .padding(pad4(0.0, 8.0, 0.0, 8.0)),
                );

                // Show connected users under the voice channel (Discord-style indent)
                if self.state.voice_active {
                    // Show self
                    let self_name = self.state.profile.display_name.clone();
                    let mut self_voice_row = Row::new()
                        .spacing(0)
                        .align_y(iced::Alignment::Center)
                        .push(Space::with_width(28))
                        .push(avatar(&self_name, 20.0, None))
                        .push(Space::with_width(6))
                        .push(text(self_name).size(13).color(C::TEXT_NORMAL));
                    if self.state.voice_muted {
                        self_voice_row = self_voice_row.push(text(" [M]").size(10).color(C::RED));
                    }
                    if self.state.voice_deafened {
                        self_voice_row = self_voice_row.push(text(" [D]").size(10).color(C::RED));
                    }
                    sidebar = sidebar.push(
                        container(
                            self_voice_row,
                        )
                        .padding(pad4(2.0, 8.0, 2.0, 8.0)),
                    );

                    // Show other voice peers
                    for (_pid, vp) in &self.state.voice_peers {
                        let vp_name = vp.display_name.clone();
                        let speaking = vp.speaking;
                        sidebar = sidebar.push(
                            container(
                                {
                                    let mut peer_row = Row::new()
                                        .spacing(0)
                                        .align_y(iced::Alignment::Center)
                                        .push(Space::with_width(28))
                                        .push(
                                            container(avatar(&vp_name, 20.0, None))
                                                .style(move |_t: &Theme| container::Style {
                                                    border: iced::Border {
                                                        width: if speaking { 2.0 } else { 0.0 },
                                                        radius: 10.0.into(),
                                                        color: C::GREEN,
                                                    },
                                                    ..Default::default()
                                                }),
                                        )
                                        .push(Space::with_width(6))
                                        .push(text(vp_name).size(13).color(
                                            if speaking { C::GREEN } else { C::TEXT_NORMAL }
                                        ));
                                    if vp.muted {
                                        peer_row = peer_row.push(text(" [M]").size(10).color(C::RED));
                                    }
                                    peer_row
                                },
                            )
                            .padding(pad4(2.0, 8.0, 2.0, 8.0)),
                        );
                    }
                }
            }
        }

        // Direct Messages
        sidebar = sidebar.push(Space::with_height(12));
        sidebar = sidebar.push(section_header("DIRECT MESSAGES", None));
        for (peer_id, profile) in &self.state.peers {
            let is_active = self.state.active_dm_peer.as_ref() == Some(peer_id);
            let name = profile.display_name.clone();
            let pid = peer_id.clone();

            sidebar = sidebar.push(
                container(
                    button(
                        row![
                            avatar(&name, 24.0, Some(profile.status)),
                            Space::with_width(8),
                            text(name).size(14).color(if is_active {
                                Color::WHITE
                            } else {
                                C::TEXT_MUTED
                            }),
                        ]
                        .align_y(iced::Alignment::Center),
                    )
                    .on_press(Message::SelectDM(pid))
                    .width(Length::Fill)
                    .padding(Padding::from([4, 8]))
                    .style(if is_active {
                        theme::channel_button_active
                    } else {
                        theme::channel_button
                    }),
                )
                .padding(pad4(0.0, 8.0, 0.0, 8.0)),
            );
        }

        // Push user panel to bottom
        sidebar = sidebar.push(Space::with_height(Length::Fill));

        // Voice connected panel (above user panel, Discord-style)
        if self.state.voice_active {
            let ch_name = self.state.voice_channel_name.clone().unwrap_or_else(|| "General".into());
            sidebar = sidebar.push(
                container(
                    column![
                        // "Voice Connected" header with green text
                        row![
                            text("Voice Connected").size(12).color(C::GREEN),
                        ],
                        // Channel name
                        text(format!("#{}", ch_name)).size(11).color(C::TEXT_MUTED),
                        Space::with_height(6),
                        // Control buttons
                        row![
                            // Mute button
                            button(
                                text(if self.state.voice_muted { "Unmute" } else { "Mute" }).size(11),
                            )
                            .on_press(Message::ToggleMute)
                            .style(if self.state.voice_muted {
                                theme::voice_disconnect_button
                            } else {
                                theme::icon_button
                            })
                            .padding(Padding::from([4, 8])),
                            Space::with_width(4),
                            // Deafen button
                            button(
                                text(if self.state.voice_deafened { "Undeaf" } else { "Deafen" }).size(11),
                            )
                            .on_press(Message::ToggleDeafen)
                            .style(if self.state.voice_deafened {
                                theme::voice_disconnect_button
                            } else {
                                theme::icon_button
                            })
                            .padding(Padding::from([4, 8])),
                            Space::with_width(4),
                            // Screen share button
                            button(
                                text(if self.state.screen_sharing { "Stop" } else { "Share" }).size(11),
                            )
                            .on_press(Message::ToggleScreenShare)
                            .style(if self.state.screen_sharing {
                                theme::voice_disconnect_button
                            } else {
                                theme::icon_button
                            })
                            .padding(Padding::from([4, 8])),
                            horizontal_space(),
                            // Disconnect button
                            button(
                                text("Leave").size(11).color(C::RED),
                            )
                            .on_press(Message::ToggleVoice)
                            .style(theme::voice_disconnect_button)
                            .padding(Padding::from([4, 8])),
                        ]
                        .align_y(iced::Alignment::Center),
                    ]
                    .spacing(2),
                )
                .padding(Padding::from([10, 12]))
                .style(theme::voice_connected),
            );
        }

        // User panel
        sidebar = sidebar.push(self.view_user_panel());

        container(sidebar)
            .height(Length::Fill)
            .style(theme::sidebar)
            .into()
    }

    // ── User panel (bottom of channel sidebar) ──────────────────────────

    fn view_user_panel(&self) -> Element<Message> {
        let _status_color = match self.state.profile.status {
            UserStatus::Online => C::STATUS_ONLINE,
            UserStatus::Away => C::STATUS_IDLE,
            UserStatus::DoNotDisturb => C::STATUS_DND,
            UserStatus::Offline => C::STATUS_OFFLINE,
        };

        container(
            row![
                button(
                    row![
                        avatar(
                            &self.state.profile.display_name,
                            32.0,
                            Some(self.state.profile.status),
                        ),
                        Space::with_width(8),
                        column![
                            text(self.state.profile.display_name.clone())
                                .size(13)
                                .color(C::TEXT_NORMAL),
                            text(format!("{}", self.state.profile.status))
                                .size(11)
                                .color(C::TEXT_MUTED),
                        ]
                        .spacing(1),
                    ]
                    .align_y(iced::Alignment::Center),
                )
                .on_press(Message::ChangeDisplayName)
                .style(theme::user_panel_button)
                .padding(Padding::from([4, 8])),
                horizontal_space(),
                // Settings button
                tooltip(
                    button(text("*").size(14).color(C::TEXT_MUTED))
                        .on_press(Message::OpenSettings)
                        .style(theme::icon_button)
                        .padding(Padding::from([6, 8])),
                    container(text("User Settings").size(12).color(C::TEXT_NORMAL))
                        .padding(Padding::from([4, 8]))
                        .style(theme::tooltip_box),
                    tooltip::Position::Top,
                )
                .gap(4),
            ]
            .align_y(iced::Alignment::Center)
            .padding(Padding::from([0, 8])),
        )
        .padding(Padding::from([8, 8]))
        .style(theme::user_panel)
        .into()
    }

    // ── Main area (messages + input) ────────────────────────────────────

    fn view_main_area(&self) -> Element<Message> {
        let mut main_col = Column::new().width(Length::Fill).height(Length::Fill);

        // ── Channel header bar ──
        let header_content = if let Some(ref peer_id) = self.state.active_dm_peer {
            let name = self
                .state
                .peers
                .get(peer_id)
                .map(|p| p.display_name.clone())
                .unwrap_or_else(|| peer_id[..8.min(peer_id.len())].to_string());
            row![
                text("@").size(20).color(C::TEXT_MUTED),
                Space::with_width(6),
                text(name).size(15).color(C::TEXT_BRIGHT),
            ]
            .align_y(iced::Alignment::Center)
        } else if let (Some(si), Some(ci)) = (self.state.active_server, self.state.active_channel) {
            if let Some(server) = self.state.servers.get(si) {
                if let Some(channel) = server.channels.get(ci) {
                    row![
                        text("#").size(22).color(C::TEXT_MUTED),
                        Space::with_width(6),
                        text(channel.name.clone())
                            .size(15)
                            .color(C::TEXT_BRIGHT),
                        Space::with_width(12),
                        container(Space::new(1, 20))
                            .style(|_t: &Theme| container::Style {
                                background: Some(C::BORDER.into()),
                                ..Default::default()
                            }),
                        Space::with_width(12),
                        text(format!("{} members", self.state.peers.len() + 1))
                            .size(13)
                            .color(C::TEXT_MUTED),
                        horizontal_space(),
                        connection_badge(self.connected),
                    ]
                    .align_y(iced::Alignment::Center)
                } else {
                    row![text("Select a channel").size(15).color(C::TEXT_MUTED)]
                }
            } else {
                row![text("Select a server").size(15).color(C::TEXT_MUTED)]
            }
        } else {
            row![text("Welcome to murmur").size(15).color(C::TEXT_BRIGHT)]
        };

        main_col = main_col.push(
            container(header_content)
                .padding(Padding::from([12, 16]))
                .width(Length::Fill)
                .style(theme::main_header),
        );

        // ── Messages area ──
        let mut messages_col = Column::new().spacing(0);

        // Welcome message at top
        if let (Some(si), Some(ci)) = (self.state.active_server, self.state.active_channel) {
            if let Some(server) = self.state.servers.get(si) {
                if let Some(channel) = server.channels.get(ci) {
                    let ch_name = channel.name.clone();
                    messages_col = messages_col.push(
                        container(
                            column![
                                container(
                                    text("#").size(48).color(Color::WHITE),
                                )
                                .width(68)
                                .height(68)
                                .center_x(68)
                                .center_y(68)
                                .style(theme::welcome_icon),
                                Space::with_height(8),
                                text(format!("Welcome to #{}", ch_name))
                                    .size(28)
                                    .color(C::TEXT_BRIGHT),
                                Space::with_height(4),
                                text(format!(
                                    "This is the start of the #{} channel.",
                                    ch_name
                                ))
                                .size(14)
                                .color(C::TEXT_MUTED),
                            ]
                            .spacing(4),
                        )
                        .padding(Padding::from([24, 16])),
                    );

                    messages_col = messages_col.push(date_separator("Today"));
                }
            }
        }

        // Render messages grouped by sender
        if self.state.active_dm_peer.is_some() {
            let msgs = self.state.current_dm_messages();
            let mut last_sender: Option<String> = None;
            for msg in &msgs {
                let is_continuation = last_sender.as_ref() == Some(&msg.from_peer_id);
                messages_col = messages_col.push(dm_widget(msg, is_continuation));
                last_sender = Some(msg.from_peer_id.clone());
            }
        } else {
            let msgs = self.state.current_messages();
            let mut last_sender: Option<String> = None;
            for msg in &msgs {
                let is_continuation = last_sender.as_ref() == Some(&msg.sender_peer_id);
                messages_col =
                    messages_col.push(message_widget(msg, is_continuation, false));
                last_sender = Some(msg.sender_peer_id.clone());
            }
        }

        main_col = main_col.push(
            scrollable(messages_col)
                .height(Length::Fill)
                .anchor_bottom(),
        );

        // ── Typing indicator ──
        main_col = main_col.push(typing_indicator(&self.typing_names, self.anim_tick));

        // ── Input area ──
        let channel_name = if self.state.active_dm_peer.is_some() {
            "Message".to_string()
        } else if let (Some(si), Some(ci)) = (self.state.active_server, self.state.active_channel) {
            self.state
                .servers
                .get(si)
                .and_then(|s| s.channels.get(ci))
                .map(|c| format!("Message #{}", c.name))
                .unwrap_or_else(|| "Type a message...".to_string())
        } else {
            "Type a message...".to_string()
        };

        let input_row = row![
            button(text("+").size(18).color(C::TEXT_MUTED))
                .on_press(Message::Noop)
                .style(theme::icon_button)
                .padding(Padding::from([8, 12])),
            text_input(&channel_name, &self.state.input_buffer)
                .on_input(Message::InputChanged)
                .on_submit(Message::SendMessage)
                .padding(Padding::from([10, 0]))
                .size(14)
                .style(|_t: &Theme, _s| text_input::Style {
                    background: Color::TRANSPARENT.into(),
                    border: iced::Border::default(),
                    icon: C::TEXT_MUTED,
                    placeholder: C::TEXT_MUTED,
                    value: C::TEXT_NORMAL,
                    selection: C::GREEN,
                }),
        ]
        .align_y(iced::Alignment::Center);

        main_col = main_col.push(
            container(
                container(input_row)
                    .padding(Padding::from([4, 8]))
                    .style(|_t: &Theme| container::Style {
                        background: Some(C::BG_ELEVATED.into()),
                        border: iced::Border {
                            width: 0.0,
                            radius: 8.0.into(),
                            color: Color::TRANSPARENT,
                        },
                        ..Default::default()
                    }),
            )
            .padding(pad4(0.0, 16.0, 22.0, 16.0)),
        );

        container(main_col)
            .style(theme::main_area)
            .into()
    }

    // ── Member sidebar ──────────────────────────────────────────────────

    fn view_member_sidebar(&self) -> Element<Message> {
        let mut sidebar = Column::new().spacing(2).width(240).padding(Padding::from([0, 8]));

        // Online section
        let online_count = self
            .state
            .peers
            .values()
            .filter(|p| p.status != UserStatus::Offline)
            .count()
            + 1; // +1 for self

        sidebar = sidebar.push(
            container(
                text(format!("ONLINE - {}", online_count))
                    .size(11)
                    .color(C::TEXT_MUTED),
            )
            .padding(pad4(16.0, 8.0, 4.0, 16.0)),
        );

        // Self
        let self_name = self.state.profile.display_name.clone();
        sidebar = sidebar.push(
            container(
                button(
                    row![
                        avatar(&self_name, 32.0, Some(self.state.profile.status)),
                        Space::with_width(8),
                        column![
                            text(self_name).size(13).color(C::TEXT_NORMAL),
                            text("(you)").size(11).color(C::TEXT_MUTED),
                        ]
                        .spacing(1),
                    ]
                    .align_y(iced::Alignment::Center)
                    .padding(Padding::from([4, 0])),
                )
                .on_press(Message::Noop)
                .width(Length::Fill)
                .padding(Padding::from([2, 8]))
                .style(theme::member_button),
            )
            .padding(Padding::from([0, 8])),
        );

        // Online peers
        let mut offline_peers: Vec<(&String, &UserProfile)> = Vec::new();
        for (peer_id, profile) in &self.state.peers {
            match profile.status {
                UserStatus::Offline => {
                    offline_peers.push((peer_id, profile));
                }
                _ => {
                    sidebar = sidebar.push(
                        container(member_entry(peer_id, profile))
                            .padding(Padding::from([0, 8])),
                    );
                }
            }
        }

        // Offline section
        if !offline_peers.is_empty() {
            sidebar = sidebar.push(Space::with_height(8));
            sidebar = sidebar.push(
                container(
                    text(format!("OFFLINE - {}", offline_peers.len()))
                        .size(11)
                        .color(C::TEXT_MUTED),
                )
                .padding(pad4(8.0, 8.0, 4.0, 16.0)),
            );

            for (peer_id, profile) in offline_peers {
                sidebar = sidebar.push(
                    container(member_entry(peer_id, profile))
                        .padding(Padding::from([0, 8])),
                );
            }
        }

        container(scrollable(sidebar))
            .height(Length::Fill)
            .style(theme::member_panel)
            .into()
    }

    // ── Context menu ────────────────────────────────────────────────────

    fn view_context_menu(&self, kind: &ContextMenuKind) -> Element<Message> {
        let items = match kind {
            ContextMenuKind::Server(idx) => {
                let idx = *idx;
                vec![
                    ContextMenuItem::Action {
                        label: "Create Channel".into(),
                        message: Message::CreateChannel,
                        danger: false,
                    },
                    ContextMenuItem::Action {
                        label: "Invite People".into(),
                        message: Message::Noop,
                        danger: false,
                    },
                    ContextMenuItem::Separator,
                    ContextMenuItem::Action {
                        label: "Notification Settings".into(),
                        message: Message::Noop,
                        danger: false,
                    },
                    ContextMenuItem::Separator,
                    ContextMenuItem::Action {
                        label: "Leave Server".into(),
                        message: Message::LeaveServer(idx),
                        danger: true,
                    },
                ]
            }
            ContextMenuKind::Channel(idx) => {
                let idx = *idx;
                vec![
                    ContextMenuItem::Action {
                        label: "Edit Channel".into(),
                        message: Message::Noop,
                        danger: false,
                    },
                    ContextMenuItem::Action {
                        label: "Mute Channel".into(),
                        message: Message::MuteChannel(idx),
                        danger: false,
                    },
                    ContextMenuItem::Separator,
                    ContextMenuItem::Action {
                        label: "Copy Channel ID".into(),
                        message: Message::Noop,
                        danger: false,
                    },
                ]
            }
            ContextMenuKind::Member(peer_id) => {
                let pid = peer_id.clone();
                vec![
                    ContextMenuItem::Action {
                        label: "Message".into(),
                        message: Message::SelectDM(pid.clone()),
                        danger: false,
                    },
                    ContextMenuItem::Action {
                        label: "Call".into(),
                        message: Message::Noop,
                        danger: false,
                    },
                    ContextMenuItem::Separator,
                    ContextMenuItem::Action {
                        label: "Copy User ID".into(),
                        message: Message::CopyPeerId(pid),
                        danger: false,
                    },
                ]
            }
            ContextMenuKind::Message(msg_id) => {
                let mid = msg_id.clone();
                vec![
                    ContextMenuItem::Action {
                        label: "Reply".into(),
                        message: Message::Noop,
                        danger: false,
                    },
                    ContextMenuItem::Action {
                        label: "Copy Text".into(),
                        message: Message::CopyMessageContent(mid.clone()),
                        danger: false,
                    },
                    ContextMenuItem::Action {
                        label: "Pin Message".into(),
                        message: Message::Noop,
                        danger: false,
                    },
                    ContextMenuItem::Separator,
                    ContextMenuItem::Action {
                        label: "Copy Message ID".into(),
                        message: Message::Noop,
                        danger: false,
                    },
                ]
            }
        };

        context_menu_widget(items)
    }

    // ── Modal ───────────────────────────────────────────────────────────

    fn view_modal(&self) -> Element<Message> {
        if self.modal == ActiveModal::Settings {
            return self.view_settings_modal();
        }

        let (title, subtitle, placeholder) = match self.modal {
            ActiveModal::CreateServer => (
                "Create a Server",
                "Your server is where you and your friends hang out.",
                "Server name...",
            ),
            ActiveModal::JoinServer => (
                "Join a Server",
                "Enter the name of a server to join its community.",
                "Server name...",
            ),
            ActiveModal::CreateChannel => (
                "Create Channel",
                "in the current server",
                "new-channel",
            ),
            ActiveModal::ChangeDisplayName => (
                "Change Display Name",
                "This is how others see you.",
                "Display name...",
            ),
            ActiveModal::ConnectPeer => (
                "Connect to Peer",
                "Paste a peer's multiaddr to connect directly.",
                "/ip4/.../tcp/.../p2p/12D3K...",
            ),
            ActiveModal::None | ActiveModal::Settings => ("", "", ""),
        };

        let input_msg: fn(String) -> Message = match self.modal {
            ActiveModal::CreateServer | ActiveModal::JoinServer => Message::ServerNameInput,
            ActiveModal::CreateChannel => Message::ChannelNameInput,
            ActiveModal::ChangeDisplayName => Message::DisplayNameInput,
            ActiveModal::ConnectPeer => Message::ConnectAddrInput,
            ActiveModal::None | ActiveModal::Settings => Message::ServerNameInput,
        };

        let modal_content = column![
            text(title)
                .size(22)
                .color(C::TEXT_BRIGHT),
            text(subtitle)
                .size(13)
                .color(C::TEXT_MUTED),
            Space::with_height(16),
            text("NAME").size(11).color(C::TEXT_MUTED),
            Space::with_height(4),
            text_input(placeholder, &self.modal_input)
                .on_input(input_msg)
                .on_submit(Message::ConfirmModal)
                .padding(10)
                .size(14)
                .style(theme::modal_input),
            Space::with_height(20),
            // Footer with buttons
            container(
                row![
                    button(
                        text("Cancel").size(14).color(C::TEXT_NORMAL),
                    )
                    .on_press(Message::CloseModal)
                    .style(theme::modal_cancel)
                    .padding(Padding::from([10, 20])),
                    horizontal_space(),
                    button(
                        text(match self.modal {
                            ActiveModal::CreateServer => "Create",
                            ActiveModal::JoinServer => "Join",
                            ActiveModal::CreateChannel => "Create Channel",
                            ActiveModal::ChangeDisplayName => "Save",
                            ActiveModal::ConnectPeer => "Connect",
                            ActiveModal::None | ActiveModal::Settings => "OK",
                        })
                        .size(14),
                    )
                    .on_press(Message::ConfirmModal)
                    .style(theme::modal_confirm)
                    .padding(Padding::from([10, 24])),
                ]
                .align_y(iced::Alignment::Center),
            )
            .padding(pad4(16.0, 0.0, 0.0, 0.0))
            .style(|_t: &Theme| container::Style {
                border: iced::Border {
                    width: 0.0,
                    ..Default::default()
                },
                ..Default::default()
            }),
        ]
        .spacing(4)
        .max_width(360);

        container(
            container(modal_content)
                .padding(pad4(24.0, 24.0, 16.0, 24.0))
                .style(theme::modal_card),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Shrink)
        .center_y(Length::Shrink)
        .into()
    }

    // ── Network event handler ───────────────────────────────────────────

    // ── Settings modal ───────────────────────────────────────────────

    fn view_settings_modal(&self) -> Element<Message> {
        let audio = &self.state.audio;

        // Your addresses section
        let mut addr_col = Column::new().spacing(3);
        addr_col = addr_col.push(text("YOUR ADDRESS (share with friends to connect)").size(10).color(C::TEXT_FAINT));
        if self.state.listen_addrs.is_empty() {
            addr_col = addr_col.push(text("Waiting for network...").size(11).color(C::TEXT_MUTED));
        } else {
            for addr in &self.state.listen_addrs {
                let addr_clone = addr.clone();
                addr_col = addr_col.push(
                    button(
                        text(addr.clone()).size(10).color(C::TEXT_DIM),
                    )
                    .on_press(Message::CopyToClipboard(addr_clone))
                    .padding(Padding::from([3, 6]))
                    .style(|_t: &Theme, status: button::Status| button::Style {
                        background: Some(match status {
                            button::Status::Hovered => Color::from_rgb(0.12, 0.12, 0.12),
                            _ => C::BG_ELEVATED,
                        }.into()),
                        text_color: match status {
                            button::Status::Hovered => C::TEXT_NORMAL,
                            _ => C::TEXT_DIM,
                        },
                        border: iced::Border { radius: 3.0.into(), ..Default::default() },
                        ..Default::default()
                    }),
                );
            }
        }

        // Input device picker
        let mut input_col = Column::new().spacing(4);
        input_col = input_col.push(text("INPUT DEVICE").size(11).color(C::TEXT_MUTED));
        // Default option
        input_col = input_col.push(
            button(
                text(format!("{}  System Default", if audio.input_device.is_none() { ">" } else { " " }))
                    .size(13)
                    .color(if audio.input_device.is_none() { Color::WHITE } else { C::TEXT_NORMAL }),
            )
            .on_press(Message::SetInputDevice("Default".into()))
            .width(Length::Fill)
            .padding(Padding::from([4, 8]))
            .style(if audio.input_device.is_none() { theme::channel_button_active } else { theme::channel_button }),
        );
        for dev in &audio.available_inputs {
            let is_selected = audio.input_device.as_ref() == Some(dev);
            let dev_name = dev.clone();
            input_col = input_col.push(
                button(
                    text(format!("{}  {}", if is_selected { ">" } else { " " }, dev))
                        .size(13)
                        .color(if is_selected { Color::WHITE } else { C::TEXT_NORMAL }),
                )
                .on_press(Message::SetInputDevice(dev_name))
                .width(Length::Fill)
                .padding(Padding::from([4, 8]))
                .style(if is_selected { theme::channel_button_active } else { theme::channel_button }),
            );
        }

        // Output device picker
        let mut output_col = Column::new().spacing(4);
        output_col = output_col.push(text("OUTPUT DEVICE").size(11).color(C::TEXT_MUTED));
        output_col = output_col.push(
            button(
                text(format!("{}  System Default", if audio.output_device.is_none() { ">" } else { " " }))
                    .size(13)
                    .color(if audio.output_device.is_none() { Color::WHITE } else { C::TEXT_NORMAL }),
            )
            .on_press(Message::SetOutputDevice("Default".into()))
            .width(Length::Fill)
            .padding(Padding::from([4, 8]))
            .style(if audio.output_device.is_none() { theme::channel_button_active } else { theme::channel_button }),
        );
        for dev in &audio.available_outputs {
            let is_selected = audio.output_device.as_ref() == Some(dev);
            let dev_name = dev.clone();
            output_col = output_col.push(
                button(
                    text(format!("{}  {}", if is_selected { ">" } else { " " }, dev))
                        .size(13)
                        .color(if is_selected { Color::WHITE } else { C::TEXT_NORMAL }),
                )
                .on_press(Message::SetOutputDevice(dev_name))
                .width(Length::Fill)
                .padding(Padding::from([4, 8]))
                .style(if is_selected { theme::channel_button_active } else { theme::channel_button }),
            );
        }

        // Volume sliders (using buttons as +/- since iced 0.13 slider API varies)
        let input_vol_pct = (audio.input_volume * 100.0) as u32;
        let output_vol_pct = (audio.output_volume * 100.0) as u32;

        let input_vol_row = row![
            text("Input Volume").size(13).color(C::TEXT_NORMAL),
            horizontal_space(),
            button(text("-").size(14)).on_press(Message::SetInputVolume((audio.input_volume - 0.1).max(0.0)))
                .style(theme::icon_button).padding(Padding::from([2, 8])),
            text(format!("{}%", input_vol_pct)).size(13).color(C::TEXT_NORMAL),
            button(text("+").size(14)).on_press(Message::SetInputVolume((audio.input_volume + 0.1).min(2.0)))
                .style(theme::icon_button).padding(Padding::from([2, 8])),
        ].align_y(iced::Alignment::Center).spacing(4);

        let output_vol_row = row![
            text("Output Volume").size(13).color(C::TEXT_NORMAL),
            horizontal_space(),
            button(text("-").size(14)).on_press(Message::SetOutputVolume((audio.output_volume - 0.1).max(0.0)))
                .style(theme::icon_button).padding(Padding::from([2, 8])),
            text(format!("{}%", output_vol_pct)).size(13).color(C::TEXT_NORMAL),
            button(text("+").size(14)).on_press(Message::SetOutputVolume((audio.output_volume + 0.1).min(2.0)))
                .style(theme::icon_button).padding(Padding::from([2, 8])),
        ].align_y(iced::Alignment::Center).spacing(4);

        // Noise suppression toggle
        let ns_row = row![
            text("Noise Suppression").size(13).color(C::TEXT_NORMAL),
            horizontal_space(),
            button(text(if audio.noise_suppression { "ON" } else { "OFF" }).size(12))
                .on_press(Message::ToggleNoiseSuppression)
                .style(if audio.noise_suppression { theme::modal_confirm } else { theme::icon_button })
                .padding(Padding::from([4, 12])),
        ].align_y(iced::Alignment::Center);

        // VAD threshold
        let vad_pct = (audio.vad_threshold * 100.0) as u32;
        let vad_row = row![
            text("Voice Activation").size(13).color(C::TEXT_NORMAL),
            horizontal_space(),
            button(text("-").size(14)).on_press(Message::SetVadThreshold((audio.vad_threshold - 0.05).max(0.0)))
                .style(theme::icon_button).padding(Padding::from([2, 8])),
            text(format!("{}%", vad_pct)).size(13).color(C::TEXT_NORMAL),
            button(text("+").size(14)).on_press(Message::SetVadThreshold((audio.vad_threshold + 0.05).min(1.0)))
                .style(theme::icon_button).padding(Padding::from([2, 8])),
        ].align_y(iced::Alignment::Center).spacing(4);

        // Mic test
        let mic_level_width = (audio.mic_level * 300.0) as f32;
        let mic_test_section = column![
            row![
                text("MIC TEST").size(11).color(C::TEXT_MUTED),
                horizontal_space(),
                button(
                    text(if audio.mic_testing { "Stop Test" } else { "Test Mic" }).size(12),
                )
                .on_press(if audio.mic_testing { Message::StopMicTest } else { Message::StartMicTest })
                .style(if audio.mic_testing { theme::voice_disconnect_button } else { theme::modal_confirm })
                .padding(Padding::from([4, 12])),
            ].align_y(iced::Alignment::Center),
            Space::with_height(4),
            // Level meter bar
            container(
                container(Space::new(mic_level_width, 8))
                    .style(move |_t: &Theme| container::Style {
                        background: Some(if mic_level_width > 200.0 {
                            C::RED
                        } else if mic_level_width > 100.0 {
                            C::YELLOW
                        } else {
                            C::GREEN
                        }.into()),
                        border: iced::Border { radius: 2.0.into(), ..Default::default() },
                        ..Default::default()
                    }),
            )
            .width(300)
            .style(|_t: &Theme| container::Style {
                background: Some(C::BG_DEEPEST.into()),
                border: iced::Border { radius: 3.0.into(), ..Default::default() },
                ..Default::default()
            }),
        ];

        let settings_content = column![
            text("Voice & Audio Settings").size(20).color(C::TEXT_BRIGHT),
            Space::with_height(12),
            scrollable(
                column![
                    addr_col,
                    Space::with_height(12),
                    horizontal_rule(1),
                    Space::with_height(12),
                    input_col,
                    Space::with_height(12),
                    output_col,
                    Space::with_height(16),
                    horizontal_rule(1),
                    Space::with_height(12),
                    input_vol_row,
                    Space::with_height(8),
                    output_vol_row,
                    Space::with_height(16),
                    horizontal_rule(1),
                    Space::with_height(12),
                    ns_row,
                    Space::with_height(8),
                    vad_row,
                    Space::with_height(16),
                    horizontal_rule(1),
                    Space::with_height(12),
                    mic_test_section,
                ]
                .spacing(2),
            )
            .height(400),
            Space::with_height(12),
            container(
                button(text("Close").size(14).color(C::TEXT_NORMAL))
                    .on_press(Message::CloseModal)
                    .style(theme::modal_cancel)
                    .padding(Padding::from([10, 24])),
            )
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
        ]
        .max_width(500);

        container(
            container(settings_content)
                .padding(Padding::from(24))
                .style(theme::modal_card),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Shrink)
        .center_y(Length::Shrink)
        .into()
    }

    fn handle_net_event(&mut self, event: NetEvent) {
        match event {
            NetEvent::MessageReceived(msg) => {
                self.state.add_message(msg);
            }
            NetEvent::DirectMessageReceived(msg) => {
                self.state.add_direct_message(msg);
            }
            NetEvent::PeerDiscovered { peer_id, name } => {
                self.state
                    .peers
                    .entry(peer_id.to_string())
                    .or_insert(UserProfile {
                        peer_id: peer_id.to_string(),
                        display_name: name,
                        status: UserStatus::Online,
                    });
            }
            NetEvent::PeerLeft(peer_id) => {
                if let Some(profile) = self.state.peers.get_mut(&peer_id.to_string()) {
                    profile.status = UserStatus::Offline;
                }
            }
            NetEvent::PresenceUpdate {
                peer_id,
                name,
                status,
            } => {
                let profile =
                    self.state
                        .peers
                        .entry(peer_id.clone())
                        .or_insert(UserProfile {
                            peer_id: peer_id.clone(),
                            display_name: name.clone(),
                            status,
                        });
                profile.display_name = name;
                profile.status = status;
            }
            NetEvent::ServerJoined {
                peer_id,
                name,
                server_id: _,
            } => {
                self.state
                    .peers
                    .entry(peer_id.clone())
                    .or_insert(UserProfile {
                        peer_id,
                        display_name: name,
                        status: UserStatus::Online,
                    });
            }
            NetEvent::ChannelCreated {
                server_id,
                channel,
            } => {
                self.state.add_channel_to_server(&server_id, channel);
            }
            NetEvent::Connected => {
                self.connected = true;
                for server in &self.state.servers {
                    for channel in &server.channels {
                        let topic = server.topic_for_channel(&channel.id);
                        let _ = self.net_cmd_tx.send(NetCommand::JoinServer(topic));
                    }
                }
            }
            NetEvent::ListeningOn(addr) => {
                if !self.state.listen_addrs.contains(&addr) {
                    self.state.listen_addrs.push(addr);
                }
            }
            NetEvent::Error(e) => {
                tracing::error!("Network error: {}", e);
            }
        }
    }

    fn subscribe_to_current_channel(&self) {
        if let Some(topic) = self.state.current_topic() {
            let _ = self.net_cmd_tx.send(NetCommand::JoinServer(topic));
        }
    }

    fn handle_command(&mut self, cmd: &str) -> IcedTask<Message> {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        match parts[0] {
            "/name" if parts.len() > 1 => {
                self.state.set_display_name(parts[1].to_string());
                let _ = self
                    .net_cmd_tx
                    .send(NetCommand::UpdatePresence(self.state.profile.status));
            }
            "/status" if parts.len() > 1 => {
                let status = match parts[1].to_lowercase().as_str() {
                    "online" => UserStatus::Online,
                    "away" => UserStatus::Away,
                    "dnd" => UserStatus::DoNotDisturb,
                    _ => UserStatus::Online,
                };
                self.state.profile.status = status;
                let _ = self.net_cmd_tx.send(NetCommand::UpdatePresence(status));
            }
            "/connect" if parts.len() > 1 => {
                if let Ok(addr) = parts[1].parse() {
                    let _ = self.net_cmd_tx.send(NetCommand::Dial(addr));
                }
            }
            _ => {}
        }
        self.state.input_buffer.clear();
        IcedTask::none()
    }
}
