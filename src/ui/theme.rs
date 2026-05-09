use egui::Color32;

// ── murmur monochrome palette ───────────────────────────────────────────────

pub const BG_DEEPEST: Color32 = Color32::from_rgb(8, 8, 8);
pub const BG_BASE: Color32 = Color32::from_rgb(12, 12, 12);
pub const BG_MAIN: Color32 = Color32::from_rgb(16, 16, 16);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(20, 20, 20);
pub const BG_SURFACE: Color32 = Color32::from_rgb(17, 17, 17);

pub const BORDER: Color32 = Color32::from_rgb(26, 26, 26);

pub const TEXT_BRIGHT: Color32 = Color32::from_rgb(224, 224, 224);
pub const TEXT_NORMAL: Color32 = Color32::from_rgb(187, 187, 187);
pub const TEXT_DIM: Color32 = Color32::from_rgb(136, 136, 136);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(85, 85, 85);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(51, 51, 51);

pub const GREEN: Color32 = Color32::from_rgb(34, 197, 94);
pub const YELLOW: Color32 = Color32::from_rgb(245, 158, 11);
pub const RED: Color32 = Color32::from_rgb(239, 68, 68);

pub const STATUS_ONLINE: Color32 = Color32::from_rgb(34, 197, 94);
pub const STATUS_IDLE: Color32 = Color32::from_rgb(245, 158, 11);
pub const STATUS_DND: Color32 = Color32::from_rgb(239, 68, 68);
pub const STATUS_OFFLINE: Color32 = Color32::from_rgb(51, 51, 51);

pub fn status_color(status: crate::types::UserStatus) -> Color32 {
    match status {
        crate::types::UserStatus::Online => STATUS_ONLINE,
        crate::types::UserStatus::Away => STATUS_IDLE,
        crate::types::UserStatus::DoNotDisturb => STATUS_DND,
        crate::types::UserStatus::Offline => STATUS_OFFLINE,
    }
}

/// Derive an avatar background color from a name.
pub fn avatar_color(name: &str) -> Color32 {
    let hash = name.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let tones: &[(u8, u8, u8)] = &[
        (24, 24, 20), (20, 24, 20), (24, 20, 19),
        (20, 20, 24), (24, 23, 20), (20, 24, 24),
    ];
    let (r, g, b) = tones[(hash as usize) % tones.len()];
    Color32::from_rgb(r, g, b)
}

/// Two-char initials from a name.
pub fn initials(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() >= 2 {
        format!("{}{}", chars[0].to_lowercase(), chars[1].to_lowercase())
    } else {
        chars.first().map(|c| c.to_lowercase().to_string()).unwrap_or_else(|| "?".into())
    }
}

/// Apply murmur's custom dark theme to egui.
pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_BASE;
    visuals.window_fill = BG_ELEVATED;
    visuals.extreme_bg_color = BG_DEEPEST;
    visuals.faint_bg_color = BG_SURFACE;
    visuals.widgets.noninteractive.bg_fill = BG_ELEVATED;
    visuals.widgets.inactive.bg_fill = BG_ELEVATED;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(30, 30, 30);
    visuals.widgets.active.bg_fill = Color32::from_rgb(35, 35, 35);
    visuals.selection.bg_fill = Color32::from_rgb(34, 80, 50);
    visuals.override_text_color = Some(TEXT_NORMAL);
    ctx.set_visuals(visuals);
}
