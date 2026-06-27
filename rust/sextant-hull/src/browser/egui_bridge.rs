//! egui bridge shell — the experimental egui-rendered chrome (gated behind
//! the SEXTANT_EGUI_BRIDGE flag), extracted from the browser.rs crate root.
//!
//! Renders the same `BrowserApp` state/logic as the softbuffer bridge shell,
//! but through the retro egui theme for anti-aliased text + HiDPI. The whole
//! module is gated at its `mod` declaration on
//! `all(windows, servo-backend, xilem-shell)`, so the per-fn cfg attrs below
//! are redundant-but-verbatim from the original location.

use super::*;

/// Convert a softbuffer `0x00RRGGBB` palette color to an egui `Color32`.
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn rgb32(color: u32) -> egui::Color32 {
    egui::Color32::from_rgb((color >> 16) as u8, (color >> 8) as u8, color as u8)
}

#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn egui_bridge_tabs() -> Vec<(MainView, &'static str)> {
    vec![
        (MainView::Browser, "BROWSER"),
        (MainView::Pilot, "PILOT"),
        (MainView::Wake, "WAKE"),
        (MainView::Log, "LOG"),
        (MainView::Guard, "GUARD"),
        (MainView::Perception, "SENSE"),
        (MainView::Perf, "PERF"),
        (MainView::Validation, "VALIDATION"),
        (MainView::Settings, "SETTINGS"),
    ]
}

#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn egui_metric_card(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    sub: &str,
    value_color: egui::Color32,
) {
    let dim = egui::Color32::from_rgb(0x93, 0xa4, 0xb0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(150.0);
            ui.label(egui::RichText::new(label).color(dim).small());
            ui.label(
                egui::RichText::new(value)
                    .color(value_color)
                    .size(20.0)
                    .strong(),
            );
            ui.label(egui::RichText::new(sub).color(dim).small());
        });
    });
}

