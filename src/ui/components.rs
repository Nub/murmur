use super::theme::C;
use super::Message;
use crate::types::*;
use iced::widget::{button, column, container, row, text, tooltip, Column, Space};
use iced::{Color, Element, Length, Padding};

/// 4-value padding helper.
pub fn pad4(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding { top, right, bottom, left }
}

// ── Avatar (squared, monospace 2-char initials) ─────────────────────────────

pub fn avatar<'a>(name: &str, size: f32, status: Option<UserStatus>) -> Element<'a, Message> {
    let chars: Vec<char> = name.chars().collect();
    let initials = if chars.len() >= 2 {
        format!("{}{}", chars[0].to_lowercase(), chars[1].to_lowercase())
    } else {
        chars.first().map(|c| c.to_lowercase().to_string()).unwrap_or_else(|| "?".into())
    };

    // Muted earth tones derived from name hash
    let hash = name.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let tones = [
        (0.094, 0.094, 0.078), // stone-warm
        (0.078, 0.094, 0.078), // moss
        (0.094, 0.078, 0.074), // rust
        (0.078, 0.078, 0.094), // slate
        (0.094, 0.090, 0.078), // sand
        (0.078, 0.094, 0.094), // teal
    ];
    let (r, g, b) = tones[(hash as usize) % tones.len()];
    let text_tones = [
        (0.416, 0.416, 0.290), // stone-warm text
        (0.290, 0.416, 0.290), // moss text
        (0.416, 0.290, 0.250), // rust text
        (0.290, 0.290, 0.416), // slate text
        (0.416, 0.380, 0.290), // sand text
        (0.290, 0.416, 0.416), // teal text
    ];
    let (tr, tg, tb) = text_tones[(hash as usize) % text_tones.len()];
    let bg = Color::from_rgb(r, g, b);
    let fg = Color::from_rgb(tr, tg, tb);

    let radius = size * 0.22; // rounded square, not circle
    let font_size = size * 0.35;

    let av = container(
        text(initials).size(font_size).color(fg),
    )
    .width(size)
    .height(size)
    .center_x(size)
    .center_y(size)
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(bg.into()),
        border: iced::Border { width: 0.0, radius: radius.into(), color: Color::TRANSPARENT },
        ..Default::default()
    });

    if let Some(user_status) = status {
        let dot_color = match user_status {
            UserStatus::Online => C::STATUS_ONLINE,
            UserStatus::Away => C::STATUS_IDLE,
            UserStatus::DoNotDisturb => C::STATUS_DND,
            UserStatus::Offline => C::STATUS_OFFLINE,
        };
        let dot_size = size * 0.28;
        let dot = container(Space::new(0, 0))
            .width(dot_size)
            .height(dot_size)
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(dot_color.into()),
                border: iced::Border { width: 1.5, radius: (dot_size / 2.0).into(), color: C::BG_BASE },
                ..Default::default()
            });

        iced::widget::stack![
            container(av).width(size + 4.0).height(size + 4.0),
            container(dot)
                .width(size + 4.0).height(size + 4.0)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Bottom),
        ].into()
    } else {
        av.into()
    }
}

// ── Server icon (horizontal topbar style) ───────────────────────────────────

pub fn server_icon_widget<'a>(
    name: &str, index: usize, is_active: bool, _has_unread: bool,
) -> Element<'a, Message> {
    let chars: Vec<char> = name.chars().collect();
    let initials = if chars.len() >= 2 {
        format!("{}{}", chars[0].to_lowercase(), chars[1].to_lowercase())
    } else {
        chars.first().map(|c| c.to_lowercase().to_string()).unwrap_or_else(|| "?".into())
    };

    let btn = button(
        container(text(initials).size(13).color(if is_active { C::TEXT_BRIGHT } else { C::TEXT_MUTED }))
            .width(36).height(36).center_x(36).center_y(36),
    )
    .on_press(Message::SelectServer(index))
    .style(if is_active { super::theme::server_icon_active } else { super::theme::server_icon });

    tooltip(
        btn,
        container(text(name.to_string()).size(11).color(C::TEXT_DIM))
            .padding(Padding::from([4, 8]))
            .style(super::theme::tooltip_box),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .into()
}

// ── Channel item ────────────────────────────────────────────────────────────

pub fn channel_item<'a>(
    name: &str, index: usize, is_active: bool, unread_count: u32,
) -> Element<'a, Message> {
    let prefix_color = if is_active { C::TEXT_MUTED } else { C::TEXT_FAINT };
    let name_color = if is_active { C::TEXT_BRIGHT } else { C::TEXT_MUTED };

    let mut content = row![
        text("#").size(12).color(prefix_color),
        Space::with_width(5),
        text(name.to_string()).size(13).color(name_color),
    ].align_y(iced::Alignment::Center);

    if unread_count > 0 {
        content = content.push(iced::widget::horizontal_space());
        content = content.push(
            container(Space::new(5, 5))
                .style(|_t: &iced::Theme| container::Style {
                    background: Some(C::TEXT_BRIGHT.into()),
                    border: iced::Border { radius: 3.0.into(), ..Default::default() },
                    ..Default::default()
                }),
        );
    }

    button(content)
        .on_press(Message::SelectChannel(index))
        .width(Length::Fill)
        .padding(Padding::from([4, 14]))
        .style(if is_active { super::theme::channel_button_active } else { super::theme::channel_button })
        .into()
}

