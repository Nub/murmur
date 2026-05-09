use iced::widget::{button, container, text_input};
use iced::{Border, Color, Shadow, Theme, Vector};

// ── murmur monochrome palette ─────────────────────────────────────────────
// Pure black/grey/white. Green is the only accent color (status/voice).

pub struct C;

impl C {
    // Backgrounds — near-black spectrum
    pub const BG_DEEPEST: Color = Color::from_rgb(0.031, 0.031, 0.031);  // #080808 topbar
    pub const BG_BASE: Color = Color::from_rgb(0.047, 0.047, 0.047);     // #0c0c0c sidebar/right
    pub const BG_MAIN: Color = Color::from_rgb(0.063, 0.063, 0.063);     // #101010 main area
    pub const BG_ELEVATED: Color = Color::from_rgb(0.078, 0.078, 0.078); // #141414 input/cards
    pub const BG_SURFACE: Color = Color::from_rgb(0.067, 0.067, 0.067);  // #111111 voice card
    pub const BG_FLOATING: Color = Color::from_rgb(0.055, 0.055, 0.055); // #0e0e0e modals

    // Borders — barely visible
    pub const BORDER: Color = Color::from_rgb(0.102, 0.102, 0.102);      // #1a1a1a
    pub const BORDER_HOVER: Color = Color::from_rgb(0.133, 0.133, 0.133);// #222222

    // Hover/active overlays
    pub const HOVER: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.02);
    pub const ACTIVE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.04);
    pub const SELECTED: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.06);

    // Text — grey scale
    pub const TEXT_BRIGHT: Color = Color::from_rgb(0.878, 0.878, 0.878); // #e0e0e0
    pub const TEXT_NORMAL: Color = Color::from_rgb(0.733, 0.733, 0.733); // #bbbbbb
    pub const TEXT_DIM: Color = Color::from_rgb(0.533, 0.533, 0.533);    // #888888
    pub const TEXT_MUTED: Color = Color::from_rgb(0.333, 0.333, 0.333);  // #555555
    pub const TEXT_FAINT: Color = Color::from_rgb(0.200, 0.200, 0.200);  // #333333
    pub const TEXT_GHOST: Color = Color::from_rgb(0.133, 0.133, 0.133);  // #222222
    pub const TEXT_INVISIBLE: Color = Color::from_rgb(0.100, 0.100, 0.100);// #1a1a1a

    // Accent — green only
    pub const GREEN: Color = Color::from_rgb(0.133, 0.773, 0.369);      // #22c55e
    pub const GREEN_DIM: Color = Color::from_rgba(0.133, 0.773, 0.369, 0.15);
    pub const YELLOW: Color = Color::from_rgb(0.961, 0.620, 0.043);     // #f59e0b
    pub const RED: Color = Color::from_rgb(0.937, 0.267, 0.267);        // #ef4444
    pub const RED_DIM: Color = Color::from_rgba(0.937, 0.267, 0.267, 0.10);

    // Status
    pub const STATUS_ONLINE: Color = Color::from_rgb(0.133, 0.773, 0.369);
    pub const STATUS_IDLE: Color = Color::from_rgb(0.961, 0.620, 0.043);
    pub const STATUS_DND: Color = Color::from_rgb(0.937, 0.267, 0.267);
    pub const STATUS_OFFLINE: Color = Color::from_rgb(0.200, 0.200, 0.200);
}

// ── Container styles ────────────────────────────────────────────────────────

pub fn topbar(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_DEEPEST.into()),
        border: Border { width: 1.0, radius: 0.0.into(), color: C::BORDER },
        ..Default::default()
    }
}

pub fn sidebar(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_BASE.into()),
        ..Default::default()
    }
}

pub fn sidebar_header(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_BASE.into()),
        border: Border { width: 1.0, radius: 0.0.into(), color: C::BORDER },
        ..Default::default()
    }
}

pub fn main_area(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_MAIN.into()),
        ..Default::default()
    }
}

pub fn main_header(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_MAIN.into()),
        border: Border { width: 1.0, radius: 0.0.into(), color: C::BORDER },
        ..Default::default()
    }
}

pub fn member_panel(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_BASE.into()),
        ..Default::default()
    }
}

pub fn voice_card(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_SURFACE.into()),
        border: Border { width: 1.0, radius: 8.0.into(), color: C::BORDER },
        ..Default::default()
    }
}

pub fn voice_controls(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb(0.039, 0.039, 0.039).into()),
        ..Default::default()
    }
}

pub fn modal_backdrop(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.65).into()),
        ..Default::default()
    }
}

pub fn modal_card(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_FLOATING.into()),
        border: Border { width: 1.0, radius: 12.0.into(), color: C::BORDER },
        shadow: Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.6), offset: Vector::new(0.0, 16.0), blur_radius: 48.0 },
        ..Default::default()
    }
}

pub fn tooltip_box(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_ELEVATED.into()),
        border: Border { width: 0.0, radius: 4.0.into(), color: Color::TRANSPARENT },
        shadow: Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.4), offset: Vector::new(0.0, 2.0), blur_radius: 8.0 },
        ..Default::default()
    }
}

pub fn context_menu(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(C::BG_FLOATING.into()),
        border: Border { width: 1.0, radius: 6.0.into(), color: C::BORDER },
        shadow: Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.5), offset: Vector::new(0.0, 4.0), blur_radius: 16.0 },
        ..Default::default()
    }
}

pub fn welcome_icon(_t: &Theme) -> container::Style {
    container::Style {
        background: None,
        ..Default::default()
    }
}