/// Per-view central content. First pass: Browser shows the active URL (the Servo
/// frame viewport is the final port step), Settings has the backend selector, and
/// the data tabs show summaries pending their full panel port.
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn egui_bridge_central(
    ui: &mut egui::Ui,
    app: &mut BrowserApp,
    viewport: Option<&egui::TextureHandle>,
) {
    let dim = egui::Color32::from_rgb(0x93, 0xa4, 0xb0);
    let cyan = egui::Color32::from_rgb(0x0e, 0xc7, 0xe8);
    let green = egui::Color32::from_rgb(0x2f, 0xbf, 0x71);
    let red = egui::Color32::from_rgb(0xe0, 0x52, 0x4a);
    match app.main_view {
        MainView::Browser => {
            ui.label(egui::RichText::new("ACTIVE PAGE").color(dim).small());
            let url = app
                .active_tab()
                .and_then(|tab| tab.url.as_ref())
                .map(|u| u.to_string());
            match url {
                Some(u) => {
                    ui.label(egui::RichText::new(u).color(cyan));
                }
                None => {
                    ui.label(
                        egui::RichText::new("NO ACTIVE PAGE — enter a URL or intent above.")
                            .color(dim),
                    );
                }
            }
            ui.add_space(6.0);
            match viewport {
                Some(texture) => {
                    // Live Servo frame uploaded as an egui texture (see
                    // `run_visible_app_egui`). Fit it into the available area while
                    // preserving the captured frame's aspect ratio.
                    let [tw, th] = texture.size();
                    let (tw, th) = (tw.max(1) as f32, th.max(1) as f32);
                    let avail = ui.available_size();
                    let avail_h = avail.y.max(120.0);
                    let scale = (avail.x / tw).min(avail_h / th).max(0.0);
                    let draw = egui::vec2((tw * scale).max(1.0), (th * scale).max(1.0));
                    let response = ui.add(
                        egui::Image::new(egui::load::SizedTexture::new(texture.id(), draw))
                            .sense(egui::Sense::click_and_drag()),
                    );
                    forward_egui_viewport_input(ui, app, &response, tw, th);
                    ui.label(
                        egui::RichText::new(format!(
                            "SERVO FRAME {}x{}  ·  CLICK TO FOCUS, THEN TYPE / SCROLL TO INTERACT",
                            tw as u32, th as u32
                        ))
                        .color(dim)
                        .small(),
                    );
                }
                None => {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_height(140.0);
                        match app.active_tab().and_then(|tab| tab.distilled_page.as_ref()) {
                            Some(page) => {
                                ui.label(egui::RichText::new(page.title.clone()).color(cyan));
                                ui.label(
                                    egui::RichText::new(page.url.as_str().to_string())
                                        .color(dim)
                                        .small(),
                                );
                                ui.add_space(4.0);
                                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                                    ui.label(egui::RichText::new(page.content.clone()).color(dim));
                                });
                            }
                            None => {
                                ui.label(
                                    egui::RichText::new(
                                        "NO SERVO FRAME YET — press RUN or an intent to capture the live viewport.",
                                    )
                                    .color(dim),
                                );
                            }
                        }
                    });
                }
            }
        }
        MainView::Settings => {
            ui.label(egui::RichText::new("LOCAL AI BACKEND").color(dim).small());
            ui.horizontal(|ui| {
                for backend in AiLocalConfig::BACKENDS {
                    let active = app.ai_config.backend == backend;
                    if ui
                        .selectable_label(active, AiLocalConfig::backend_label(backend))
                        .clicked()
                    {
                        app.apply_ai_backend(backend);
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("ENDPOINT  {}", app.ai_config.endpoint))
                    .color(dim)
                    .small(),
            );
            ui.label(
                egui::RichText::new(format!("MODEL     {}", app.ai_config.model))
                    .color(dim)
                    .small(),
            );
        }
        MainView::Wake => {
            ui.label(egui::RichText::new("DIGITAL WAKE").color(cyan).strong());
            if app.wake_results.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "NO WAKE RESULTS YET. DISTILL A PAGE OR SEARCH WAKE IN THE RAIL.",
                    )
                    .color(dim),
                );
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for entry in &app.wake_results {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.label(egui::RichText::new(format!(
                                "{} | {}",
                                entry.title,
                                short_url(&entry.url)
                            )));
                            ui.label(
                                egui::RichText::new(format!(
                                    "IMPORTANCE {:.2} | USED {}",
                                    entry.importance, entry.usage_count
                                ))
                                .color(dim)
                                .small(),
                            );
                        });
                    }
                });
            }
        }
        MainView::Pilot => {
            ui.label(egui::RichText::new("PILOT RUN").color(cyan).strong());
            if let Some(run) = app.last_pilot_run.as_ref() {
                ui.label(egui::RichText::new(format!("INTENT  {}", run.intent)));
                ui.label(
                    egui::RichText::new(format!(
                        "PLANNER {}  |  {}  |  {}ms",
                        run.planner, run.status, run.reason_ms
                    ))
                    .color(dim)
                    .small(),
                );
                ui.separator();
                ui.label(egui::RichText::new("PLAN").color(dim).small());
                for (index, step) in run.plan.iter().enumerate() {
                    ui.label(egui::RichText::new(format!(
                        "{}. {}  {}",
                        index + 1,
                        step.kind.to_uppercase(),
                        step.detail
                    )));
                }
                if run.analysis.iter().any(|line| !line.trim().is_empty()) {
                    ui.separator();
                    ui.label(egui::RichText::new("ANALYSIS").color(dim).small());
                    for line in &run.analysis {
                        if !line.trim().is_empty() {
                            ui.label(egui::RichText::new(line));
                        }
                    }
                }
            } else {
                ui.label(
                    egui::RichText::new("NO PILOT RUN YET. ENTER AN INTENT IN THE ADDRESS BAR.")
                        .color(dim),
                );
            }
        }
        MainView::Log => {
            ui.label(egui::RichText::new("CAPTAIN'S LOG").color(cyan).strong());
            if app.recent_logs.is_empty() {
                ui.label(egui::RichText::new("NO LOG ENTRIES YET").color(dim));
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for entry in &app.recent_logs {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} | {}",
                                status_label(&entry.status),
                                entry.intent
                            ))
                            .small(),
                        );
                    }
                });
            }
        }
        MainView::Guard => {
            ui.label(egui::RichText::new("LOCAL GUARD").color(cyan).strong());
            let lines = {
                let active_url = app
                    .active_tab()
                    .and_then(|tab| tab.url.as_ref())
                    .or_else(|| {
                        app.active_tab()
                            .and_then(|tab| tab.distilled_page.as_ref())
                            .map(|page| &page.url)
                    });
                let active_page = app.active_tab().and_then(|tab| tab.distilled_page.as_ref());
                guard_report_lines(app, active_url, active_page)
            };
            for line in &lines {
                let color = if line.contains("BLOCK") || line.contains("HARDENED") {
                    red
                } else if line.contains("ALLOW") || line.contains("ONLINE") {
                    green
                } else {
                    dim
                };
                ui.label(egui::RichText::new(line.as_str()).color(color).small());
            }
            ui.add_space(8.0);
            ui.separator();
            egui_appliance_cert_section(ui, app);
        }
        MainView::Perception => {
            ui.label(egui::RichText::new("PAGE PERCEPTION").color(cyan).strong());
            match app.active_tab().and_then(|tab| tab.distilled_page.as_ref()) {
                None => {
                    ui.label(
                        egui::RichText::new(
                            "NO DISTILLED PAGE YET. DISTILL THE ACTIVE TAB TO BUILD A SEMANTIC MAP.",
                        )
                        .color(dim),
                    );
                }
                Some(page) => {
                    let counts = semantic_counts(page);
                    ui.label(egui::RichText::new(page.title.clone()).strong());
                    ui.label(
                        egui::RichText::new(page.url.as_str().to_string())
                            .color(dim)
                            .small(),
                    );
                    ui.label(egui::RichText::new(page_perception_summary(page)).color(green));
                    ui.label(
                        egui::RichText::new(format!(
                            "SEMANTICS H={} LINKS={} BUTTONS={} INPUTS={} IMAGES={} TEXT={}",
                            counts.headings,
                            counts.links,
                            counts.buttons,
                            counts.inputs,
                            counts.images,
                            counts.text
                        ))
                        .color(dim)
                        .small(),
                    );
                    let source = page
                        .metadata
                        .get("distillation_backend")
                        .or_else(|| page.metadata.get("source"))
                        .or_else(|| page.metadata.get("distiller"))
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());
                    ui.label(
                        egui::RichText::new(format!("SOURCE {source}"))
                            .color(dim)
                            .small(),
                    );
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("sense_nodes").striped(true).show(ui, |ui| {
                            ui.label(egui::RichText::new("TYPE").color(dim).small());
                            ui.label(egui::RichText::new("SELECTOR").color(dim).small());
                            ui.label(egui::RichText::new("TEXT").color(dim).small());
                            ui.end_row();
                            for node in key_semantic_nodes(page) {
                                let color = match &node.node_type {
                                    NodeType::Heading => cyan,
                                    NodeType::Input | NodeType::Button => red,
                                    NodeType::Link => green,
                                    NodeType::Image | NodeType::Text => dim,
                                };
                                ui.label(
                                    egui::RichText::new(semantic_node_type_label(&node.node_type))
                                        .color(color),
                                );
                                ui.label(
                                    egui::RichText::new(truncate(&node.selector, 40)).color(dim),
                                );
                                ui.label(
                                    egui::RichText::new(truncate(&node.text, 80)).color(color),
                                );
                                ui.end_row();
                            }
                        });
                    });
                }
            }
        }
        MainView::Perf => {
            ui.label(
                egui::RichText::new("BROWSER PERFORMANCE")
                    .color(cyan)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!("LATEST {}", app.perf.summary()))
                    .color(dim)
                    .small(),
            );
            let slowest = app
                .slowest_perf_event()
                .map(|event| {
                    format!(
                        "SLOWEST {} {} {}",
                        event.phase,
                        format_duration(event.duration),
                        event.label
                    )
                })
                .unwrap_or_else(|| "SLOWEST pending".to_string());
            ui.label(egui::RichText::new(slowest).color(red).small());
            ui.add_space(6.0);
            if app.perf_events.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "NO PERFORMANCE EVENTS YET. OPEN, DISTILL, OR CAPTURE A FRAME.",
                    )
                    .color(dim),
                );
            } else {
                let slow = app
                    .slowest_perf_event()
                    .map(|event| (event.phase, event.duration));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("perf_events").striped(true).show(ui, |ui| {
                        ui.label(egui::RichText::new("PHASE").color(dim).small());
                        ui.label(egui::RichText::new("TIME").color(dim).small());
                        ui.label(egui::RichText::new("DETAIL").color(dim).small());
                        ui.end_row();
                        for event in app.perf_events.iter().rev() {
                            let hot = slow
                                .map(|(phase, duration)| {
                                    phase == event.phase && duration == event.duration
                                })
                                .unwrap_or(false);
                            let color = if hot { red } else { dim };
                            ui.label(egui::RichText::new(event.phase).color(color));
                            ui.label(
                                egui::RichText::new(format_duration(event.duration)).color(color),
                            );
                            ui.label(egui::RichText::new(truncate(&event.label, 80)).color(color));
                            ui.end_row();
                        }
                    });
                });
            }
        }
        MainView::Validation => {
            let rows = app.validation_rows();
            let pass = app.validation_pass_count();
            ui.label(
                egui::RichText::new("NATIVE BROWSER VALIDATION")
                    .color(cyan)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!("{} OF {} CHECKS COMPLETE", pass, rows.len()))
                    .color(if pass == rows.len() { green } else { red }),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for row in &rows {
                    ui.horizontal(|ui| {
                        let color = rgb32(validation_status_color(row.status));
                        ui.label(
                            egui::RichText::new(validation_status_label(row.status)).color(color),
                        );
                        ui.label(egui::RichText::new(row.label).strong());
                        ui.label(egui::RichText::new(truncate(&row.detail, 80)).color(dim));
                    });
                }
            });
        }
    }
}