// ── Section header ──────────────────────────────────────────────────────────

pub fn section_header<'a>(label: &str, action: Option<Message>) -> Element<'a, Message> {
    let mut header = row![
        text(label.to_string()).size(10).color(C::TEXT_FAINT),
        iced::widget::horizontal_space(),
    ].align_y(iced::Alignment::Center);

    if let Some(msg) = action {
        header = header.push(
            button(text("+").size(12).color(C::TEXT_FAINT))
                .on_press(msg).style(super::theme::icon_button).padding(Padding::from([0, 4])),
        );
    }

    container(header).padding(pad4(12.0, 14.0, 4.0, 14.0)).width(Length::Fill).into()
}

// ── Message widget ──────────────────────────────────────────────────────────

pub fn message_widget<'a>(
    msg: &ChatMessage, is_continuation: bool, _show_hover_actions: bool,
) -> Element<'a, Message> {
    let timestamp = msg.timestamp.format("%H:%M").to_string();
    let sender = msg.sender_name.clone();
    let content = msg.content.clone();
    let edited = msg.edited;
    let has_reply = msg.reply_to.is_some();

    if is_continuation {
        container(
            row![
                container(Space::new(0, 0)).width(34),
                Space::with_width(10),
                text(content).size(13).color(C::TEXT_DIM),
            ]
        )
        .padding(pad4(1.0, 24.0, 1.0, 24.0))
        .width(Length::Fill)
        .into()
    } else {
        let mut header_row = row![
            text(sender).size(12).color(C::TEXT_NORMAL),
            Space::with_width(6),
            text(timestamp).size(9).color(C::TEXT_FAINT),
            Space::with_width(4),
            text("e2e").size(8).color(C::TEXT_GHOST),
        ].align_y(iced::Alignment::Center);

        if edited {
            header_row = header_row.push(Space::with_width(4));
            header_row = header_row.push(text("(edited)").size(9).color(C::TEXT_FAINT));
        }

        let mut msg_col = Column::new().spacing(3);

        // Show reply context if present
        if has_reply {
            msg_col = msg_col.push(
                container(
                    text("replying to a message").size(10).color(C::TEXT_FAINT),
                )
                .padding(pad4(2.0, 8.0, 2.0, 8.0))
                .style(|_t: &iced::Theme| container::Style {
                    background: Some(C::BG_ELEVATED.into()),
                    border: iced::Border { radius: 3.0.into(), ..Default::default() },
                    ..Default::default()
                }),
            );
        }

        msg_col = msg_col.push(header_row);
        msg_col = msg_col.push(text(content).size(13).color(C::TEXT_DIM));

        // Show reactions
        if !msg.reactions.is_empty() {
            let mut reaction_row = iced::widget::Row::new().spacing(4);
            for (emoji, peers) in &msg.reactions {
                let count = peers.len();
                let label = format!("{} {}", emoji, count);
                let msg_id = msg.id.clone();
                let emoji_clone = emoji.clone();
                reaction_row = reaction_row.push(
                    button(text(label).size(10).color(C::TEXT_DIM))
                        .on_press(Message::ReactToMessage(msg_id, emoji_clone))
                        .padding(Padding::from([2, 6]))
                        .style(|_t: &iced::Theme, status: button::Status| button::Style {
                            background: Some(match status {
                                button::Status::Hovered => Color::from_rgb(0.12, 0.12, 0.12),
                                _ => C::BG_ELEVATED,
                            }.into()),
                            text_color: C::TEXT_DIM,
                            border: iced::Border { radius: 4.0.into(), ..Default::default() },
                            ..Default::default()
                        }),
                );
            }
            msg_col = msg_col.push(reaction_row);
        }

        container(
            row![
                avatar(&msg.sender_name, 34.0, None),
                Space::with_width(10),
                msg_col,
            ].align_y(iced::Alignment::Start),
        )
        .padding(pad4(5.0, 24.0, 5.0, 24.0))
        .width(Length::Fill)
        .into()
    }
}

