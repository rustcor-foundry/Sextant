//! Hosted-direct egui shell — the interactive hosted-direct browser with an
//! egui chrome (tab strip, navigation, bookmarks), rendered through wgpu (or a
//! softbuffer fallback) over the embedded Servo child window. Extracted from
//! the browser.rs crate root; gated once at the `mod` declaration on
//! `all(windows, servo-backend)`. `use super::*` supplies the crate root's
//! types/imports so the moved bodies are unchanged.

use super::*;

/// Interactive hosted-direct shell with an egui chrome (tab strip, navigation,
/// bookmarks). The chrome is rendered through the GPU (wgpu) when a hardware
/// adapter is available, or software-rasterized into a softbuffer surface as a
/// fallback on GPU-less hosts; see [`init_chrome_backend`]. It hosts the embedded
/// Servo child window below the chrome. The softbuffer `run_hosted_direct_app`
/// above is retained for the `--hosted-direct-smoke` proofs.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn run_hosted_direct_app_egui(
    startup_input: Option<String>,
    browser_mode: BrowserMode,
    certificate_path: Option<PathBuf>,
    local_appliance_cert_fingerprint: Option<String>,
    remember_local_appliance_cert: Option<String>,
    allow_insecure_local_tls: bool,
    direct_resource_audit: bool,
) -> Result<(), String> {
    let target = startup_input.unwrap_or_else(|| "https://example.com".to_string());

    let event_loop =
        EventLoop::new().map_err(|error| format!("event loop initialization failed: {error}"))?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sextant Browser - Direct")
            .with_inner_size(PhysicalSize::new(1180, 760))
            .with_window_icon(sextant_window_icon())
            .build(&event_loop)
            .map_err(|error| format!("window creation failed: {error}"))?,
    );
    let parent_hwnd = window_hwnd(&window).ok_or_else(|| {
        "hosted direct mode could not resolve the parent window handle".to_string()
    })?;

    // Render the chrome through the GPU when one is available, or fall back to a
    // pure-CPU softbuffer surface on GPU-less hosts (see `init_chrome_backend`).
    let mut chrome = init_chrome_backend(&window)?;

    let mut size = window.inner_size();
    let egui_ctx = egui::Context::default();
    let mut egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        egui::ViewportId::ROOT,
        &window,
        Some(window.scale_factor() as f32),
        None,
    );

    let mut child = Some(spawn_hosted_direct_child(
        parent_hwnd,
        size,
        &target,
        browser_mode,
        certificate_path.as_deref(),
        local_appliance_cert_fingerprint.as_deref(),
        remember_local_appliance_cert.as_deref(),
        allow_insecure_local_tls,
        direct_resource_audit,
    )?);

    let mut summary = HostedDirectLogSummary::default();
    let mut log_monitor = HostedDirectLogMonitor::default();
    let log_path = child.as_ref().map(|child| child.log_path.clone());
    let mut next_refresh = Instant::now();
    let mut address = target.clone();
    let mut address_focused = false;
    let mut bookmarks = load_hosted_direct_bookmarks();
    let mut last_positioned: Option<(u32, u32, u32)> = None;
    let started = Instant::now();

    let event_loop_result = event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);
        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => {
                let response = egui_state.on_window_event(&window, &event);
                // Do not let a RedrawRequested feed back into another redraw request
                // (egui reports `repaint` for it), which would spin the loop redrawing
                // a static chrome. Only genuine input/state events schedule a redraw.
                if response.repaint && !matches!(event, WindowEvent::RedrawRequested) {
                    window.request_redraw();
                }
                match event {
                    WindowEvent::CloseRequested => {
                        terminate_child_process(&mut child);
                        elwt.exit();
                    }
                    WindowEvent::Resized(new_size) => {
                        size = new_size;
                        chrome.resize(new_size);
                        last_positioned = None;
                        window.request_redraw();
                    }
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                        ..
                    } => {
                        // A click reaching the parent is in the chrome (the
                        // embedded child window covers the content area below);
                        // grab OS keyboard focus so the egui address bar receives
                        // typed input instead of the child holding it.
                        focus_hosted_direct_parent_window(&window, parent_hwnd);
                        window.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        if size.width == 0 || size.height == 0 {
                            return;
                        }
                        if !address_focused {
                            if let Some(url) = summary.latest_url.as_deref() {
                                address = url.to_string();
                            }
                        }
                        let mut action: Option<HostedDirectChromeAction> = None;
                        let raw_input = egui_state.take_egui_input(&window);
                        let full_output = egui_ctx.run(raw_input, |ctx| {
                            action = draw_hosted_direct_chrome_egui(
                                ctx,
                                &summary,
                                &mut address,
                                &mut address_focused,
                                &bookmarks,
                            );
                        });
                        egui_state.handle_platform_output(&window, full_output.platform_output);
                        match action {
                            Some(HostedDirectChromeAction::Child(command)) => {
                                send_hosted_direct_command(child.as_mut(), &command);
                                next_refresh = Instant::now();
                            }
                            Some(HostedDirectChromeAction::SelectTab(index)) => {
                                send_hosted_direct_command(
                                    child.as_mut(),
                                    &format!("select-tab {index}"),
                                );
                                next_refresh = Instant::now();
                            }
                            Some(HostedDirectChromeAction::CloseTab(index)) => {
                                send_hosted_direct_command(
                                    child.as_mut(),
                                    &format!("select-tab {index}"),
                                );
                                send_hosted_direct_command(child.as_mut(), "close-tab");
                                next_refresh = Instant::now();
                            }
                            Some(HostedDirectChromeAction::AddBookmark) => {
                                if let Some(url) = summary.latest_url.clone() {
                                    if !bookmarks.iter().any(|bookmark| bookmark.url == url) {
                                        let title = summary
                                            .active_title
                                            .clone()
                                            .filter(|title| !title.is_empty())
                                            .unwrap_or_else(|| url.clone());
                                        bookmarks.push(HostedDirectBookmark { title, url });
                                        save_hosted_direct_bookmarks(&bookmarks);
                                    }
                                }
                            }
                            None => {}
                        }
                        chrome.render(
                            &egui_ctx,
                            full_output.shapes,
                            full_output.textures_delta,
                            full_output.pixels_per_point,
                            size,
                            false,
                        );
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                let now = Instant::now();
                if let Some((_status, _path)) = poll_hosted_direct_child_exit(&mut child) {
                    elwt.exit();
                    return;
                }
                if now >= next_refresh {
                    if let Some(path) = log_path.as_deref() {
                        if let Some(updated) = log_monitor.refresh(path) {
                            summary = updated;
                            window.request_redraw();
                        }
                    }
                    next_refresh = now + HOSTED_DIRECT_SUMMARY_REFRESH;
                }
                // Keep the embedded child sized to sit just below the egui chrome;
                // its physical top offset tracks the display scale.
                if let Some(child) = child.as_mut() {
                    let chrome_px =
                        (HOSTED_DIRECT_CHROME_POINTS * window.scale_factor() as f32).round() as u32;
                    let key = (size.width, size.height, chrome_px);
                    if last_positioned != Some(key)
                        && position_hosted_direct_child(parent_hwnd, child, size, chrome_px)
                    {
                        last_positioned = Some(key);
                        window.request_redraw();
                    }
                }
                // Keep nudging the embedded child to repaint until it reports a
                // first present, so the initial page is visible without an input
                // event waking the loop.
                if summary.first_present.is_none() && started.elapsed() < Duration::from_secs(6) {
                    window.request_redraw();
                    elwt.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(100)));
                } else {
                    elwt.set_control_flow(ControlFlow::WaitUntil(next_refresh));
                }
            }
            _ => {}
        }
    });
    // In Incognito the embedded child is hard-killed on close, so its ephemeral
    // Servo data dir (cookies, IndexedDB, ...) is never cleaned by the child's own
    // exit path. Sweep leftover unlocked incognito dirs here; a dir still in use by
    // a concurrent instance stays locked and is left untouched.
    if browser_mode == BrowserMode::Incognito {
        sweep_direct_incognito_data_dirs();
    }
    event_loop_result.map_err(|error| format!("hosted direct egui event loop failed: {error}"))
}