/// GUARD-tab local appliance certificate trust list: status, REFRESH/FORGET
/// controls, and a selectable list of persisted origin+fingerprint entries.
/// Click intents are recorded and applied after the immutable render borrow ends.
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn egui_appliance_cert_section(ui: &mut egui::Ui, app: &mut BrowserApp) {
    let dim = egui::Color32::from_rgb(0x93, 0xa4, 0xb0);
    ui.label(
        egui::RichText::new("LOCAL APPLIANCE CERTIFICATES")
            .color(dim)
            .small(),
    );

    let can_forget = app
        .selected_appliance_cert
        .is_some_and(|index| index < app.appliance_cert_entries.len());
    let mut refresh = false;
    let mut forget = false;
    ui.horizontal(|ui| {
        if ui.button("REFRESH").clicked() {
            refresh = true;
        }
        if ui
            .add_enabled(can_forget, egui::Button::new("FORGET"))
            .clicked()
        {
            forget = true;
        }
    });
    ui.label(
        egui::RichText::new(truncate(&app.appliance_cert_status, 160))
            .color(dim)
            .small(),
    );

    let mut to_select: Option<usize> = None;
    if app.appliance_cert_entries.is_empty() {
        ui.label(
            egui::RichText::new("NO TRUSTED LOCAL APPLIANCE CERTIFICATES.")
                .color(dim)
                .small(),
        );
    } else {
        for (index, entry) in app.appliance_cert_entries.iter().enumerate() {
            let selected = app.selected_appliance_cert == Some(index);
            let label = appliance_cert_entry_label(entry);
            let fingerprint = compact_fingerprint(&entry.fingerprint_sha256);
            let created = truncate(&entry.created_at, 24);
            if ui
                .selectable_label(selected, format!("{label}  ({fingerprint} | {created})"))
                .clicked()
            {
                to_select = Some(index);
            }
        }
    }

    if let Some(index) = to_select {
        app.selected_appliance_cert = Some(index);
    }
    if refresh {
        app.refresh_appliance_cert_trust();
    }
    if forget {
        app.forget_selected_appliance_cert();
    }
}