pub fn dm_widget<'a>(msg: &DirectMessage, is_continuation: bool) -> Element<'a, Message> {
    let timestamp = msg.timestamp.format("%H:%M").to_string();
    let from_name = msg.from_name.clone();
    let content = msg.content.clone();

    if is_continuation {
        container(
            row![
                container(Space::new(0, 0)).width(34),
                Space::with_width(10),
                text(content).size(13).color(C::TEXT_DIM),
            ]
        )
        .padding(pad4(1.0, 24.0, 1.0, 24.0))
        .width(Length::Fill)
        .into()
    } else {
        container(
            row![
                avatar(&from_name, 34.0, None),
                Space::with_width(10),
                column![
                    row![
                        text(from_name).size(12).color(C::TEXT_NORMAL),
                        Space::with_width(6),
                        text(timestamp).size(9).color(C::TEXT_FAINT),
                    ].align_y(iced::Alignment::Center),
                    text(content).size(13).color(C::TEXT_DIM),
                ].spacing(3),
            ].align_y(iced::Alignment::Start),
        )
        .padding(pad4(5.0, 24.0, 5.0, 24.0))
        .width(Length::Fill)
        .into()
    }
}

// ── Member entry ────────────────────────────────────────────────────────────

pub fn member_entry<'a>(peer_id: &str, profile: &UserProfile) -> Element<'a, Message> {
    let name = profile.display_name.clone();
    let pid = peer_id.to_string();

    button(
        row![
            avatar(&name, 26.0, Some(profile.status)),
            Space::with_width(7),
            column![
                text(name).size(11).color(C::TEXT_DIM),
                text(format!("{}", profile.status)).size(9).color(C::TEXT_FAINT),
            ].spacing(1),
        ].align_y(iced::Alignment::Center),
    )
    .on_press(Message::SelectDM(pid))
    .width(Length::Fill)
    .padding(Padding::from([4, 12]))
    .style(super::theme::member_button)
    .into()
}

// ── Context menu ────────────────────────────────────────────────────────────

pub fn context_menu_widget<'a>(items: Vec<ContextMenuItem>) -> Element<'a, Message> {
    let mut col = Column::new().spacing(2).padding(Padding::from([6, 8]));
    for item in items {
        match item {
            ContextMenuItem::Action { label, message, danger } => {
                col = col.push(
                    button(text(label).size(12))
                        .on_press(message).width(Length::Fill).padding(Padding::from([5, 8]))
                        .style(if danger { super::theme::context_menu_item_danger } else { super::theme::context_menu_item }),
                );
            }
            ContextMenuItem::Separator => {
                col = col.push(
                    container(Space::new(Length::Fill, 1))
                        .style(|_t: &iced::Theme| container::Style {
                            background: Some(C::BORDER.into()), ..Default::default()
                        })
                        .padding(Padding::from([4, 0])),
                );
            }
        }
    }
    container(col).width(180).style(super::theme::context_menu).into()
}

pub enum ContextMenuItem {
    Action { label: String, message: Message, danger: bool },
    Separator,
}

// ── Typing indicator ────────────────────────────────────────────────────────

pub fn typing_indicator<'a>(names: &[String], tick: u64) -> Element<'a, Message> {
    if names.is_empty() { return Space::new(0, 0).into(); }
    let dots: String = ".".repeat(((tick / 8) % 4) as usize);
    let who = if names.len() == 1 {
        format!("{} is typing{}", names[0], dots)
    } else {
        format!("several people are typing{}", dots)
    };
    container(text(who).size(10).color(C::TEXT_FAINT))
        .padding(Padding::from([2, 24])).width(Length::Fill).into()
}

// ── Connection badge ────────────────────────────────────────────────────────

pub fn connection_badge<'a>(connected: bool) -> Element<'a, Message> {
    let (label, color) = if connected { ("p2p", C::GREEN) } else { ("connecting", C::YELLOW) };
    container(
        row![
            container(Space::new(0, 0)).width(5).height(5)
                .style(move |_t: &iced::Theme| container::Style {
                    background: Some(color.into()),
                    border: iced::Border { radius: 3.0.into(), ..Default::default() },
                    ..Default::default()
                }),
            Space::with_width(4),
            text(label).size(10).color(C::TEXT_FAINT),
        ].align_y(iced::Alignment::Center),
    )
    .padding(Padding::from([3, 8]))
    .style(|_t: &iced::Theme| container::Style {
        background: Some(C::BG_ELEVATED.into()),
        border: iced::Border { radius: 4.0.into(), ..Default::default() },
        ..Default::default()
    })
    .into()
}

// ── Date separator ──────────────────────────────────────────────────────────

pub fn date_separator<'a>(label: &str) -> Element<'a, Message> {
    container(
        row![
            container(Space::new(Length::Fill, 1))
                .style(|_t: &iced::Theme| container::Style {
                    background: Some(C::BORDER.into()), ..Default::default()
                }),
            container(text(label.to_string()).size(9).color(C::TEXT_FAINT))
                .padding(Padding::from([0, 12])),
            container(Space::new(Length::Fill, 1))
                .style(|_t: &iced::Theme| container::Style {
                    background: Some(C::BORDER.into()), ..Default::default()
                }),
        ].align_y(iced::Alignment::Center),
    )
    .padding(Padding::from([6, 24]))
    .width(Length::Fill)
    .into()
}