/// Remove leftover ephemeral Incognito Servo data dirs from `%TEMP%`. Dirs still
/// held open by a running instance fail to delete and are skipped, so this is safe
/// to call while other Incognito windows may be open.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn sweep_direct_incognito_data_dirs() {
    let Ok(entries) = std::fs::read_dir(env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_incognito_dir = path.is_dir()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("sextant-browser-direct-incognito-"));
        if is_incognito_dir {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// Build the three-row egui chrome (tab strip, navigation, bookmarks) and return
/// the action the event loop should apply, if any.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn draw_hosted_direct_chrome_egui(
    ctx: &egui::Context,
    summary: &HostedDirectLogSummary,
    address: &mut String,
    address_focused: &mut bool,
    bookmarks: &[HostedDirectBookmark],
) -> Option<HostedDirectChromeAction> {
    let mut action: Option<HostedDirectChromeAction> = None;
    egui::TopBottomPanel::top("hosted-direct-chrome")
        .exact_height(HOSTED_DIRECT_CHROME_POINTS)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            // Row 1: standard-width tab strip, like Chrome/Firefox — each tab shows
            // a favicon-placeholder initial, the (truncated) site, and a nested
            // close button; a `+` opens a new tab.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                let multi_tab = summary.tabs.len() > 1;
                if summary.tabs.is_empty() {
                    let label =
                        truncate_label(summary.active_title.as_deref().unwrap_or("New Tab"), 16);
                    let _ = ui.add_sized([150.0, 26.0], egui::SelectableLabel::new(true, label));
                } else {
                    for (index, tab) in summary.tabs.iter().enumerate() {
                        let site = hosted_direct_tab_site(&tab.url);
                        let initial = site.chars().next().unwrap_or('•').to_ascii_uppercase();
                        let label = format!("{initial}   {}", truncate_label(&site, 14));
                        if ui
                            .add_sized([138.0, 26.0], egui::SelectableLabel::new(tab.active, label))
                            .on_hover_text(&tab.url)
                            .clicked()
                            && !tab.active
                        {
                            action = Some(HostedDirectChromeAction::SelectTab(index));
                        }
                        if multi_tab && ui.small_button("×").on_hover_text("Close tab").clicked() {
                            action = Some(HostedDirectChromeAction::CloseTab(index));
                        }
                    }
                }
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("+").size(18.0))
                            .min_size(egui::vec2(28.0, 26.0)),
                    )
                    .on_hover_text("New tab")
                    .clicked()
                {
                    action = Some(HostedDirectChromeAction::Child("new-tab".to_string()));
                }
            });
            ui.separator();
            // Row 2: navigation controls and the address bar.
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(summary.can_go_back, egui::Button::new("◀"))
                    .on_hover_text("Back")
                    .clicked()
                {
                    action = Some(HostedDirectChromeAction::Child("back".to_string()));
                }
                if ui
                    .add_enabled(summary.can_go_forward, egui::Button::new("▶"))
                    .on_hover_text("Forward")
                    .clicked()
                {
                    action = Some(HostedDirectChromeAction::Child("forward".to_string()));
                }
                if ui.button("⟳").on_hover_text("Reload").clicked() {
                    action = Some(HostedDirectChromeAction::Child("reload".to_string()));
                }
                let address_response = ui.add_sized(
                    [ui.available_width(), 24.0],
                    egui::TextEdit::singleline(address).hint_text("Search or enter address"),
                );
                *address_focused = address_response.has_focus();
                if address_response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter))
                {
                    let target = address.trim();
                    if !target.is_empty() {
                        action = Some(HostedDirectChromeAction::Child(format!(
                            "navigate {target}"
                        )));
                    }
                }
            });
            ui.separator();
            // Row 3: bookmarks bar (or the certificate warning when active).
            if summary.certificate_warning_active() {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xd9, 0xa4, 0x41),
                        "Certificate warning",
                    );
                    if ui.button("Go back").clicked() {
                        action = Some(HostedDirectChromeAction::Child(
                            "certificate-go-back".to_string(),
                        ));
                    }
                    if ui.button("Trust once").clicked() {
                        action = Some(HostedDirectChromeAction::Child("trust-once".to_string()));
                    }
                    if ui.button("Trust this appliance").clicked() {
                        action = Some(HostedDirectChromeAction::Child(
                            "trust-this-appliance".to_string(),
                        ));
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui.button("★").on_hover_text("Bookmark this page").clicked() {
                        action = Some(HostedDirectChromeAction::AddBookmark);
                    }
                    if summary.is_loading() {
                        ui.add(egui::ProgressBar::new(summary.load_progress()).desired_width(90.0));
                    }
                    ui.separator();
                    if bookmarks.is_empty() {
                        ui.weak("No bookmarks yet — ★ saves the current page");
                    } else {
                        for bookmark in bookmarks {
                            if ui
                                .button(truncate_label(&bookmark.title, 18))
                                .on_hover_text(&bookmark.url)
                                .clicked()
                            {
                                *address = bookmark.url.clone();
                                action = Some(HostedDirectChromeAction::Child(format!(
                                    "navigate {}",
                                    bookmark.url
                                )));
                            }
                        }
                    }
                });
            }
        });
    action
}