/// Convert a captured Servo frame (`0x00RRGGBB` packed pixels) into an egui
/// `ColorImage` for upload as a texture. Defends the `len == w*h` invariant.
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn frame_to_color_image(frame: &RenderedFrame) -> egui::ColorImage {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let expected = width.saturating_mul(height);
    let mut pixels = Vec::with_capacity(expected);
    for &packed in frame.pixels.iter().take(expected) {
        pixels.push(egui::Color32::from_rgb(
            (packed >> 16) as u8,
            (packed >> 8) as u8,
            packed as u8,
        ));
    }
    pixels.resize(expected, egui::Color32::BLACK);
    egui::ColorImage {
        size: [width, height],
        pixels,
    }
}

/// Cheap content hash (FNV-1a over packed pixels + dims) so the viewport texture
/// is re-uploaded only when the captured frame actually changes.
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn frame_fingerprint(frame: &RenderedFrame) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &packed in &frame.pixels {
        hash ^= packed as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^= ((frame.width as u64) << 32) | frame.height as u64;
    hash
}

/// Map an egui `Key` to the engine's `BrowserKey` for viewport key forwarding.
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn browser_key_from_egui(key: egui::Key) -> Option<BrowserKey> {
    Some(match key {
        egui::Key::Enter => BrowserKey::Enter,
        egui::Key::Backspace => BrowserKey::Backspace,
        egui::Key::Tab => BrowserKey::Tab,
        egui::Key::Escape => BrowserKey::Escape,
        egui::Key::ArrowLeft => BrowserKey::ArrowLeft,
        egui::Key::ArrowRight => BrowserKey::ArrowRight,
        egui::Key::ArrowUp => BrowserKey::ArrowUp,
        egui::Key::ArrowDown => BrowserKey::ArrowDown,
        egui::Key::Delete => BrowserKey::Delete,
        _ => return None,
    })
}