pub fn server_name_header(_t: &Theme) -> container::Style {
    sidebar_header(_t)
}

pub fn user_panel(_t: &Theme) -> container::Style {
    voice_controls(_t)
}

pub fn voice_connected(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgba(0.133, 0.773, 0.369, 0.03).into()),
        border: Border { width: 1.0, radius: 0.0.into(), color: Color::from_rgba(0.133, 0.773, 0.369, 0.1) },
        ..Default::default()
    }
}

// ── Button styles ───────────────────────────────────────────────────────────

pub fn server_icon(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgb(0.118, 0.118, 0.118).into()),
            text_color: C::TEXT_NORMAL,
            border: Border { width: 0.0, radius: 8.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
        _ => button::Style {
            background: Some(C::BG_ELEVATED.into()),
            text_color: C::TEXT_MUTED,
            border: Border { width: 0.0, radius: 10.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
    }
}

pub fn server_icon_active(_t: &Theme, _s: button::Status) -> button::Style {
    button::Style {
        background: Some(C::BG_ELEVATED.into()),
        text_color: C::TEXT_BRIGHT,
        border: Border { width: 1.5, radius: 8.0.into(), color: C::TEXT_FAINT },
        ..Default::default()
    }
}

pub fn add_server_icon(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgb(0.118, 0.118, 0.118).into()),
            text_color: C::TEXT_DIM,
            border: Border { width: 1.5, radius: 8.0.into(), color: C::BORDER_HOVER },
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: C::TEXT_FAINT,
            border: Border { width: 1.5, radius: 10.0.into(), color: C::BORDER },
            ..Default::default()
        },
    }
}

pub fn channel_button(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(C::HOVER.into()),
            text_color: C::TEXT_NORMAL,
            border: Border { width: 0.0, radius: 0.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: C::TEXT_MUTED,
            border: Border { width: 0.0, radius: 0.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
    }
}

pub fn channel_button_active(_t: &Theme, _s: button::Status) -> button::Style {
    button::Style {
        background: Some(C::ACTIVE.into()),
        text_color: C::TEXT_BRIGHT,
        border: Border { width: 0.0, radius: 0.0.into(), color: Color::TRANSPARENT },
        ..Default::default()
    }
}

pub fn member_button(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(C::HOVER.into()),
            text_color: C::TEXT_NORMAL,
            border: Border { width: 0.0, radius: 0.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: C::TEXT_DIM,
            border: Border { width: 0.0, radius: 0.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
    }
}

pub fn icon_button(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(C::HOVER.into()),
            text_color: C::TEXT_DIM,
            border: Border { width: 0.0, radius: 6.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: C::TEXT_FAINT,
            border: Border { width: 0.0, radius: 6.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
    }
}

pub fn voice_button(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: None,
            text_color: C::TEXT_NORMAL,
            border: Border { width: 1.0, radius: 4.0.into(), color: C::BORDER_HOVER },
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: C::TEXT_MUTED,
            border: Border { width: 1.0, radius: 4.0.into(), color: C::BORDER },
            ..Default::default()
        },
    }
}

pub fn voice_disconnect_button(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(C::RED_DIM.into()),
            text_color: C::RED,
            border: Border { width: 1.0, radius: 4.0.into(), color: Color::from_rgba(0.937, 0.267, 0.267, 0.3) },
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: Color::from_rgba(0.937, 0.267, 0.267, 0.6),
            border: Border { width: 1.0, radius: 4.0.into(), color: Color::from_rgba(0.937, 0.267, 0.267, 0.15) },
            ..Default::default()
        },
    }
}

pub fn modal_confirm(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(C::TEXT_BRIGHT.into()),
            text_color: C::BG_DEEPEST,
            border: Border { width: 0.0, radius: 6.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
        _ => button::Style {
            background: Some(C::TEXT_FAINT.into()),
            text_color: C::TEXT_BRIGHT,
            border: Border { width: 0.0, radius: 6.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
    }
}

pub fn modal_cancel(_t: &Theme, status: button::Status) -> button::Style {
    icon_button(_t, status)
}

pub fn user_panel_button(_t: &Theme, status: button::Status) -> button::Style {
    icon_button(_t, status)
}

pub fn status_menu_button(_t: &Theme, status: button::Status) -> button::Style {
    icon_button(_t, status)
}

pub fn context_menu_item(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(C::HOVER.into()),
            text_color: C::TEXT_BRIGHT,
            border: Border { width: 0.0, radius: 4.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: C::TEXT_NORMAL,
            border: Border { width: 0.0, radius: 4.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
    }
}

pub fn context_menu_item_danger(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(C::RED_DIM.into()),
            text_color: C::RED,
            border: Border { width: 0.0, radius: 4.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: C::RED,
            border: Border { width: 0.0, radius: 4.0.into(), color: Color::TRANSPARENT },
            ..Default::default()
        },
    }
}

// ── Text input ──────────────────────────────────────────────────────────────

pub fn chat_input(_t: &Theme, _s: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Color::TRANSPARENT.into(),
        border: Border::default(),
        icon: C::TEXT_FAINT,
        placeholder: C::TEXT_GHOST,
        value: C::TEXT_NORMAL,
        selection: C::GREEN,
    }
}

pub fn modal_input(_t: &Theme, _s: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: C::BG_ELEVATED.into(),
        border: Border { width: 1.0, radius: 6.0.into(), color: C::BORDER },
        icon: C::TEXT_FAINT,
        placeholder: C::TEXT_GHOST,
        value: C::TEXT_NORMAL,
        selection: C::GREEN,
    }
}
