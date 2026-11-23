//! Cyberpunk Dark HUD theme for TrophyVault Desktop.
//! Provides color palette, widget styling, and visual helpers.

use eframe::egui::{self, Color32, Margin, Rounding, Stroke, Visuals};

// --- Color Palette ---
pub const COLOR_BG_OBSIDIAN: Color32 = Color32::from_rgb(0x0c, 0x10, 0x17);     // #0c1017 Obsidian background
pub const COLOR_PANEL_BG: Color32 = Color32::from_rgb(0x12, 0x18, 0x24);        // #121824 Card / Panel background
pub const COLOR_CARD_BG: Color32 = Color32::from_rgb(0x16, 0x1f, 0x30);         // #161f30 Slightly elevated card background
pub const COLOR_BORDER_SLATE: Color32 = Color32::from_rgb(0x1e, 0x29, 0x3b);    // #1e293b Slate border
pub const COLOR_ACCENT_CYAN: Color32 = Color32::from_rgb(0x00, 0xf3, 0xff);     // #00f3ff Neon cyan accent
pub const COLOR_ACCENT_GOLD: Color32 = Color32::from_rgb(0xff, 0xc7, 0x00);     // #ffc700 Gold accent
pub const COLOR_TEXT_PRIMARY: Color32 = Color32::from_rgb(0xf1, 0xf5, 0xf9);    // #f1f5f9 Primary text
pub const COLOR_TEXT_SECONDARY: Color32 = Color32::from_rgb(0x94, 0xa3, 0xb8);  // #94a3b8 Secondary text
pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(0x64, 0x74, 0x8b);      // #64748b Muted text
pub const COLOR_SUCCESS_GREEN: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);   // #10b981 Green success
pub const COLOR_ERROR_RED: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);       // #ef4444 Red error

// Pill / Glow active highlight fills
pub const COLOR_CYAN_PILL_BG: Color32 = Color32::from_rgba_premultiplied(0, 243, 255, 28);
#[allow(dead_code)]
pub const COLOR_CYAN_GLOW: Color32 = Color32::from_rgba_premultiplied(0, 243, 255, 60);

/// Generates the complete Cyberpunk Dark HUD visuals for egui.
pub fn cyberpunk_visuals() -> Visuals {
    let mut visuals = Visuals::dark();

    // Canvas & panel fills
    visuals.panel_fill = COLOR_BG_OBSIDIAN;
    visuals.window_fill = COLOR_PANEL_BG;
    visuals.faint_bg_color = COLOR_BG_OBSIDIAN;
    visuals.extreme_bg_color = COLOR_BG_OBSIDIAN;
    visuals.code_bg_color = COLOR_BG_OBSIDIAN;

    // Text & status colors
    visuals.override_text_color = Some(COLOR_TEXT_PRIMARY);
    visuals.warn_fg_color = COLOR_ACCENT_GOLD;
    visuals.error_fg_color = COLOR_ERROR_RED;

    // Window & menu rounding
    visuals.window_rounding = Rounding::same(12.0_f32);
    visuals.window_stroke = Stroke::new(1.0_f32, COLOR_BORDER_SLATE);
    visuals.menu_rounding = Rounding::same(8.0_f32);

    let default_rounding = Rounding::same(8.0_f32);

    // Noninteractive widgets
    visuals.widgets.noninteractive.bg_fill = COLOR_PANEL_BG;
    visuals.widgets.noninteractive.weak_bg_fill = COLOR_BG_OBSIDIAN;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER_SLATE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, COLOR_TEXT_SECONDARY);
    visuals.widgets.noninteractive.rounding = default_rounding;

    // Inactive (idle buttons, inputs)
    visuals.widgets.inactive.bg_fill = COLOR_PANEL_BG;
    visuals.widgets.inactive.weak_bg_fill = COLOR_PANEL_BG;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER_SLATE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, COLOR_TEXT_PRIMARY);
    visuals.widgets.inactive.rounding = default_rounding;

    // Hovered widgets
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x1a, 0x24, 0x36);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x1a, 0x24, 0x36);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, COLOR_ACCENT_CYAN);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, COLOR_TEXT_PRIMARY);
    visuals.widgets.hovered.rounding = default_rounding;

    // Active (clicked/pressed/selected widgets)
    visuals.widgets.active.bg_fill = Color32::from_rgb(0x14, 0x2e, 0x48);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x14, 0x2e, 0x48);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, COLOR_ACCENT_CYAN);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, COLOR_ACCENT_CYAN);
    visuals.widgets.active.rounding = default_rounding;

    // Open (e.g. ComboBox popup)
    visuals.widgets.open.bg_fill = COLOR_PANEL_BG;
    visuals.widgets.open.weak_bg_fill = COLOR_PANEL_BG;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, COLOR_ACCENT_CYAN);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, COLOR_TEXT_PRIMARY);
    visuals.widgets.open.rounding = default_rounding;

    // Selection highlight
    visuals.selection.bg_fill = COLOR_CYAN_PILL_BG;
    visuals.selection.stroke = Stroke::new(1.0_f32, COLOR_ACCENT_CYAN);

    visuals
}

/// Applies the Cyberpunk Dark HUD theme to the egui Context.
pub fn apply_theme(ctx: &egui::Context) {
    ctx.set_visuals(cyberpunk_visuals());
}

/// Reusable card container frame with 10px rounding and slate border.
pub fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(COLOR_CARD_BG)
        .stroke(Stroke::new(1.0_f32, COLOR_BORDER_SLATE))
        .rounding(Rounding::same(10.0_f32))
        .inner_margin(Margin::same(12.0_f32))
}

/// Left sidebar container frame with obsidian fill and slate border right.
pub fn sidebar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(COLOR_PANEL_BG)
        .stroke(Stroke::new(1.0_f32, COLOR_BORDER_SLATE))
        .inner_margin(Margin::symmetric(14.0_f32, 16.0_f32))
}

/// Active nav button pill frame with neon cyan accent border and subtle glow.
#[allow(dead_code)]
pub fn active_nav_pill_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(COLOR_CYAN_PILL_BG)
        .stroke(Stroke::new(1.0_f32, COLOR_ACCENT_CYAN))
        .rounding(Rounding::same(8.0_f32))
        .inner_margin(Margin::symmetric(12.0_f32, 8.0_f32))
}

/// Inactive nav button frame.
#[allow(dead_code)]
pub fn inactive_nav_pill_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .rounding(Rounding::same(8.0_f32))
        .inner_margin(Margin::symmetric(12.0_f32, 8.0_f32))
}