/// Forward pointer, wheel, and keyboard input from the egui viewport `Image` into
/// the live Servo session. The image-local pointer position is mapped to Servo
/// viewport pixels (the captured frame size) so it is independent of the on-screen
/// display scale / aspect-fit. Keyboard is forwarded only while the viewport image
/// holds egui focus, so it never steals input from the address / Wake text fields.
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn forward_egui_viewport_input(
    ui: &egui::Ui,
    app: &mut BrowserApp,
    response: &egui::Response,
    frame_w: f32,
    frame_h: f32,
) {
    let rect = response.rect;
    let to_frame = |pos: egui::Pos2| -> (f32, f32) {
        let fx = ((pos.x - rect.min.x) / rect.width().max(1.0) * frame_w)
            .clamp(0.0, (frame_w - 1.0).max(0.0));
        let fy = ((pos.y - rect.min.y) / rect.height().max(1.0) * frame_h)
            .clamp(0.0, (frame_h - 1.0).max(0.0));
        (fx, fy)
    };

    // Pointer movement (hover or drag), throttled by the shared move limiter.
    if let Some(pos) = response
        .hover_pos()
        .or_else(|| response.interact_pointer_pos())
    {
        let (x, y) = to_frame(pos);
        app.forward_browser_mouse_move(x, y, false);
    }

    // Press: a plain click is a down+up pair; a drag presses now, releases later.
    if response.drag_started() || response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (x, y) = to_frame(pos);
            app.forward_browser_mouse_move(x, y, true);
            app.queue_viewport_input(ViewportInputEvent::MouseButton {
                x,
                y,
                pressed: true,
            });
            if response.clicked() {
                app.queue_viewport_input(ViewportInputEvent::MouseButton {
                    x,
                    y,
                    pressed: false,
                });
            }
        }
        response.request_focus();
    }
    if response.drag_stopped() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (x, y) = to_frame(pos);
            app.queue_viewport_input(ViewportInputEvent::MouseButton {
                x,
                y,
                pressed: false,
            });
        }
    }

    // Wheel, only while the pointer is over the viewport.
    if response.hovered() {
        let scroll = ui.input(|i| i.raw_scroll_delta);
        if scroll.x != 0.0 || scroll.y != 0.0 {
            app.queue_viewport_input(ViewportInputEvent::Wheel {
                delta_x: scroll.x as f64,
                delta_y: scroll.y as f64,
                pixel_mode: true,
            });
        }
    }

    // Keyboard, only while the viewport image holds focus.
    if response.has_focus() {
        let events = ui.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    app.queue_viewport_input(ViewportInputEvent::Text { text });
                }
                egui::Event::Key { key, pressed, .. } => {
                    if let Some(browser_key) = browser_key_from_egui(key) {
                        app.queue_viewport_input(ViewportInputEvent::KeyNamed {
                            key: browser_key,
                            pressed,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn draw_egui_bridge(
    ctx: &egui::Context,
    app: &mut BrowserApp,
    viewport: Option<&egui::TextureHandle>,
) {
    use egui::{Color32, Key, RichText};
    let cyan = Color32::from_rgb(0x0e, 0xc7, 0xe8);
    let green = Color32::from_rgb(0x2f, 0xbf, 0x71);
    let red = Color32::from_rgb(0xe0, 0x52, 0x4a);
    let dim = Color32::from_rgb(0x93, 0xa4, 0xb0);

    egui::TopBottomPanel::top("bridge_top").show(ctx, |ui| {
        ui.add_space(4.0);
        // Row 1: branding + mode chips.
        ui.horizontal(|ui| {
            ui.label(RichText::new("SEXTANT").color(cyan).strong());
            ui.label(RichText::new("AI BROWSER").color(dim).small());
            ui.separator();
            for mode in BrowserMode::ALL {
                let active = app.browser_mode == mode;
                if ui
                    .selectable_label(active, mode.label().to_uppercase())
                    .clicked()
                {
                    app.set_browser_mode(mode);
                }
            }
        });
        ui.add_space(6.0);
        // Row 2: nav controls + full-width address/intent bar + RUN.
        ui.horizontal(|ui| {
            let (can_back, can_fwd) = app
                .active_tab()
                .map(|tab| (tab.can_go_back, tab.can_go_forward))
                .unwrap_or((false, false));
            if ui
                .add_enabled(can_back, egui::Button::new("◀"))
                .on_hover_text("Back")
                .clicked()
            {
                app.back();
            }
            if ui
                .add_enabled(can_fwd, egui::Button::new("▶"))
                .on_hover_text("Forward")
                .clicked()
            {
                app.forward();
            }
            if ui.button("⟳").on_hover_text("Reload").clicked() {
                app.reload();
            }
            if ui.button("+").on_hover_text("New tab").clicked() {
                app.new_tab();
            }
            ui.separator();
            let button_w = 56.0;
            let field_w = (ui.available_width() - button_w - ui.spacing().item_spacing.x).max(80.0);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut app.address_input)
                    .desired_width(field_w)
                    .hint_text("URL, search, or  intent: ..."),
            );
            let run = ui.button(RichText::new("RUN").color(green)).clicked();
            if run || (resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                app.navigate_input();
            }
        });
        // Row 2.5: open page tabs — click to switch, × closes the active tab.
        {
            let active_id = app.active_tab().map(|tab| tab.id);
            let tabs: Vec<(Uuid, String, bool)> = app
                .engine
                .get_tabs()
                .iter()
                .map(|tab| {
                    let label = tab
                        .url
                        .as_ref()
                        .map(short_url)
                        .unwrap_or_else(|| "about:blank".to_string());
                    (tab.id, label, Some(tab.id) == active_id)
                })
                .collect();
            if !tabs.is_empty() {
                let mut to_switch = None;
                let mut close_active = false;
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (id, label, active) in &tabs {
                        if ui.selectable_label(*active, truncate(label, 22)).clicked() {
                            to_switch = Some(*id);
                        }
                    }
                    if ui.button("×").on_hover_text("Close active tab").clicked() {
                        close_active = true;
                    }
                });
                if let Some(id) = to_switch {
                    app.switch_to_page_tab(id);
                }
                if close_active {
                    app.close_tab();
                }
            }
        }
        ui.add_space(6.0);
        // Row 3: view tabs.
        ui.horizontal(|ui| {
            for (view, label) in egui_bridge_tabs() {
                if ui.selectable_label(app.main_view == view, label).clicked() {
                    app.main_view = view;
                }
            }
        });
        ui.add_space(4.0);
    });

    egui::TopBottomPanel::bottom("bridge_status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("\u{25CF}").color(if app.last_ok { green } else { red }));
            ui.label(
                RichText::new(format!("MODE {}", app.browser_mode.label().to_uppercase()))
                    .color(dim),
            );
            ui.separator();
            ui.label(RichText::new(truncate(&app.last_status, 140)).color(dim));
        });
    });

    egui::SidePanel::right("bridge_rail")
        .resizable(false)
        .exact_width(320.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("MAYA SIDECAR").color(dim).small());
            let status_label = app
                .pilot_status
                .trim_start_matches("PILOT ")
                .trim()
                .to_string();
            ui.label(
                RichText::new(format!("\u{25CF} {status_label}"))
                    .color(rgb32(pilot_status_color(&app.pilot_status))),
            );
            ui.separator();
            if !app.pilot_plan.is_empty() {
                ui.label(RichText::new("PLAN").color(dim).small());
                for step in &app.pilot_plan {
                    ui.label(RichText::new(format!("\u{2022} {step}")).small());
                }
                ui.add_space(4.0);
            }
            ui.label(RichText::new("RESULT").color(dim).small());
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(if app.pilot_result.is_empty() {
                    "\u{2014}"
                } else {
                    app.pilot_result.as_str()
                });
            });

            if app.native_intent_allowed() && app.pending_consent.is_none() {
                ui.add_space(6.0);
                ui.label(RichText::new("QUICK INTENTS").color(dim).small());
                ui.horizontal(|ui| {
                    for (label, intent) in AI_RAIL_PRESETS {
                        if ui.button(label).clicked() {
                            app.run_preset_intent(intent);
                        }
                    }
                });
            }

            if let Some(pending) = app.pending_consent.clone() {
                ui.add_space(6.0);
                ui.label(RichText::new("CAPTAIN'S KEY").color(dim).small());
                ui.label(RichText::new(truncate(&pending.message, 90)).small());
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("AUTHORIZE").color(green)).clicked() {
                        app.authorize_pilot_consent();
                    }
                    if ui.button("DENY").clicked() {
                        app.deny_pilot_consent();
                    }
                });
            }

            ui.add_space(10.0);
            ui.label(RichText::new("ASK OR SEARCH WAKE").color(dim).small());
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut app.wake_query)
                        .desired_width((ui.available_width() - 70.0).max(60.0)),
                );
                if ui.button("SEARCH").clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                {
                    app.search_wake();
                }
            });
        });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(4.0);
        let tab_count = app.engine.get_tabs().len();
        let backend = app
            .active_tab()
            .map(|tab| display_backend(app, tab).to_string())
            .unwrap_or_else(|| "NONE".to_string());
        let page = app
            .active_tab()
            .and_then(|tab| tab.distilled_page.as_ref())
            .map(|_| "READY")
            .unwrap_or("PENDING");
        ui.horizontal(|ui| {
            egui_metric_card(ui, "TABS", &tab_count.to_string(), "ACTIVE SESSION", green);
            egui_metric_card(ui, "ENGINE", &backend, "LOCAL BUILD", cyan);
            egui_metric_card(
                ui,
                "WAKE",
                &app.wake_results.len().to_string(),
                "SEARCH HITS",
                if app.wake_results.is_empty() {
                    dim
                } else {
                    green
                },
            );
            egui_metric_card(ui, "PAGE", page, "DISTILL STATUS", cyan);
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);
        egui_bridge_central(ui, app, viewport);
    });
}

