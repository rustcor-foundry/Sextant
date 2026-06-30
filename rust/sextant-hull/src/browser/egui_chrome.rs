//! Shared egui chrome widgets and theme dispatch — tab chips, view chips,
//! metric cards, and helpers used by hosted-direct and the egui bridge shell.

use super::*;

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn apply_chrome_ui_theme(ctx: &egui::Context, theme: ChromeUiTheme) {
    match theme {
        ChromeUiTheme::Modern => apply_modern_egui_theme(ctx),
        ChromeUiTheme::Retro => apply_retro_egui_theme(ctx),
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn egui_chip_fill(active: bool) -> egui::Color32 {
    if active {
        egui::Color32::from_rgb(38, 46, 62)
    } else {
        egui::Color32::from_rgb(24, 28, 38)
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn egui_chip_stroke(active: bool) -> egui::Stroke {
    if active {
        egui::Stroke::new(1.0, egui::Color32::from_rgb(88, 118, 168))
    } else {
        egui::Stroke::NONE
    }
}

/// Compact selectable chip for view tabs, mode pills, presets, and settings toggles.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn egui_chip(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    let mut clicked = false;
    egui::Frame::none()
        .fill(egui_chip_fill(active))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .stroke(egui_chip_stroke(active))
        .show(ui, |ui| {
            let response = ui.add(
                egui::Label::new(label).sense(egui::Sense::click()),
            );
            clicked = response.clicked();
        });
    clicked
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub struct EguiTabChipResponse {
    pub clicked: bool,
    pub close_clicked: bool,
}

/// One browser tab chip: colored favicon initial, truncated site label, optional
/// inline close. Shared by hosted-direct and the egui bridge shell.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn egui_tab_chip(
    ui: &mut egui::Ui,
    site: &str,
    url: &str,
    active: bool,
    show_close: bool,
) -> EguiTabChipResponse {
    let initial = site.chars().next().unwrap_or('•').to_ascii_uppercase();
    let (r, g, b) = hosted_direct_tab_favicon_rgb(url);
    let mut clicked = false;
    let mut close_clicked = false;
    egui::Frame::none()
        .fill(egui_chip_fill(active))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(6.0, 3.0))
        .stroke(egui_chip_stroke(active))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(r, g, b))
                    .rounding(4.0)
                    .inner_margin(egui::Margin::symmetric(4.0, 2.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(initial.to_string())
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                    });
                let label = ui
                    .add(egui::Label::new(truncate_label(site, 16)).sense(egui::Sense::click()))
                    .on_hover_text(url);
                clicked |= label.clicked();
                if show_close
                    && ui
                        .add(
                            egui::Button::new(egui::RichText::new("×").size(14.0))
                                .frame(false)
                                .min_size(egui::vec2(16.0, 16.0)),
                        )
                        .on_hover_text("Close tab")
                        .clicked()
                {
                    close_clicked = true;
                }
            });
        });
    EguiTabChipResponse {
        clicked,
        close_clicked,
    }
}

/// Rounded summary card for bridge dashboard metrics.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn egui_metric_card(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    sub: &str,
    value_color: egui::Color32,
) {
    let dim = egui::Color32::from_rgb(0x93, 0xa4, 0xb0);
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(24, 28, 38))
        .rounding(8.0)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(0x31, 0x3d, 0x48)))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(140.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(label).color(dim).small());
                ui.label(
                    egui::RichText::new(value)
                        .color(value_color)
                        .size(19.0)
                        .strong(),
                );
                ui.label(egui::RichText::new(sub).color(dim).small());
            });
        });
}

/// Rounded inset panel for rail/section body copy.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn egui_panel_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(24, 28, 38))
        .rounding(6.0)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(0x31, 0x3d, 0x48)))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, add_contents)
        .inner
}

/// Muted section heading used in the bridge rail and settings panels.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn egui_section_heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .color(egui::Color32::from_rgb(0x93, 0xa4, 0xb0))
            .small(),
    );
}
