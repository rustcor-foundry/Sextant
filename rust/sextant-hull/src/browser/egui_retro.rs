//! Retro egui theme + proof — `apply_retro_egui_theme` (the shared Sextant
//! retro-HUD theme: navy bg, cyan edges, green/amber status, AA monospace) and
//! the standalone egui-render proof (`run_retro_egui_proof`). Extracted from
//! the browser.rs crate root; gated once at the `mod` declaration on
//! `all(windows, servo-backend)`. The theme is re-exported from the crate root
//! so the xilem egui_bridge can share it.

use super::*;

/// Apply the Sextant retro-HUD theme to an egui context: the bridge-shell dark
/// palette (navy bg, cyan edges, green/amber status), sharp corners, and a crisp
/// anti-aliased monospace everywhere. This is the "high-resolution retro" proof:
/// same terminal feel as the softbuffer bridge shell, rendered AA + HiDPI.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn apply_retro_egui_theme(ctx: &egui::Context) {
    use egui::{Color32, FontFamily::Monospace, FontId, Rounding, Stroke, TextStyle, Visuals};

    let bg = Color32::from_rgb(0x10, 0x16, 0x1d);
    let panel = Color32::from_rgb(0x19, 0x23, 0x2c);
    let panel_alt = Color32::from_rgb(0x20, 0x2b, 0x35);
    let field = Color32::from_rgb(0x0d, 0x13, 0x19);
    let cyan = Color32::from_rgb(0x0e, 0xc7, 0xe8);
    let cyan_dim = Color32::from_rgb(0x2a, 0x6b, 0x7d);
    let green = Color32::from_rgb(0x2f, 0xbf, 0x71);
    let text = Color32::from_rgb(0xdc, 0xe7, 0xef);
    let text_dim = Color32::from_rgb(0x93, 0xa4, 0xb0);
    let border = Color32::from_rgb(0x31, 0x3d, 0x48);
    let sharp = Rounding::ZERO;

    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(text);
    visuals.hyperlink_color = cyan;
    visuals.panel_fill = bg;
    visuals.window_fill = panel;
    visuals.window_stroke = Stroke::new(1.0, cyan_dim);
    visuals.window_rounding = sharp;
    visuals.menu_rounding = sharp;
    visuals.extreme_bg_color = field;
    visuals.faint_bg_color = panel;
    visuals.selection.bg_fill = cyan.linear_multiply(0.25);
    visuals.selection.stroke = Stroke::new(1.0, cyan);

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = panel;
    w.noninteractive.weak_bg_fill = panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, cyan_dim);
    w.noninteractive.fg_stroke = Stroke::new(1.0, text_dim);
    w.noninteractive.rounding = sharp;
    w.inactive.bg_fill = panel_alt;
    w.inactive.weak_bg_fill = panel_alt;
    w.inactive.bg_stroke = Stroke::new(1.0, border);
    w.inactive.fg_stroke = Stroke::new(1.0, text);
    w.inactive.rounding = sharp;
    w.hovered.bg_fill = Color32::from_rgb(0x4a, 0x74, 0x88);
    w.hovered.weak_bg_fill = Color32::from_rgb(0x4a, 0x74, 0x88);
    w.hovered.bg_stroke = Stroke::new(1.0, cyan);
    w.hovered.fg_stroke = Stroke::new(1.0, text);
    w.hovered.rounding = sharp;
    w.active.bg_fill = green;
    w.active.weak_bg_fill = green;
    w.active.bg_stroke = Stroke::new(1.0, cyan);
    w.active.fg_stroke = Stroke::new(1.0, bg);
    w.active.rounding = sharp;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::new(19.0, Monospace)),
        (TextStyle::Body, FontId::new(13.0, Monospace)),
        (TextStyle::Monospace, FontId::new(13.0, Monospace)),
        (TextStyle::Button, FontId::new(13.0, Monospace)),
        (TextStyle::Small, FontId::new(11.0, Monospace)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_style(style);
}