#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn poll_egui_bridge_lanes(app: &mut BrowserApp) -> bool {
    let mut changed = false;
    changed |= app.collect_pending_navigation().is_some();
    changed |= app.collect_pending_observation_warmup().is_some();
    changed |= app.collect_pending_distillation().is_some();
    changed |= app.collect_pending_persistence().is_some();
    changed |= app.collect_pending_wake_search().is_some();
    changed |= app.collect_pending_log_writes();
    changed |= app.collect_pending_log_refresh();
    changed |= app.collect_pending_pilot_plan().is_some();
    changed |= app.collect_pending_pilot_analysis().is_some();
    changed |= app.collect_pending_frame_capture();
    // Push queued viewport input (clicks / scroll / typed keys) into the live
    // Servo session before refreshing, the same way the softbuffer loop does.
    changed |= app.flush_viewport_input_lane();
    // Drive the live viewport + AI observation the same way the softbuffer event
    // loop does, so the BROWSER frame keeps refreshing instead of freezing after
    // the first capture.
    changed |= app.maybe_refresh_frame();
    changed |= app.maybe_start_scheduled_observation_warmup();
    changed
}

#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
fn egui_bridge_has_pending_async(app: &BrowserApp) -> bool {
    app.pending_navigation.is_some()
        || app.pending_distillation.is_some()
        || app.pending_persistence.is_some()
        || app.pending_wake_search.is_some()
        || app.pending_observation_warmup.is_some()
        || !app.pending_log_writes.is_empty()
        || app.pending_log_refresh.is_some()
        || app.pending_frame_capture.is_some()
        || !app.pending_viewport_input.is_empty()
        || app.pilot_plan_pending()
        || app.pilot_analysis_pending()
}

