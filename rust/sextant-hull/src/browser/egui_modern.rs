//! Modern egui theme — the default Sextant chrome look shared by hosted-direct
//! and the egui bridge shell. Rounded controls, proportional type, and the same
//! dark palette as the direct-mode tab strip.

/// Apply the modern Sextant chrome theme: dark panels, soft borders, rounded
/// widgets, and proportional typography (the hosted-direct look).
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn apply_modern_egui_theme(ctx: &egui::Context) {
    use egui::{Color32, FontFamily::Proportional, FontId, Rounding, Stroke, TextStyle, Visuals};

    let bg = Color32::from_rgb(0x10, 0x16, 0x1d);
    let panel = Color32::from_rgb(0x18, 0x1c, 0x26);
    let panel_alt = Color32::from_rgb(0x24, 0x28, 0x32);
    let field = Color32::from_rgb(0x0d, 0x13, 0x19);
    let accent = Color32::from_rgb(0x58, 0x76, 0xa8);
    let cyan = Color32::from_rgb(0x0e, 0xc7, 0xe8);
    let green = Color32::from_rgb(0x2f, 0xbf, 0x71);
    let text = Color32::from_rgb(0xdc, 0xe7, 0xef);
    let text_dim = Color32::from_rgb(0x93, 0xa4, 0xb0);
    let border = Color32::from_rgb(0x31, 0x3d, 0x48);
    let round = Rounding::same(6.0);

    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(text);
    visuals.hyperlink_color = cyan;
    visuals.panel_fill = bg;
    visuals.window_fill = panel;
    visuals.window_stroke = Stroke::new(1.0, border);
    visuals.window_rounding = round;
    visuals.menu_rounding = round;
    visuals.extreme_bg_color = field;
    visuals.faint_bg_color = panel;
    visuals.selection.bg_fill = accent.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, accent);

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = panel;
    w.noninteractive.weak_bg_fill = panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, text_dim);
    w.noninteractive.rounding = round;
    w.inactive.bg_fill = panel_alt;
    w.inactive.weak_bg_fill = panel_alt;
    w.inactive.bg_stroke = Stroke::new(1.0, border);
    w.inactive.fg_stroke = Stroke::new(1.0, text);
    w.inactive.rounding = round;
    w.hovered.bg_fill = Color32::from_rgb(0x38, 0x46, 0x62);
    w.hovered.weak_bg_fill = Color32::from_rgb(0x38, 0x46, 0x62);
    w.hovered.bg_stroke = Stroke::new(1.0, accent);
    w.hovered.fg_stroke = Stroke::new(1.0, text);
    w.hovered.rounding = round;
    w.active.bg_fill = green;
    w.active.weak_bg_fill = green;
    w.active.bg_stroke = Stroke::new(1.0, cyan);
    w.active.fg_stroke = Stroke::new(1.0, bg);
    w.active.rounding = round;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::new(18.0, Proportional)),
        (TextStyle::Body, FontId::new(13.5, Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, Proportional)),
        (TextStyle::Button, FontId::new(13.0, Proportional)),
        (TextStyle::Small, FontId::new(11.5, Proportional)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    ctx.set_style(style);
}