/// Sample HUD for the retro-egui proof: mirrors the bridge-shell layout (top bar +
/// mode chips + intent field, metric cards, content panel, right AI rail, status
/// bar) so the look can be judged 1:1 against the softbuffer shell.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn draw_retro_egui_proof(ctx: &egui::Context, intent: &mut String, zoom: &mut f32) {
    use egui::{Color32, RichText};
    let cyan = Color32::from_rgb(0x0e, 0xc7, 0xe8);
    let green = Color32::from_rgb(0x2f, 0xbf, 0x71);
    let dim = Color32::from_rgb(0x93, 0xa4, 0xb0);

    egui::TopBottomPanel::top("retro_top").show(ctx, |ui| {
        ui.add_space(4.0);
        // Row 1: branding + mode chips, HiDPI toggle on the right.
        ui.horizontal(|ui| {
            ui.label(RichText::new("SEXTANT").color(cyan).strong());
            ui.label(RichText::new("AI BROWSER").color(dim).small());
            ui.separator();
            for chip in ["AGENT", "ASSIST", "OBSERVE", "DIRECT", "INCOG"] {
                let _ = ui.button(chip);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("HiDPI x{:.2}", *zoom)).clicked() {
                    *zoom = match *zoom {
                        z if z < 1.24 => 1.25,
                        z if z < 1.49 => 1.5,
                        z if z < 1.99 => 2.0,
                        _ => 1.0,
                    };
                }
            });
        });
        ui.add_space(6.0);
        // Row 2: the intent bar gets its own full-width horizontal space.
        ui.horizontal(|ui| {
            let button_w = 64.0;
            let field_w = (ui.available_width() - button_w - ui.spacing().item_spacing.x).max(80.0);
            ui.add(
                egui::TextEdit::singleline(intent)
                    .desired_width(field_w)
                    .hint_text("ENTER AN INTENT..."),
            );
            let _ = ui.button(RichText::new("RUN").color(green));
        });
        ui.add_space(4.0);
    });

    egui::TopBottomPanel::bottom("retro_status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("MODE AGENT    AI CONTROL    ENGINE SERVO    RENDER egui(SW)")
                    .color(dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("RETRO egui PROOF").color(cyan));
            });
        });
    });

    egui::SidePanel::right("retro_rail")
        .resizable(false)
        .exact_width(320.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("MAYA SIDECAR").color(dim).small());
            ui.label(RichText::new("\u{25CF} PILOT COMPLETE").color(green));
            ui.separator();
            ui.label(RichText::new("ACTIVE CONTEXT").color(dim).small());
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("INTENT: DISTILL THE ACTIVE PAGE INTO WAKE")
            });
            ui.add_space(6.0);
            ui.label(RichText::new("QUICK INTENTS").color(dim).small());
            ui.horizontal(|ui| {
                let _ = ui.button("SUMMARIZE");
                let _ = ui.button("DISTILL HERE");
            });
        });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (label, value, sub) in [
                ("TABS", "1", "ACTIVE SESSION"),
                ("ENGINE", "SERVO", "LOCAL BUILD"),
                ("WAKE", "10", "SEARCH HITS"),
                ("PAGE", "READY", "DISTILL STATUS"),
            ] {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(150.0);
                        ui.label(RichText::new(label).color(dim).small());
                        ui.label(RichText::new(value).color(cyan).size(22.0).strong());
                        ui.label(RichText::new(sub).color(dim).small());
                    });
                });
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("ACTIVE PAGE").color(dim).small());
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Example Domain").heading().color(cyan));
            ui.label("Crisp anti-aliased monospace at the display's native pixel density.");
            ui.label("The quick brown fox jumps over the lazy dog  0123456789  ()[]{}<>=+-/*");
            ui.label(
                RichText::new("Same retro HUD feel as the softbuffer shell -- sharpened.")
                    .color(green),
            );
        });
    });
}

/// Standalone proof window for the retro-egui look (`--retro-egui-proof`). Reuses
/// the production egui+softbuffer chrome path (`init_chrome_backend` /
/// `ChromeBackend::render`) so it renders the same way on this GPU-less host.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn run_retro_egui_proof() -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|error| format!("event loop initialization failed: {error}"))?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sextant - Retro egui Theme Proof")
            .with_inner_size(PhysicalSize::new(1180, 760))
            .with_window_icon(sextant_window_icon())
            .build(&event_loop)
            .map_err(|error| format!("window creation failed: {error}"))?,
    );
    let mut chrome = init_chrome_backend(&window)?;
    let mut size = window.inner_size();
    let egui_ctx = egui::Context::default();
    apply_retro_egui_theme(&egui_ctx);
    let mut egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        egui::ViewportId::ROOT,
        &window,
        Some(window.scale_factor() as f32),
        None,
    );
    let mut intent = "INTENT: DISTILL THE ACTIVE PAGE INTO WAKE".to_string();
    let mut zoom = 1.0f32;

    let event_loop_result = event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);
        if let Event::WindowEvent { event, window_id } = event {
            if window_id != window.id() {
                return;
            }
            let response = egui_state.on_window_event(&window, &event);
            if response.repaint && !matches!(event, WindowEvent::RedrawRequested) {
                window.request_redraw();
            }
            match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(new_size) => {
                    size = new_size;
                    chrome.resize(new_size);
                    window.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    if size.width == 0 || size.height == 0 {
                        return;
                    }
                    egui_ctx.set_zoom_factor(zoom);
                    let raw_input = egui_state.take_egui_input(&window);
                    let full_output = egui_ctx.run(raw_input, |ctx| {
                        draw_retro_egui_proof(ctx, &mut intent, &mut zoom);
                    });
                    egui_state.handle_platform_output(&window, full_output.platform_output);
                    chrome.render(
                        &egui_ctx,
                        full_output.shapes,
                        full_output.textures_delta,
                        full_output.pixels_per_point,
                        size,
                        true,
                    );
                }
                _ => {}
            }
        }
    });
    event_loop_result.map_err(|error| format!("retro egui proof event loop failed: {error}"))
}