/// Experimental egui bridge shell entry (`SEXTANT_EGUI_BRIDGE=1`). Reuses the
/// production egui+softbuffer chrome path so it renders on this GPU-less host.
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
pub fn run_visible_app_egui(browser_mode: BrowserMode) -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|error| format!("event loop initialization failed: {error}"))?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sextant Browser (egui)")
            .with_inner_size(PhysicalSize::new(1280, 820))
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
    let mut app =
        BrowserApp::new().map_err(|error| format!("browser app initialization failed: {error}"))?;
    app.observation_warmup_enabled = true;
    app.set_browser_mode(browser_mode);
    if !app.last_ok {
        return Err(app.last_status.clone());
    }
    app.layout(size);

    // Live Servo-frame viewport state for the BROWSER view: keep the last
    // uploaded egui texture plus a fingerprint of the frame it came from, so the
    // texture is re-uploaded only when the captured frame actually changes.
    let mut viewport_tex: Option<egui::TextureHandle> = None;
    let mut viewport_fp: u64 = 0;

    let event_loop_result = event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);
        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => {
                let response = egui_state.on_window_event(&window, &event);
                if response.repaint && !matches!(event, WindowEvent::RedrawRequested) {
                    window.request_redraw();
                }
                match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Resized(new_size) => {
                        size = new_size;
                        chrome.resize(new_size);
                        app.layout(new_size);
                        window.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        if size.width == 0 || size.height == 0 {
                            return;
                        }
                        // Refresh the viewport texture from the latest captured
                        // Servo frame before running the UI, so the BROWSER view
                        // shows live web content (works on GPU and software chrome).
                        match app.latest_frame.as_ref() {
                            Some(frame)
                                if frame.width > 0
                                    && frame.height > 0
                                    && frame.pixels.len()
                                        >= frame.width as usize * frame.height as usize =>
                            {
                                let fingerprint = frame_fingerprint(frame);
                                if viewport_tex.is_none() || fingerprint != viewport_fp {
                                    let image = frame_to_color_image(frame);
                                    viewport_tex = Some(egui_ctx.load_texture(
                                        "sextant_servo_viewport",
                                        image,
                                        egui::TextureOptions::LINEAR,
                                    ));
                                    viewport_fp = fingerprint;
                                }
                            }
                            _ => viewport_tex = None,
                        }
                        let raw_input = egui_state.take_egui_input(&window);
                        let full_output = egui_ctx.run(raw_input, |ctx| {
                            draw_egui_bridge(ctx, &mut app, viewport_tex.as_ref());
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
            Event::AboutToWait => {
                if poll_egui_bridge_lanes(&mut app) {
                    window.request_redraw();
                }
                if egui_bridge_has_pending_async(&app) {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(40),
                    ));
                }
            }
            _ => {}
        }
    });
    event_loop_result.map_err(|error| format!("egui bridge event loop failed: {error}"))
}
