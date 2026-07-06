//! Hosted-direct browser subsystem — `run_hosted_direct_app` and everything it
//! owns: the embedded-child shell state, the GPU/software chrome backend +
//! bilinear rasterizer, Win32 child-window spawn/resize/focus management, the
//! egui-less chrome drawing, and the hosted-direct log parsing. Free functions
//! and types split out of browser.rs; `use super::*` supplies BrowserApp, the
//! model types, and theme colors. Re-exported from the root (`use hosted_direct::*`).

use super::*;

pub fn run_hosted_direct_app(
    startup_input: Option<String>,
    browser_mode: BrowserMode,
    certificate_path: Option<PathBuf>,
    local_appliance_cert_fingerprint: Option<String>,
    remember_local_appliance_cert: Option<String>,
    allow_insecure_local_tls: bool,
    direct_resource_audit: bool,
    hosted_direct_smoke_timeout: Option<Duration>,
    hosted_direct_smoke_action: Option<HostedDirectSmokeAction>,
) -> Result<(), String> {
    if !matches!(browser_mode, BrowserMode::Direct | BrowserMode::Incognito) {
        return Err(format!(
            "hosted direct Servo currently supports Direct or Incognito mode only; {} mode still needs the bridge shell for AI surfaces",
            browser_mode.label()
        ));
    }

    let mut shell = HostedDirectShellState::new(
        startup_input.unwrap_or_else(|| "https://example.com".to_string()),
    );
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
    let context = Context::new(window.clone())
        .map_err(|error| format!("softbuffer context initialization failed: {error}"))?;
    let mut surface = Surface::new(&context, window.clone())
        .map_err(|error| format!("softbuffer surface initialization failed: {error}"))?;
    let mut surface_size = PhysicalSize::new(0, 0);
    let mut direct_summary = HostedDirectLogSummary::default();
    let mut direct_log_monitor = HostedDirectLogMonitor::default();
    let mut next_summary_refresh = Instant::now();
    let mut latest_parent_size = window.inner_size();
    let hosted_smoke_started = Instant::now();
    let mut hosted_smoke_completed = false;
    let mut hosted_smoke_action_requested = false;
    let mut hosted_smoke_action_relaunched = false;
    let mut hosted_smoke_certificate_fingerprint: Option<String> = None;
    let hosted_smoke_result: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));
    let hosted_smoke_result_for_loop = Arc::clone(&hosted_smoke_result);
    let mut hosted_local_appliance_cert_fingerprint = local_appliance_cert_fingerprint;
    let mut direct_child = Some(spawn_hosted_direct_child(
        parent_hwnd,
        latest_parent_size,
        &shell.target,
        browser_mode,
        certificate_path.as_deref(),
        hosted_local_appliance_cert_fingerprint.as_deref(),
        remember_local_appliance_cert.as_deref(),
        allow_insecure_local_tls,
        direct_resource_audit,
        HOSTED_DIRECT_CHROME_H,
    )?);

    window.request_redraw();
    let event_loop_result = event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    terminate_child_process(&mut direct_child);
                    elwt.exit();
                }
                WindowEvent::Resized(size) => {
                    latest_parent_size = size;
                    if let Some(child) = direct_child.as_mut() {
                        resize_hosted_direct_child(
                            parent_hwnd,
                            child,
                            size,
                            HOSTED_DIRECT_CHROME_H,
                        );
                    }
                    direct_summary = HostedDirectLogSummary::default();
                    next_summary_refresh = Instant::now();
                    window.request_redraw();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    shell.cursor_position = Some((position.x, position.y));
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    shell.modifiers = modifiers.state();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if shell.handle_keyboard(event, direct_child.as_mut()) {
                        window.request_redraw();
                    }
                }
                WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
                    if shell.handle_text_commit(text) {
                        window.request_redraw();
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some((x, y)) = shell.cursor_position {
                        if y < HOSTED_DIRECT_CHROME_H as f64 {
                            focus_hosted_direct_parent_window(&window, parent_hwnd);
                        }
                        if shell.handle_mouse_down(
                            PhysicalSize::new(latest_parent_size.width, latest_parent_size.height),
                            x,
                            y,
                            &direct_summary,
                            browser_mode != BrowserMode::Incognito,
                            direct_child.as_mut(),
                        ) {
                            window.request_redraw();
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    let _ = draw_hosted_direct_shell(
                        &window,
                        &mut surface,
                        &mut surface_size,
                        browser_mode,
                        &shell,
                        direct_child.as_ref().map(|child| child.child.id()),
                        direct_child.as_ref().map(|child| child.log_path.as_path()),
                        &direct_summary,
                    );
                }
                _ => {}
            },
            Event::AboutToWait => {
                let now = Instant::now();
                if let Some((status, log_path)) = poll_hosted_direct_child_exit(&mut direct_child) {
                    direct_summary = parse_hosted_direct_log_summary(&log_path);
                    if let Some(trust) = shell.take_pending_certificate_trust() {
                        #[cfg(all(target_os = "windows", feature = "servo-backend"))]
                        match relaunch_hosted_direct_child_after_certificate_trust(
                            parent_hwnd,
                            latest_parent_size,
                            &direct_summary,
                            &shell.target,
                            trust,
                            browser_mode,
                            certificate_path.as_deref(),
                            &mut hosted_local_appliance_cert_fingerprint,
                            remember_local_appliance_cert.as_deref(),
                            direct_resource_audit,
                            HOSTED_DIRECT_CHROME_H,
                        ) {
                            Ok((child, restart_target)) => {
                                shell.target = restart_target.clone();
                                if !shell.address_focused {
                                    shell.address_input = restart_target;
                                    shell.address_cursor = shell.address_input.len();
                                }
                                direct_log_monitor = HostedDirectLogMonitor::default();
                                direct_child = Some(child);
                                direct_summary = HostedDirectLogSummary::default();
                                hosted_smoke_action_relaunched = true;
                                if let Some(action) = hosted_direct_smoke_action {
                                    println!(
                                        "[hosted-direct-smoke] certificate action {} child relaunched",
                                        action.label()
                                    );
                                }
                                window.request_redraw();
                            }
                            Err(error) => {
                                eprintln!(
                                    "[hosted-direct] failed to relaunch trusted child after {status}: {error}"
                                );
                            }
                        }
                        #[cfg(not(all(target_os = "windows", feature = "servo-backend")))]
                        {
                            let _ = trust;
                            eprintln!(
                                "[hosted-direct] child exited ({status}); certificate trust relaunch unavailable in this build"
                            );
                        }
                    } else {
                        eprintln!("[hosted-direct] child exited: {status}");
                    }
                }
                if now >= next_summary_refresh {
                    if let Some(child) = direct_child.as_ref() {
                        if let Some(summary) = direct_log_monitor.refresh(&child.log_path) {
                            direct_summary = summary;
                            if !shell.address_focused {
                                if let Some(url) = direct_summary.latest_url.as_ref() {
                                    shell.target = url.clone();
                                    shell.address_input = url.clone();
                                }
                            }
                            window.request_redraw();
                        }
                    }
                    if let Some(timeout) = hosted_direct_smoke_timeout {
                        if hosted_smoke_completed {
                            elwt.exit();
                            return;
                        }
                        if let Some(action) = hosted_direct_smoke_action {
                            if !hosted_smoke_action_requested
                                && direct_summary.certificate_fingerprint_sha256.is_some()
                            {
                                hosted_smoke_certificate_fingerprint =
                                    direct_summary.certificate_fingerprint_sha256.clone();
                                let rects = hosted_direct_chrome_rects(latest_parent_size);
                                let rect = action.rect(rects);
                                let x = rect.x as f64 + rect.w as f64 / 2.0;
                                let y = rect.y as f64 + rect.h as f64 / 2.0;
                                if shell.handle_mouse_down(
                                    latest_parent_size,
                                    x,
                                    y,
                                    &direct_summary,
                                    browser_mode != BrowserMode::Incognito,
                                    direct_child.as_mut(),
                                ) {
                                    hosted_smoke_action_requested = true;
                                    println!(
                                        "[hosted-direct-smoke] certificate action {} requested",
                                        action.label()
                                    );
                                    window.request_redraw();
                                }
                            }
                            let action_complete = match action {
                                HostedDirectSmokeAction::Back => {
                                    direct_summary.certificate_back_requested
                                }
                                HostedDirectSmokeAction::TrustOnce => {
                                    hosted_smoke_action_relaunched
                                        && direct_summary.first_present.is_some()
                                }
                                HostedDirectSmokeAction::TrustThisAppliance => {
                                    hosted_smoke_action_relaunched
                                        && direct_summary.first_present.is_some()
                                }
                            };
                            if hosted_smoke_action_requested && action_complete {
                                hosted_smoke_completed = true;
                                println!("[hosted-direct-smoke] parent shell ready");
                                println!(
                                    "[hosted-direct-smoke] certificate action {} completed",
                                    action.label()
                                );
                                if let Some(child) = direct_child.as_ref() {
                                    println!("[hosted-direct-smoke] child pid {}", child.child.id());
                                }
                                if let Some(first_present) = direct_summary.first_present.as_ref() {
                                    println!(
                                        "[hosted-direct-smoke] first direct present {first_present}"
                                    );
                                }
                                if let Some(url) = direct_summary.latest_url.as_ref() {
                                    println!("[hosted-direct-smoke] active url {url}");
                                }
                                if let Some(fingerprint) = hosted_smoke_certificate_fingerprint
                                    .as_ref()
                                    .or(direct_summary.certificate_fingerprint_sha256.as_ref())
                                {
                                    println!(
                                        "[hosted-direct-smoke] certificate fingerprint sha256 {fingerprint}"
                                    );
                                }
                                if let Ok(mut result) = hosted_smoke_result_for_loop.lock() {
                                    *result = Some(Ok(()));
                                }
                                terminate_child_process(&mut direct_child);
                                elwt.exit();
                                return;
                            }
                        } else if direct_summary.first_present.is_some() {
                            hosted_smoke_completed = true;
                            println!("[hosted-direct-smoke] parent shell ready");
                            if let Some(child) = direct_child.as_ref() {
                                println!("[hosted-direct-smoke] child pid {}", child.child.id());
                            }
                            if let Some(first_present) = direct_summary.first_present.as_ref() {
                                println!(
                                    "[hosted-direct-smoke] first direct present {first_present}"
                                );
                            }
                            if let Some(url) = direct_summary.latest_url.as_ref() {
                                println!("[hosted-direct-smoke] active url {url}");
                            }
                            if let Some(fingerprint) =
                                direct_summary.certificate_fingerprint_sha256.as_ref()
                            {
                                println!(
                                    "[hosted-direct-smoke] certificate fingerprint sha256 {fingerprint}"
                                );
                            }
                            if let Ok(mut result) = hosted_smoke_result_for_loop.lock() {
                                *result = Some(Ok(()));
                            }
                            terminate_child_process(&mut direct_child);
                            elwt.exit();
                            return;
                        }
                        if hosted_smoke_started.elapsed() >= timeout {
                            let error = format!(
                                "hosted direct smoke timed out after {}",
                                format_duration(timeout)
                            );
                            eprintln!("[hosted-direct-smoke] {error}");
                            if let Ok(mut result) = hosted_smoke_result_for_loop.lock() {
                                *result = Some(Err(error));
                            }
                            terminate_child_process(&mut direct_child);
                            elwt.exit();
                            return;
                        }
                    }
                    if let Some(child) = direct_child.as_mut() {
                        if resize_hosted_direct_child(
                            parent_hwnd,
                            child,
                            latest_parent_size,
                            HOSTED_DIRECT_CHROME_H,
                        ) {
                            window.request_redraw();
                        }
                    }
                    next_summary_refresh = now + HOSTED_DIRECT_SUMMARY_REFRESH;
                }
                elwt.set_control_flow(ControlFlow::WaitUntil(next_summary_refresh));
            }
            _ => {}
        }
    });
    event_loop_result.map_err(|error| format!("hosted direct event loop failed: {error}"))?;
    let hosted_result = hosted_smoke_result
        .lock()
        .map_err(|_| "hosted direct smoke result lock poisoned".to_string())?
        .take();
    match hosted_result {
        Some(result) => result,
        None => Ok(()),
    }
}

/// Logical (point) height of the three-row egui chrome (tab strip, navigation,
/// bookmarks). The embedded child is positioned below this height in physical
/// pixels (`points * scale_factor`), so the boundary stays aligned at any DPI.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub const HOSTED_DIRECT_CHROME_POINTS: f32 = 108.0;

/// Physical chrome height for the egui hosted-direct shell at the given scale.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn hosted_direct_egui_chrome_height_px(scale_factor: f32) -> u32 {
    (HOSTED_DIRECT_CHROME_POINTS * scale_factor).round().max(1.0) as u32
}

/// Deterministic favicon badge color from a tab URL/host (placeholder until real
/// favicons are fetched from the page).
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn hosted_direct_tab_favicon_rgb(url: &str) -> (u8, u8, u8) {
    let key = hosted_direct_tab_site(url);
    let hash = key
        .bytes()
        .fold(0u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(u32::from(byte)));
    (
        72 + ((hash >> 16) & 0x7f) as u8,
        72 + ((hash >> 8) & 0x7f) as u8,
        72 + (hash & 0x7f) as u8,
    )
}

/// A saved bookmark shown in the hosted-direct chrome bookmarks bar.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct HostedDirectBookmark {
    pub title: String,
    pub url: String,
}

/// An action requested from the egui chrome for the event loop to apply.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub enum HostedDirectChromeAction {
    Child(String),
    SelectTab(usize),
    CloseTab(usize),
    AddBookmark,
    CertificateTrustOnce,
    CertificateTrustAppliance,
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn hosted_direct_bookmarks_path() -> PathBuf {
    app_data_dir().join("browser").join("bookmarks.json")
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn load_hosted_direct_bookmarks() -> Vec<HostedDirectBookmark> {
    std::fs::read_to_string(hosted_direct_bookmarks_path())
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn save_hosted_direct_bookmarks(bookmarks: &[HostedDirectBookmark]) {
    let path = hosted_direct_bookmarks_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(bookmarks) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn truncate_label(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Position the embedded Servo child window just below the egui chrome, using the
/// chrome's physical height so the boundary tracks the display scale.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn position_hosted_direct_child(
    parent_hwnd: isize,
    child: &mut HostedDirectChild,
    parent_size: PhysicalSize<u32>,
    chrome_height_px: u32,
) -> bool {
    if child.window_hwnd.is_none() {
        child.window_hwnd = find_hosted_direct_child_window(parent_hwnd, child.child.id());
    }
    let Some(hwnd) = child.window_hwnd else {
        return false;
    };
    let chrome = chrome_height_px.min(parent_size.height);
    let width = parent_size.width.max(320) as i32;
    let height = parent_size.height.saturating_sub(chrome).max(240) as i32;
    unsafe {
        SetWindowPos(
            hwnd as HWND,
            std::ptr::null_mut(),
            0,
            chrome as i32,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        ) != 0
    }
}

/// A CPU-resident copy of an egui texture (font atlas / images), stored as
/// premultiplied sRGB `Color32` pixels so the software rasterizer can sample it.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
#[derive(Default)]
pub struct ChromeTextureStore {
    pub textures: std::collections::HashMap<egui::TextureId, ChromeTexture>,
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub struct ChromeTexture {
    pub w: usize,
    pub h: usize,
    pub px: Vec<egui::Color32>,
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
impl ChromeTextureStore {
    pub fn apply(&mut self, delta: &egui::TexturesDelta) {
        for (id, image_delta) in &delta.set {
            let (dw, dh, src): (usize, usize, Vec<egui::Color32>) = match &image_delta.image {
                egui::epaint::ImageData::Color(img) => {
                    (img.size[0], img.size[1], img.pixels.clone())
                }
                egui::epaint::ImageData::Font(img) => {
                    (img.size[0], img.size[1], img.srgba_pixels(None).collect())
                }
            };
            match image_delta.pos {
                None => {
                    self.textures.insert(
                        *id,
                        ChromeTexture {
                            w: dw,
                            h: dh,
                            px: src,
                        },
                    );
                }
                Some([x0, y0]) => {
                    if let Some(tex) = self.textures.get_mut(id) {
                        for row in 0..dh {
                            let ty = y0 + row;
                            if ty >= tex.h {
                                break;
                            }
                            for col in 0..dw {
                                let tx = x0 + col;
                                if tx < tex.w {
                                    tex.px[ty * tex.w + tx] = src[row * dw + col];
                                }
                            }
                        }
                    }
                }
            }
        }
        for id in &delta.free {
            self.textures.remove(id);
        }
    }

    pub fn get(&self, id: egui::TextureId) -> Option<&ChromeTexture> {
        self.textures.get(&id)
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn chrome_sample_bilinear(tex: &ChromeTexture, u: f32, v: f32) -> (f32, f32, f32, f32) {
    if tex.w == 0 || tex.h == 0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let fx = (u * tex.w as f32 - 0.5).clamp(0.0, (tex.w - 1) as f32);
    let fy = (v * tex.h as f32 - 0.5).clamp(0.0, (tex.h - 1) as f32);
    let x0 = fx.floor() as usize;
    let y0 = fy.floor() as usize;
    let x1 = (x0 + 1).min(tex.w - 1);
    let y1 = (y0 + 1).min(tex.h - 1);
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let texel = |x: usize, y: usize| {
        let c = tex.px[y * tex.w + x];
        (c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32)
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c00 = texel(x0, y0);
    let c10 = texel(x1, y0);
    let c01 = texel(x0, y1);
    let c11 = texel(x1, y1);
    let top = (
        lerp(c00.0, c10.0, tx),
        lerp(c00.1, c10.1, tx),
        lerp(c00.2, c10.2, tx),
        lerp(c00.3, c10.3, tx),
    );
    let bot = (
        lerp(c01.0, c11.0, tx),
        lerp(c01.1, c11.1, tx),
        lerp(c01.2, c11.2, tx),
        lerp(c01.3, c11.3, tx),
    );
    (
        lerp(top.0, bot.0, ty),
        lerp(top.1, bot.1, ty),
        lerp(top.2, bot.2, ty),
        lerp(top.3, bot.3, ty),
    )
}

#[inline]
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn chrome_edge(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)
}

/// Software-rasterize egui's tessellated output into a `0x00RRGGBB` pixel buffer.
/// Triangles are barycentric-filled with per-vertex colour, bilinear texture
/// sampling, and premultiplied-alpha "over" blending. Only rows `< max_y` are
/// touched so we never paint into the embedded child window's area below the
/// chrome. This keeps the chrome on a pure-CPU path (no D3D/WARP device), which
/// is essential on GPU-less hosts where wgpu's DX12+WARP rasterizer pool spins.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn rasterize_chrome(
    prims: &[egui::ClippedPrimitive],
    store: &ChromeTextureStore,
    buffer: &mut [u32],
    fb_w: usize,
    max_y: usize,
    ppp: f32,
) {
    for cp in prims {
        let mesh = match &cp.primitive {
            egui::epaint::Primitive::Mesh(mesh) => mesh,
            egui::epaint::Primitive::Callback(_) => continue,
        };
        if mesh.indices.is_empty() {
            continue;
        }
        let tex = store.get(mesh.texture_id);
        let clip_min_x = (cp.clip_rect.min.x * ppp).floor().max(0.0) as i32;
        let clip_min_y = (cp.clip_rect.min.y * ppp).floor().max(0.0) as i32;
        let clip_max_x = (cp.clip_rect.max.x * ppp).ceil().min(fb_w as f32) as i32;
        let clip_max_y = (cp.clip_rect.max.y * ppp).ceil().min(max_y as f32) as i32;
        if clip_max_x <= clip_min_x || clip_max_y <= clip_min_y {
            continue;
        }
        for tri in mesh.indices.chunks_exact(3) {
            let v0 = &mesh.vertices[tri[0] as usize];
            let v1 = &mesh.vertices[tri[1] as usize];
            let v2 = &mesh.vertices[tri[2] as usize];
            let p0 = (v0.pos.x * ppp, v0.pos.y * ppp);
            let p1 = (v1.pos.x * ppp, v1.pos.y * ppp);
            let p2 = (v2.pos.x * ppp, v2.pos.y * ppp);
            let bx0 = (p0.0.min(p1.0).min(p2.0)).floor() as i32;
            let bx1 = (p0.0.max(p1.0).max(p2.0)).ceil() as i32;
            let by0 = (p0.1.min(p1.1).min(p2.1)).floor() as i32;
            let by1 = (p0.1.max(p1.1).max(p2.1)).ceil() as i32;
            let bx0 = bx0.max(clip_min_x).max(0);
            let bx1 = bx1.min(clip_max_x);
            let by0 = by0.max(clip_min_y).max(0);
            let by1 = by1.min(clip_max_y);
            if bx1 <= bx0 || by1 <= by0 {
                continue;
            }
            let area = chrome_edge(p0, p1, p2);
            if area.abs() < 1.0e-6 {
                continue;
            }
            let inv_area = 1.0 / area;
            let (c0r, c0g, c0b, c0a) = (
                v0.color.r() as f32,
                v0.color.g() as f32,
                v0.color.b() as f32,
                v0.color.a() as f32,
            );
            let (c1r, c1g, c1b, c1a) = (
                v1.color.r() as f32,
                v1.color.g() as f32,
                v1.color.b() as f32,
                v1.color.a() as f32,
            );
            let (c2r, c2g, c2b, c2a) = (
                v2.color.r() as f32,
                v2.color.g() as f32,
                v2.color.b() as f32,
                v2.color.a() as f32,
            );
            for y in by0..by1 {
                for x in bx0..bx1 {
                    let px = x as f32 + 0.5;
                    let py = y as f32 + 0.5;
                    let w0 = chrome_edge(p1, p2, (px, py)) * inv_area;
                    let w1 = chrome_edge(p2, p0, (px, py)) * inv_area;
                    let w2 = chrome_edge(p0, p1, (px, py)) * inv_area;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let cr = w0 * c0r + w1 * c1r + w2 * c2r;
                    let cg = w0 * c0g + w1 * c1g + w2 * c2g;
                    let cb = w0 * c0b + w1 * c1b + w2 * c2b;
                    let ca = w0 * c0a + w1 * c1a + w2 * c2a;
                    let (sr, sg, sb, sa) = if let Some(tex) = tex {
                        let u = w0 * v0.uv.x + w1 * v1.uv.x + w2 * v2.uv.x;
                        let v = w0 * v0.uv.y + w1 * v1.uv.y + w2 * v2.uv.y;
                        let (tr, tg, tb, ta) = chrome_sample_bilinear(tex, u, v);
                        (
                            cr * tr / 255.0,
                            cg * tg / 255.0,
                            cb * tb / 255.0,
                            ca * ta / 255.0,
                        )
                    } else {
                        (cr, cg, cb, ca)
                    };
                    if sa <= 0.0 {
                        continue;
                    }
                    let idx = y as usize * fb_w + x as usize;
                    let dst = buffer[idx];
                    let dr = ((dst >> 16) & 0xff) as f32;
                    let dg = ((dst >> 8) & 0xff) as f32;
                    let db = (dst & 0xff) as f32;
                    let inv = 1.0 - sa / 255.0;
                    let nr = (sr + dr * inv).round().clamp(0.0, 255.0) as u32;
                    let ng = (sg + dg * inv).round().clamp(0.0, 255.0) as u32;
                    let nb = (sb + db * inv).round().clamp(0.0, 255.0) as u32;
                    buffer[idx] = (nr << 16) | (ng << 8) | nb;
                }
            }
        }
    }
}

/// GPU-backed chrome: egui rendered through wgpu. Selected only when a hardware
/// adapter is present (a real GPU), where wgpu is efficient and there is no WARP
/// rasterizer-pool idle spin.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub struct GpuChrome {
    pub _instance: wgpu::Instance,
    pub _adapter: wgpu::Adapter,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub renderer: egui_wgpu::Renderer,
}

/// CPU-backed chrome: egui software-rasterized into a softbuffer surface.
/// Selected on GPU-less hosts where wgpu would fall back to the WARP software
/// device, whose rasterizer thread pool spins even while idle.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub struct SoftwareChrome {
    // Declared before `_context` so the surface is dropped first.
    pub surface: Surface<Arc<Window>, Arc<Window>>,
    pub _context: Context<Arc<Window>>,
    pub textures: ChromeTextureStore,
    pub surface_size: Option<(u32, u32)>,
}

/// The selected chrome render backend. The web content is always rendered by the
/// Servo child (which uses the GPU when available); this only governs the chrome
/// strip drawn by the parent.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub enum ChromeBackend {
    Gpu(GpuChrome),
    Software(SoftwareChrome),
}

/// Choose the chrome render backend: prefer a hardware GPU (wgpu/DX12), and fall
/// back to a pure-CPU softbuffer surface when the only adapter is WARP (the
/// "Microsoft Basic Render Driver", reported as `DeviceType::Cpu`) or none. This
/// keeps GPU rendering on capable machines while avoiding the WARP idle spin on
/// GPU-less hosts (e.g. Windows Server / RDP).
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn init_chrome_backend(window: &Arc<Window>) -> Result<ChromeBackend, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        flags: wgpu::InstanceFlags::empty(),
        ..Default::default()
    });
    let surface = instance
        .create_surface(window.clone())
        .map_err(|error| format!("wgpu surface creation failed: {error}"))?;
    // Only a real GPU qualifies for the wgpu path; WARP reports `DeviceType::Cpu`.
    let hardware = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .filter(|adapter| adapter.get_info().device_type != wgpu::DeviceType::Cpu);

    if let Some(adapter) = hardware {
        let info = adapter.get_info();
        println!(
            "[hosted-direct] GPU chrome via wgpu adapter {} backend={:?} type={:?}",
            info.name, info.backend, info.device_type
        );
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("hosted-direct-chrome"),
                required_features: wgpu::Features::empty(),
                required_limits:
                    wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            },
            None,
        ))
        .map_err(|error| format!("wgpu device request failed: {error}"))?;
        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        // egui expects a non-sRGB (linear) target so it can encode gamma itself.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let renderer = egui_wgpu::Renderer::new(&device, surface_format, None, 1);
        Ok(ChromeBackend::Gpu(GpuChrome {
            _instance: instance,
            _adapter: adapter,
            surface,
            device,
            queue,
            config,
            renderer,
        }))
    } else {
        println!(
            "[hosted-direct] software chrome: no hardware GPU adapter (WARP/none); using softbuffer"
        );
        // Release the probe's wgpu surface before binding softbuffer to the window.
        drop(surface);
        drop(instance);
        let context = Context::new(window.clone())
            .map_err(|error| format!("softbuffer context initialization failed: {error}"))?;
        let sb_surface = Surface::new(&context, window.clone())
            .map_err(|error| format!("softbuffer surface initialization failed: {error}"))?;
        Ok(ChromeBackend::Software(SoftwareChrome {
            surface: sb_surface,
            _context: context,
            textures: ChromeTextureStore::default(),
            surface_size: None,
        }))
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
impl ChromeBackend {
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if let ChromeBackend::Gpu(gpu) = self {
            if new_size.width > 0 && new_size.height > 0 {
                gpu.config.width = new_size.width;
                gpu.config.height = new_size.height;
                gpu.surface.configure(&gpu.device, &gpu.config);
            }
        }
        // The software backend resizes its softbuffer surface lazily in `render`.
    }

    pub fn render(
        &mut self,
        egui_ctx: &egui::Context,
        shapes: Vec<egui::epaint::ClippedShape>,
        textures_delta: egui::TexturesDelta,
        pixels_per_point: f32,
        size: PhysicalSize<u32>,
        full: bool,
    ) {
        match self {
            ChromeBackend::Gpu(gpu) => {
                let tris = egui_ctx.tessellate(shapes, pixels_per_point);
                for (id, image_delta) in &textures_delta.set {
                    gpu.renderer
                        .update_texture(&gpu.device, &gpu.queue, *id, image_delta);
                }
                let screen_descriptor = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [gpu.config.width, gpu.config.height],
                    pixels_per_point,
                };
                let mut encoder =
                    gpu.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("hosted-direct-chrome-encoder"),
                        });
                gpu.renderer.update_buffers(
                    &gpu.device,
                    &gpu.queue,
                    &mut encoder,
                    &tris,
                    &screen_descriptor,
                );
                let frame = match gpu.surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(_) => {
                        gpu.surface.configure(&gpu.device, &gpu.config);
                        return;
                    }
                };
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("hosted-direct-chrome-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.043,
                                    g: 0.075,
                                    b: 0.102,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    gpu.renderer.render(&mut pass, &tris, &screen_descriptor);
                }
                for id in &textures_delta.free {
                    gpu.renderer.free_texture(id);
                }
                gpu.queue.submit(Some(encoder.finish()));
                frame.present();
            }
            ChromeBackend::Software(sw) => {
                let tris = egui_ctx.tessellate(shapes, pixels_per_point);
                sw.textures.apply(&textures_delta);
                let fb_w = size.width.max(1);
                let fb_h = size.height.max(1);
                if sw.surface_size != Some((fb_w, fb_h)) {
                    if let (Some(w), Some(h)) = (NonZeroU32::new(fb_w), NonZeroU32::new(fb_h)) {
                        if sw.surface.resize(w, h).is_ok() {
                            sw.surface_size = Some((fb_w, fb_h));
                        }
                    }
                }
                // Only paint the top chrome strip; everything below belongs to the
                // embedded Servo child window, so we present just that rect (a full
                // softbuffer present would flicker the child).
                let chrome_px = ((HOSTED_DIRECT_CHROME_POINTS * pixels_per_point).round() as u32)
                    .clamp(1, fb_h);
                // Full-window egui (e.g. the retro proof) clears/rasterizes/presents
                // the whole surface; the hosted-direct chrome paints only the top
                // strip so the embedded Servo child below is never overdrawn.
                let paint_h = if full { fb_h } else { chrome_px };
                if let Ok(mut buffer) = sw.surface.buffer_mut() {
                    let buf_len = buffer.len();
                    let strip = (paint_h as usize)
                        .saturating_mul(fb_w as usize)
                        .min(buf_len);
                    for pixel in buffer.iter_mut().take(strip) {
                        *pixel = CHROME_BG;
                    }
                    rasterize_chrome(
                        &tris,
                        &sw.textures,
                        &mut buffer,
                        fb_w as usize,
                        paint_h as usize,
                        pixels_per_point,
                    );
                    match (NonZeroU32::new(fb_w), NonZeroU32::new(paint_h)) {
                        (Some(width), Some(height)) => {
                            let _ = buffer.present_with_damage(&[softbuffer::Rect {
                                x: 0,
                                y: 0,
                                width,
                                height,
                            }]);
                        }
                        _ => {
                            let _ = buffer.present();
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostedDirectLogSummary {
    pub first_present: Option<String>,
    pub latest_load: Option<String>,
    pub latest_swap: Option<String>,
    pub latest_input: Option<String>,
    pub latest_url: Option<String>,
    pub certificate_fingerprint_sha256: Option<String>,
    pub active_title: Option<String>,
    pub active_tab_index: Option<usize>,
    pub tab_count: Option<usize>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub latest_load_complete: bool,
    pub slow_frames: usize,
    pub max_frame_ms: Option<f64>,
    pub max_frame_label: Option<String>,
    pub max_paint_ms: Option<f64>,
    pub max_paint_label: Option<String>,
    pub latest_resource_audit: Option<String>,
    pub certificate_back_requested: bool,
    pub certificate_trust_once_requested: bool,
    pub certificate_trust_this_appliance_requested: bool,
    pub tabs: Vec<HostedDirectTab>,
}

/// One tab reported by the hosted-direct child for the chrome tab strip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostedDirectTab {
    pub url: String,
    pub active: bool,
}

/// The site label shown on a tab: the host without a leading `www.`, or "New Tab".
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn hosted_direct_tab_site(url: &str) -> String {
    if url.is_empty() || url == "about:blank" {
        return "New Tab".to_string();
    }
    let host = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_string()));
    match host {
        Some(host) => host
            .strip_prefix("www.")
            .unwrap_or(host.as_str())
            .to_string(),
        None => "New Tab".to_string(),
    }
}

#[derive(Default)]
pub struct HostedDirectLogMonitor {
    pub last_len: u64,
    pub last_modified: Option<SystemTime>,
}

impl HostedDirectLogMonitor {
    pub fn refresh(&mut self, path: &Path) -> Option<HostedDirectLogSummary> {
        let metadata = std::fs::metadata(path).ok()?;
        let len = metadata.len();
        let modified = metadata.modified().ok();
        if len == self.last_len && modified == self.last_modified {
            return None;
        }
        self.last_len = len;
        self.last_modified = modified;
        Some(parse_hosted_direct_log_summary(path))
    }
}

impl HostedDirectLogSummary {
    pub fn compact_perf_status(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!(
            "first {}",
            self.first_present.as_deref().unwrap_or("pending")
        ));
        if let Some(load) = self.latest_load.as_deref() {
            parts.push(format!("load {load}"));
        }
        if let Some(input) = self.latest_input.as_deref() {
            parts.push(format!("input {input}"));
        }
        if self.slow_frames > 0 {
            parts.push(format!(
                "slow {} paint {}",
                self.slow_frames,
                self.max_paint_label.as_deref().unwrap_or("?")
            ));
        }
        parts.join(" | ")
    }

    pub fn compact_audit_status(&self) -> Option<&str> {
        self.latest_resource_audit.as_deref()
    }

    pub fn certificate_warning_active(&self) -> bool {
        self.certificate_fingerprint_sha256.is_some()
            || self
                .active_title
                .as_deref()
                .is_some_and(|title| title.eq_ignore_ascii_case("Certificate error"))
    }

    pub fn compact_certificate_status(&self) -> Option<String> {
        if let Some(fingerprint) = self.certificate_fingerprint_sha256.as_deref() {
            return Some(format!("cert {}", compact_fingerprint(fingerprint)));
        }
        self.certificate_warning_active()
            .then(|| "cert blocked".to_string())
    }

    pub fn load_progress(&self) -> f32 {
        if self.latest_load_complete {
            1.0
        } else if self.latest_load.is_some() {
            0.62
        } else if self.first_present.is_some() {
            0.32
        } else {
            0.12
        }
    }

    pub fn is_loading(&self) -> bool {
        !self.latest_load_complete
    }

    pub fn can_switch_tabs(&self) -> bool {
        self.tab_count.unwrap_or(1) > 1
    }
}

#[derive(Clone, Copy)]
pub struct HostedDirectChromeRects {
    pub back: Rect,
    pub forward: Rect,
    pub reload: Rect,
    pub new_tab: Rect,
    pub previous_tab: Rect,
    pub next_tab: Rect,
    pub close_tab: Rect,
    pub certificate_back: Rect,
    pub trust_once: Rect,
    pub trust_appliance: Rect,
    pub address: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedDirectCertificateTrust {
    Once,
    Remember,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedDirectSmokeAction {
    Back,
    TrustOnce,
    TrustThisAppliance,
}

impl HostedDirectSmokeAction {
    pub fn parse_arg(value: Option<String>) -> Result<Option<Self>, String> {
        let Some(value) = value else {
            return Ok(None);
        };
        match value.trim().to_ascii_lowercase().as_str() {
            "back" => Ok(Some(Self::Back)),
            "once" | "trust-once" | "trust_once" => Ok(Some(Self::TrustOnce)),
            "trust" | "remember" | "trust-this-appliance" | "trust_this_appliance" => {
                Ok(Some(Self::TrustThisAppliance))
            }
            other => Err(format!(
                "unsupported hosted direct certificate action '{other}'; expected back, once, or trust"
            )),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Back => "back",
            Self::TrustOnce => "once",
            Self::TrustThisAppliance => "trust",
        }
    }

    pub fn rect(self, rects: HostedDirectChromeRects) -> Rect {
        match self {
            Self::Back => rects.certificate_back,
            Self::TrustOnce => rects.trust_once,
            Self::TrustThisAppliance => rects.trust_appliance,
        }
    }
}

pub struct HostedDirectShellState {
    pub target: String,
    pub address_input: String,
    pub address_cursor: usize,
    pub address_focused: bool,
    pub address_replace_on_text: bool,
    pub pending_certificate_trust: Option<HostedDirectCertificateTrust>,
    pub cursor_position: Option<(f64, f64)>,
    pub modifiers: ModifiersState,
}

impl HostedDirectShellState {
    pub fn new(target: String) -> Self {
        Self {
            address_input: target.clone(),
            address_cursor: target.len(),
            target,
            address_focused: false,
            address_replace_on_text: false,
            pending_certificate_trust: None,
            cursor_position: None,
            modifiers: ModifiersState::empty(),
        }
    }

    pub fn handle_mouse_down(
        &mut self,
        size: PhysicalSize<u32>,
        x: f64,
        y: f64,
        summary: &HostedDirectLogSummary,
        persistent_trust_allowed: bool,
        child: Option<&mut HostedDirectChild>,
    ) -> bool {
        let rects = hosted_direct_chrome_rects(size);
        if rects.address.contains(x, y) {
            self.address_input = self.target.clone();
            self.address_cursor = self.address_input.len();
            self.address_focused = true;
            self.address_replace_on_text = true;
            return true;
        }
        if rects.back.contains(x, y) {
            self.address_focused = false;
            if summary.can_go_back {
                let _ = send_hosted_direct_command(child, "back");
            }
            return true;
        }
        if rects.forward.contains(x, y) {
            self.address_focused = false;
            if summary.can_go_forward {
                let _ = send_hosted_direct_command(child, "forward");
            }
            return true;
        }
        if rects.reload.contains(x, y) {
            self.address_focused = false;
            let _ = send_hosted_direct_command(child, "reload");
            return true;
        }
        if rects.new_tab.contains(x, y) {
            self.address_input.clear();
            self.address_cursor = 0;
            self.address_focused = true;
            self.address_replace_on_text = false;
            let _ = send_hosted_direct_command(child, "new-tab");
            return true;
        }
        if rects.previous_tab.contains(x, y) {
            self.address_focused = false;
            if summary.can_switch_tabs() {
                let _ = send_hosted_direct_command(child, "previous-tab");
            }
            return true;
        }
        if rects.next_tab.contains(x, y) {
            self.address_focused = false;
            if summary.can_switch_tabs() {
                let _ = send_hosted_direct_command(child, "next-tab");
            }
            return true;
        }
        if rects.close_tab.contains(x, y) {
            self.address_focused = false;
            let _ = send_hosted_direct_command(child, "close-tab");
            return true;
        }
        if summary.certificate_warning_active() && rects.certificate_back.contains(x, y) {
            self.address_focused = false;
            let _ = send_hosted_direct_command(child, "certificate-go-back");
            return true;
        }
        let certificate_trust_ready = summary.certificate_fingerprint_sha256.is_some();
        if certificate_trust_ready && rects.trust_once.contains(x, y) {
            self.address_focused = false;
            self.pending_certificate_trust = Some(HostedDirectCertificateTrust::Once);
            let _ = send_hosted_direct_command(child, "trust-once");
            return true;
        }
        if certificate_trust_ready
            && persistent_trust_allowed
            && rects.trust_appliance.contains(x, y)
        {
            self.address_focused = false;
            self.pending_certificate_trust = Some(HostedDirectCertificateTrust::Remember);
            let _ = send_hosted_direct_command(child, "trust-this-appliance");
            return true;
        }
        if y < HOSTED_DIRECT_CHROME_H as f64 {
            self.address_focused = false;
            return true;
        }
        false
    }

    pub fn take_pending_certificate_trust(&mut self) -> Option<HostedDirectCertificateTrust> {
        self.pending_certificate_trust.take()
    }

    pub fn handle_keyboard(
        &mut self,
        event: KeyEvent,
        child: Option<&mut HostedDirectChild>,
    ) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        if self.modifiers.control_key() {
            if let Key::Character(value) = &event.logical_key {
                if value.eq_ignore_ascii_case("l") {
                    self.address_input = self.target.clone();
                    self.address_cursor = self.address_input.len();
                    self.address_focused = true;
                    self.address_replace_on_text = true;
                    return true;
                }
                if self.address_focused && value.eq_ignore_ascii_case("a") {
                    self.address_replace_on_text = true;
                    self.address_cursor = self.address_input.len();
                    return true;
                }
                if value.eq_ignore_ascii_case("t") {
                    self.address_input.clear();
                    self.address_cursor = 0;
                    self.address_focused = true;
                    self.address_replace_on_text = false;
                    let _ = send_hosted_direct_command(child, "new-tab");
                    return true;
                }
                if value.eq_ignore_ascii_case("w") {
                    self.address_focused = false;
                    self.address_replace_on_text = false;
                    let _ = send_hosted_direct_command(child, "close-tab");
                    return true;
                }
            }
            if let Key::Named(NamedKey::Tab) = &event.logical_key {
                self.address_focused = false;
                self.address_replace_on_text = false;
                let command = if self.modifiers.shift_key() {
                    "previous-tab"
                } else {
                    "next-tab"
                };
                let _ = send_hosted_direct_command(child, command);
                return true;
            }
        }
        if !self.address_focused {
            return false;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => {
                let target = self.address_input.trim().to_string();
                if !target.is_empty() {
                    self.target = target.clone();
                    self.address_input = target.clone();
                    self.address_cursor = self.address_input.len();
                    self.address_focused = false;
                    self.address_replace_on_text = false;
                    let _ = send_hosted_direct_command(child, &format!("navigate {target}"));
                    return true;
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.address_input = self.target.clone();
                self.address_cursor = self.address_input.len();
                self.address_focused = false;
                self.address_replace_on_text = false;
                true
            }
            Key::Named(NamedKey::Backspace) => {
                if self.address_replace_on_text {
                    self.address_input.clear();
                    self.address_cursor = 0;
                    self.address_replace_on_text = false;
                } else if self.address_cursor > 0 {
                    let previous = previous_char_boundary(&self.address_input, self.address_cursor);
                    self.address_input.drain(previous..self.address_cursor);
                    self.address_cursor = previous;
                }
                true
            }
            Key::Named(NamedKey::Delete) => {
                if self.address_replace_on_text {
                    self.address_input.clear();
                    self.address_cursor = 0;
                    self.address_replace_on_text = false;
                } else if self.address_cursor < self.address_input.len() {
                    let next = next_char_boundary(&self.address_input, self.address_cursor);
                    self.address_input.drain(self.address_cursor..next);
                }
                true
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.address_replace_on_text = false;
                self.address_cursor =
                    previous_char_boundary(&self.address_input, self.address_cursor);
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.address_replace_on_text = false;
                self.address_cursor = next_char_boundary(&self.address_input, self.address_cursor);
                true
            }
            Key::Named(NamedKey::Home) => {
                self.address_replace_on_text = false;
                self.address_cursor = 0;
                true
            }
            Key::Named(NamedKey::End) => {
                self.address_replace_on_text = false;
                self.address_cursor = self.address_input.len();
                true
            }
            Key::Named(NamedKey::Space) => {
                self.push_address_text(" ");
                true
            }
            Key::Character(value) if !self.modifiers.control_key() => {
                if value.chars().all(|ch| !ch.is_control()) {
                    self.push_address_text(value);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub fn push_address_text(&mut self, value: &str) {
        if self.address_replace_on_text {
            self.address_input.clear();
            self.address_cursor = 0;
            self.address_replace_on_text = false;
        }
        self.address_cursor =
            clamp_char_boundary(&self.address_input, self.address_cursor);
        self.address_input.insert_str(self.address_cursor, value);
        // IME commits and pasted text can be multi-byte; keep the cursor on a
        // scalar boundary so `address_input[..cursor]` never panics.
        self.address_cursor = clamp_char_boundary(
            &self.address_input,
            self.address_cursor.saturating_add(value.len()),
        );
    }

    pub fn handle_text_commit(&mut self, text: String) -> bool {
        if !self.address_focused || text.is_empty() {
            return false;
        }
        if text.chars().all(|ch| !ch.is_control()) {
            self.push_address_text(&text);
            return true;
        }
        false
    }

    pub fn address_display_text(&self, max_chars: usize) -> String {
        if !self.address_focused {
            return truncate(&self.target, max_chars);
        }
        let cursor = clamp_char_boundary(
            &self.address_input,
            self.address_cursor.min(self.address_input.len()),
        );
        let mut display = String::with_capacity(self.address_input.len() + 1);
        display.push_str(&self.address_input[..cursor]);
        display.push('_');
        display.push_str(&self.address_input[cursor..]);
        if display.chars().count() <= max_chars {
            return display;
        }
        if max_chars <= 3 {
            return truncate(&display, max_chars);
        }
        let cursor_chars = self.address_input[..cursor].chars().count();
        let start = cursor_chars.saturating_sub(max_chars.saturating_sub(3));
        let visible: String = display
            .chars()
            .skip(start)
            .take(max_chars.saturating_sub(3))
            .collect();
        format!("...{visible}")
    }
}

pub fn previous_char_boundary(value: &str, index: usize) -> usize {
    let index = clamp_char_boundary(value, index.min(value.len()));
    value[..index]
        .char_indices()
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

pub fn clamp_char_boundary(value: &str, index: usize) -> usize {
    if index >= value.len() {
        return value.len();
    }
    if value.is_char_boundary(index) {
        return index;
    }
    let mut i = index;
    while i > 0 && !value.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn next_char_boundary(value: &str, index: usize) -> usize {
    let index = clamp_char_boundary(value, index.min(value.len()));
    if index >= value.len() {
        return value.len();
    }
    value[index..]
        .chars()
        .next()
        .map(|ch| index + ch.len_utf8())
        .unwrap_or(value.len())
}

pub struct HostedDirectChild {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub log_path: PathBuf,
    #[cfg(all(target_os = "windows", feature = "servo-backend"))]
    pub window_hwnd: Option<isize>,
    #[cfg(all(target_os = "windows", feature = "servo-backend"))]
    pub last_resized_parent_size: Option<PhysicalSize<u32>>,
}

pub fn spawn_hosted_direct_child(
    parent_hwnd: isize,
    parent_size: PhysicalSize<u32>,
    target: &str,
    browser_mode: BrowserMode,
    certificate_path: Option<&Path>,
    local_appliance_cert_fingerprint: Option<&str>,
    remember_local_appliance_cert: Option<&str>,
    allow_insecure_local_tls: bool,
    direct_resource_audit: bool,
    chrome_height_px: u32,
) -> Result<HostedDirectChild, String> {
    let (_, embed_y, width, height) = hosted_direct_child_bounds(parent_size, chrome_height_px);
    let log_path = hosted_direct_child_log_path()?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| {
            format!(
                "failed to open hosted direct log {}: {error}",
                log_path.display()
            )
        })?;
    let log_err = log
        .try_clone()
        .map_err(|error| format!("failed to clone hosted direct log handle: {error}"))?;
    let mut command = Command::new(env::current_exe().map_err(|error| error.to_string())?);
    command
        .arg("--browser-mode")
        .arg(browser_mode.cli_arg())
        .arg("--render-path")
        .arg("direct")
        .arg("--raw-direct-window")
        .arg("--start")
        .arg(target)
        .arg("--embed-parent-hwnd")
        .arg(parent_hwnd.to_string())
        .arg("--embed-x")
        .arg("0")
        .arg("--embed-y")
        .arg(embed_y.to_string())
        .arg("--embed-width")
        .arg(width.to_string())
        .arg("--embed-height")
        .arg(height.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    if let Some(path) = certificate_path {
        command.arg("--certificate-path").arg(path);
    }
    if let Some(fingerprint) = local_appliance_cert_fingerprint {
        command
            .arg("--local-appliance-cert-fingerprint")
            .arg(fingerprint);
    }
    if let Some(origin) = remember_local_appliance_cert {
        command.arg("--remember-local-appliance-cert").arg(origin);
    }
    if allow_insecure_local_tls {
        command.arg("--allow-insecure-local-tls");
    }
    if direct_resource_audit {
        command.arg("--direct-resource-audit");
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to launch hosted direct Servo child: {error}"))?;
    let stdin = child.stdin.take();
    println!(
        "[hosted-direct] child pid {} log {}",
        child.id(),
        log_path.display()
    );
    Ok(HostedDirectChild {
        child,
        stdin,
        log_path,
        #[cfg(all(target_os = "windows", feature = "servo-backend"))]
        window_hwnd: None,
        #[cfg(all(target_os = "windows", feature = "servo-backend"))]
        last_resized_parent_size: None,
    })
}

/// Relaunch the embedded Servo child after a certificate trust decision. The
/// child exits once the trust action completes; the parent reads the log
/// summary for the fingerprint and target URL, then spawns a replacement child
/// with the appropriate trust flags (session-only TLS bypass vs persisted trust).
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn hosted_direct_cert_trust_launch_flags(
    trust: HostedDirectCertificateTrust,
    fingerprint: String,
    stored_fingerprint: &mut Option<String>,
) -> bool {
    match trust {
        HostedDirectCertificateTrust::Once => true,
        HostedDirectCertificateTrust::Remember => {
            *stored_fingerprint = Some(fingerprint);
            false
        }
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn relaunch_hosted_direct_child_after_certificate_trust(
    parent_hwnd: isize,
    parent_size: PhysicalSize<u32>,
    summary: &HostedDirectLogSummary,
    fallback_target: &str,
    trust: HostedDirectCertificateTrust,
    browser_mode: BrowserMode,
    certificate_path: Option<&Path>,
    hosted_local_appliance_cert_fingerprint: &mut Option<String>,
    remember_local_appliance_cert: Option<&str>,
    direct_resource_audit: bool,
    chrome_height_px: u32,
) -> Result<(HostedDirectChild, String), String> {
    let fingerprint = summary
        .certificate_fingerprint_sha256
        .clone()
        .ok_or_else(|| {
            "child exited after certificate trust request without a fingerprint".to_string()
        })?;
    let allow_insecure_local_tls = hosted_direct_cert_trust_launch_flags(
        trust,
        fingerprint,
        hosted_local_appliance_cert_fingerprint,
    );
    let restart_target = summary
        .latest_url
        .as_deref()
        .unwrap_or(fallback_target)
        .to_string();
    let child = spawn_hosted_direct_child(
        parent_hwnd,
        parent_size,
        &restart_target,
        browser_mode,
        certificate_path,
        hosted_local_appliance_cert_fingerprint.as_deref(),
        remember_local_appliance_cert,
        allow_insecure_local_tls,
        direct_resource_audit,
        chrome_height_px,
    )?;
    Ok((child, restart_target))
}

pub fn send_hosted_direct_command(child: Option<&mut HostedDirectChild>, command: &str) -> bool {
    let Some(child) = child else {
        return false;
    };
    let Some(stdin) = child.stdin.as_mut() else {
        return false;
    };
    if command.contains('\n') || command.contains('\r') {
        return false;
    }
    writeln!(stdin, "{command}")
        .and_then(|_| stdin.flush())
        .is_ok()
}

pub fn poll_hosted_direct_child_exit(
    child: &mut Option<HostedDirectChild>,
) -> Option<(ExitStatus, PathBuf)> {
    let status = child.as_mut()?.child.try_wait().ok()??;
    let child = child.take()?;
    Some((status, child.log_path))
}

pub fn hosted_direct_chrome_rects(size: PhysicalSize<u32>) -> HostedDirectChromeRects {
    let y = 44;
    let h = 20;
    let button_w = 30;
    let gap = 6;
    let back = Rect {
        x: 18,
        y,
        w: button_w,
        h,
    };
    let forward = Rect {
        x: back.x + back.w + gap,
        y,
        w: button_w,
        h,
    };
    let reload = Rect {
        x: forward.x + forward.w + gap,
        y,
        w: button_w,
        h,
    };
    let new_tab = Rect {
        x: reload.x + reload.w + gap,
        y,
        w: button_w,
        h,
    };
    let previous_tab = Rect {
        x: new_tab.x + new_tab.w + gap,
        y,
        w: button_w,
        h,
    };
    let next_tab = Rect {
        x: previous_tab.x + previous_tab.w + gap,
        y,
        w: button_w,
        h,
    };
    let close_tab = Rect {
        x: next_tab.x + next_tab.w + gap,
        y,
        w: button_w,
        h,
    };
    let certificate_back = Rect {
        x: size.width.saturating_sub(250),
        y,
        w: 56,
        h,
    };
    let trust_once = Rect {
        x: certificate_back.x + certificate_back.w + gap,
        y,
        w: 68,
        h,
    };
    let trust_appliance = Rect {
        x: trust_once.x + trust_once.w + gap,
        y,
        w: 96,
        h,
    };
    let address_x = close_tab.x + close_tab.w + 10;
    let reserved_right = 286;
    let address_w = size
        .width
        .saturating_sub(address_x)
        .saturating_sub(reserved_right)
        .max(160);
    HostedDirectChromeRects {
        back,
        forward,
        reload,
        new_tab,
        previous_tab,
        next_tab,
        close_tab,
        certificate_back,
        trust_once,
        trust_appliance,
        address: Rect {
            x: address_x,
            y,
            w: address_w,
            h,
        },
    }
}

pub fn hosted_direct_child_bounds(
    parent_size: PhysicalSize<u32>,
    chrome_height_px: u32,
) -> (i32, i32, u32, u32) {
    let chrome = chrome_height_px.min(parent_size.height);
    (
        0,
        chrome as i32,
        parent_size.width.max(320),
        parent_size
            .height
            .saturating_sub(chrome)
            .max(240),
    )
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn resize_hosted_direct_child(
    parent_hwnd: isize,
    child: &mut HostedDirectChild,
    parent_size: PhysicalSize<u32>,
    chrome_height_px: u32,
) -> bool {
    if child.window_hwnd.is_some() && child.last_resized_parent_size == Some(parent_size) {
        return false;
    }
    if child.window_hwnd.is_none() {
        child.window_hwnd = find_hosted_direct_child_window(parent_hwnd, child.child.id());
    }
    let Some(hwnd) = child.window_hwnd else {
        return false;
    };
    let (x, y, width, height) = hosted_direct_child_bounds(parent_size, chrome_height_px);
    let resized = unsafe {
        SetWindowPos(
            hwnd as HWND,
            std::ptr::null_mut(),
            x,
            y,
            width as i32,
            height as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        ) != 0
    };
    if resized {
        child.last_resized_parent_size = Some(parent_size);
    }
    resized
}

#[cfg(not(all(target_os = "windows", feature = "servo-backend")))]
pub fn resize_hosted_direct_child(
    _parent_hwnd: isize,
    _child: &mut HostedDirectChild,
    _parent_size: PhysicalSize<u32>,
    _chrome_height_px: u32,
) -> bool {
    false
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub struct HostedDirectChildWindowSearch {
    pub pid: u32,
    pub hwnd: Option<isize>,
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn find_hosted_direct_child_window(parent_hwnd: isize, child_pid: u32) -> Option<isize> {
    let mut search = HostedDirectChildWindowSearch {
        pid: child_pid,
        hwnd: None,
    };
    unsafe {
        EnumChildWindows(
            parent_hwnd as HWND,
            Some(enum_hosted_direct_child_window),
            &mut search as *mut HostedDirectChildWindowSearch as LPARAM,
        );
    }
    search.hwnd
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
unsafe extern "system" fn enum_hosted_direct_child_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    let search = &mut *(lparam as *mut HostedDirectChildWindowSearch);
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == search.pid {
        search.hwnd = Some(hwnd as isize);
        return 0;
    }
    1
}

pub fn hosted_direct_child_log_path() -> Result<PathBuf, String> {
    let dir = app_data_dir().join("browser").join(HOSTED_DIRECT_LOG_DIR);
    std::fs::create_dir_all(&dir).map_err(|error| {
        format!(
            "failed to create hosted direct log directory {}: {error}",
            dir.display()
        )
    })?;
    Ok(dir.join(format!(
        "servo-child-{}.log",
        Utc::now().format("%Y%m%d-%H%M%S-%3f")
    )))
}

pub fn terminate_child_process(child: &mut Option<HostedDirectChild>) {
    if let Some(mut child) = child.take() {
        let _ = child.child.kill();
        let _ = child.child.wait();
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub fn focus_hosted_direct_parent_window(window: &Window, parent_hwnd: isize) {
    window.focus_window();
    unsafe {
        let hwnd = parent_hwnd as HWND;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        SetActiveWindow(hwnd);
        SetFocus(hwnd);
    }
}

#[cfg(not(all(target_os = "windows", feature = "servo-backend")))]
pub fn focus_hosted_direct_parent_window(window: &Window, _parent_hwnd: isize) {
    window.focus_window();
}

pub fn draw_hosted_direct_shell(
    window: &Window,
    surface: &mut Surface<Arc<Window>, Arc<Window>>,
    surface_size: &mut PhysicalSize<u32>,
    browser_mode: BrowserMode,
    shell: &HostedDirectShellState,
    child_pid: Option<u32>,
    child_log_path: Option<&Path>,
    summary: &HostedDirectLogSummary,
) -> Result<(), String> {
    let size = window.inner_size();
    if size.width == 0 || size.height == 0 {
        return Ok(());
    }
    if *surface_size != size {
        surface
            .resize(
                NonZeroU32::new(size.width.max(1)).expect("width is nonzero"),
                NonZeroU32::new(size.height.max(1)).expect("height is nonzero"),
            )
            .map_err(|error| error.to_string())?;
        *surface_size = size;
    }
    let mut buffer = surface.buffer_mut().map_err(|error| error.to_string())?;
    buffer.fill(BG);
    fill_rect(
        &mut buffer,
        size.width,
        size.height,
        Rect {
            x: 0,
            y: 0,
            w: size.width,
            h: HOSTED_DIRECT_CHROME_H,
        },
        PANEL_DARK,
    );
    fill_rect(
        &mut buffer,
        size.width,
        size.height,
        Rect {
            x: 0,
            y: HOSTED_DIRECT_CHROME_H.saturating_sub(1),
            w: size.width,
            h: 1,
        },
        BORDER,
    );
    draw_text(
        &mut buffer,
        size.width,
        size.height,
        18,
        16,
        "SEXTANT DIRECT",
        TEXT,
        2,
    );
    draw_text(
        &mut buffer,
        size.width,
        size.height,
        220,
        18,
        browser_mode.status(),
        TEXT_DIM,
        1,
    );
    let status = child_pid
        .map(|pid| format!("hosted servo pid {pid}"))
        .unwrap_or_else(|| "hosted servo starting".to_string());
    let log_status = child_log_path
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(|name| format!("log {name}"))
        .unwrap_or_default();
    let summary_x = 300;
    let summary_max_chars = size.width.saturating_sub(545) / (GLYPH_W + GLYPH_GAP);
    if HOSTED_DIRECT_DEBUG_TELEMETRY && summary_max_chars >= 18 {
        let perf_text = summary.compact_perf_status();
        draw_text(
            &mut buffer,
            size.width,
            size.height,
            summary_x,
            18,
            &truncate(&perf_text, summary_max_chars.min(82) as usize),
            if summary.slow_frames > 0 {
                STATUS_WARN
            } else {
                TEXT_DIM
            },
            1,
        );
        if let Some(audit_text) = summary.compact_audit_status() {
            draw_text(
                &mut buffer,
                size.width,
                size.height,
                summary_x,
                30,
                &truncate(audit_text, summary_max_chars.min(82) as usize),
                TEXT_DIM,
                1,
            );
        }
    }
    let rects = hosted_direct_chrome_rects(size);
    draw_hosted_direct_button(
        &mut buffer,
        size.width,
        size.height,
        rects.back,
        "<",
        summary.can_go_back,
    );
    draw_hosted_direct_button(
        &mut buffer,
        size.width,
        size.height,
        rects.forward,
        ">",
        summary.can_go_forward,
    );
    draw_hosted_direct_button(
        &mut buffer,
        size.width,
        size.height,
        rects.reload,
        "R",
        true,
    );
    draw_hosted_direct_button(
        &mut buffer,
        size.width,
        size.height,
        rects.new_tab,
        "+",
        true,
    );
    draw_hosted_direct_button(
        &mut buffer,
        size.width,
        size.height,
        rects.previous_tab,
        "T-",
        summary.can_switch_tabs(),
    );
    draw_hosted_direct_button(
        &mut buffer,
        size.width,
        size.height,
        rects.next_tab,
        "T+",
        summary.can_switch_tabs(),
    );
    draw_hosted_direct_button(
        &mut buffer,
        size.width,
        size.height,
        rects.close_tab,
        "X",
        true,
    );
    fill_rect(
        &mut buffer,
        size.width,
        size.height,
        rects.address,
        if shell.address_focused {
            FIELD_FOCUS
        } else {
            FIELD
        },
    );
    draw_hosted_direct_progress(&mut buffer, size.width, size.height, rects.address, summary);
    fill_rect(
        &mut buffer,
        size.width,
        size.height,
        Rect {
            x: rects.address.x,
            y: rects.address.y,
            w: rects.address.w,
            h: 2,
        },
        if shell.address_focused {
            BUTTON_BRIGHT
        } else {
            BORDER
        },
    );
    let address_chars = rects.address.w.saturating_sub(16) / (GLYPH_W + GLYPH_GAP);
    let address_text = shell.address_display_text(address_chars as usize);
    draw_text(
        &mut buffer,
        size.width,
        size.height,
        rects.address.x + 8,
        rects.address.y + 6,
        &truncate(&address_text, address_chars as usize),
        if shell.address_focused {
            TEXT
        } else {
            TEXT_DIM
        },
        1,
    );
    let right_x = size.width.saturating_sub(250);
    if !summary.certificate_warning_active() {
        draw_text(
            &mut buffer,
            size.width,
            size.height,
            right_x,
            48,
            &status,
            STATUS_OK,
            1,
        );
        if HOSTED_DIRECT_DEBUG_TELEMETRY && !log_status.is_empty() {
            draw_text(
                &mut buffer,
                size.width,
                size.height,
                right_x,
                30,
                &truncate(&log_status, 32),
                TEXT_DIM,
                1,
            );
        }
    }
    if let Some(certificate_status) = summary.compact_certificate_status() {
        let certificate_trust_ready = summary.certificate_fingerprint_sha256.is_some();
        draw_text(
            &mut buffer,
            size.width,
            size.height,
            right_x,
            30,
            &truncate(&certificate_status, 32),
            STATUS_WARN,
            1,
        );
        draw_hosted_direct_button(
            &mut buffer,
            size.width,
            size.height,
            rects.certificate_back,
            "BACK",
            true,
        );
        draw_hosted_direct_button(
            &mut buffer,
            size.width,
            size.height,
            rects.trust_once,
            "ONCE",
            certificate_trust_ready,
        );
        draw_hosted_direct_button(
            &mut buffer,
            size.width,
            size.height,
            rects.trust_appliance,
            "TRUST",
            certificate_trust_ready && browser_mode != BrowserMode::Incognito,
        );
    }
    let tab_label = match (summary.active_tab_index, summary.tab_count) {
        (Some(index), Some(total)) => format!("TAB {index}/{total}"),
        _ => "TAB 1/1".to_string(),
    };
    let title = summary
        .active_title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("loading");
    let title_x = rects.address.x;
    let title_chars =
        size.width.saturating_sub(title_x).saturating_sub(272) / (GLYPH_W + GLYPH_GAP);
    if title_chars >= 18 {
        draw_text(
            &mut buffer,
            size.width,
            size.height,
            title_x,
            16,
            &truncate(&format!("{tab_label}  {title}"), title_chars as usize),
            TEXT_DIM,
            1,
        );
    }
    buffer.present().map_err(|error| error.to_string())
}

pub fn draw_hosted_direct_button(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    enabled: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if enabled { PANEL_ALT } else { BUTTON_DISABLED },
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: 1,
        },
        if enabled { BORDER } else { FIELD },
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 11u32.saturating_sub((label.chars().count() as u32).saturating_mul(2)),
        rect.y + 6,
        label,
        if enabled { TEXT } else { TEXT_DIM },
        1,
    );
}

pub fn draw_hosted_direct_progress(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    address: Rect,
    summary: &HostedDirectLogSummary,
) {
    let progress = summary.load_progress().clamp(0.0, 1.0);
    let track = Rect {
        x: address.x,
        y: address.y + address.h.saturating_sub(2),
        w: address.w,
        h: 2,
    };
    fill_rect(buffer, width, height, track, BORDER);
    let fill_width = ((address.w as f32) * progress).round() as u32;
    if fill_width > 0 {
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: track.x,
                y: track.y,
                w: fill_width.min(track.w),
                h: track.h,
            },
            if summary.is_loading() {
                BUTTON_BRIGHT
            } else {
                STATUS_OK
            },
        );
    }
}

pub fn parse_hosted_direct_log_summary(path: &Path) -> HostedDirectLogSummary {
    let Ok(contents) = read_hosted_direct_log_window(path) else {
        return HostedDirectLogSummary::default();
    };
    parse_hosted_direct_log_summary_text(&contents)
}

pub fn read_hosted_direct_log_window(path: &Path) -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let full_window = HOSTED_DIRECT_LOG_HEAD_BYTES + HOSTED_DIRECT_LOG_TAIL_BYTES;
    if len <= full_window {
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        return Ok(contents);
    }

    let mut head = vec![0; HOSTED_DIRECT_LOG_HEAD_BYTES as usize];
    file.read_exact(&mut head)?;

    file.seek(SeekFrom::Start(len - HOSTED_DIRECT_LOG_TAIL_BYTES))?;
    let mut tail = Vec::with_capacity(HOSTED_DIRECT_LOG_TAIL_BYTES as usize);
    file.read_to_end(&mut tail)?;

    let mut contents = String::from_utf8_lossy(&head).into_owned();
    contents.push('\n');
    contents.push_str(&String::from_utf8_lossy(&tail));
    Ok(contents)
}

pub fn parse_hosted_direct_log_summary_text(contents: &str) -> HostedDirectLogSummary {
    let mut summary = HostedDirectLogSummary::default();
    for line in contents.lines() {
        if !line.contains("[window-direct]") {
            continue;
        }
        if let Some(value) = extract_token_after(line, "first direct present in ") {
            summary.first_present = Some(value.to_string());
        }
        if let Some(rest) = line.split_once("active load status ").map(|(_, rest)| rest) {
            if let Some(status) = rest.split_whitespace().next() {
                summary.latest_load_complete = status == "Complete";
                if let Some(duration) = extract_between(rest, " after ", " url ") {
                    summary.latest_load = Some(format!("{status} {duration}"));
                }
                if let Some(url) = extract_after(rest, " url ") {
                    summary.latest_url = Some(url.to_string());
                }
            }
        }
        if let Some(rest) = line
            .split_once("pending load status ")
            .map(|(_, rest)| rest)
        {
            if let Some(status) = rest.split_whitespace().next() {
                summary.latest_load_complete = false;
                if let Some(duration) = extract_between(rest, " after ", " url ") {
                    summary.latest_load = Some(format!("pending {status} {duration}"));
                }
                if let Some(url) = extract_after(rest, " url ") {
                    summary.latest_url = Some(url.to_string());
                }
            }
        }
        if let Some(rest) = line.split_once("chrome status ").map(|(_, rest)| rest) {
            if let Some(tab) = extract_token_after(rest, "tab ") {
                if let Some((index, total)) = tab.split_once('/') {
                    summary.active_tab_index = index.parse::<usize>().ok();
                    summary.tab_count = total.parse::<usize>().ok();
                }
            }
            if let Some(back) = extract_token_after(rest, "back ") {
                summary.can_go_back = back == "true";
            }
            if let Some(forward) = extract_token_after(rest, "forward ") {
                summary.can_go_forward = forward == "true";
            }
            if let Some(title) = extract_between(rest, " title ", " url ") {
                summary.active_title = Some(title.to_string());
            }
            if let Some(url) = extract_after(rest, " url ") {
                summary.latest_url = Some(url.to_string());
            }
        }
        if let Some(rest) = line.split_once("chrome tab ").map(|(_, rest)| rest) {
            let mut parts = rest.split_whitespace();
            if let (Some(index), Some(active), Some(url)) =
                (parts.next(), parts.next(), parts.next())
            {
                if let Ok(index) = index.parse::<usize>() {
                    if index == 0 {
                        summary.tabs.clear();
                    }
                    if index == summary.tabs.len() {
                        summary.tabs.push(HostedDirectTab {
                            url: url.to_string(),
                            active: active == "1",
                        });
                    }
                }
            }
        }
        if let Some(fingerprint) = extract_token_after(line, "certificate fingerprint sha256 ") {
            summary.certificate_fingerprint_sha256 = Some(fingerprint.to_string());
        }
        if line.contains("host command certificate-go-back") {
            summary.certificate_back_requested = true;
        }
        if line.contains("host command trust-once") {
            summary.certificate_trust_once_requested = true;
        }
        if line.contains("host command trust-this-appliance") {
            summary.certificate_trust_this_appliance_requested = true;
        }
        if let Some(duration) = extract_between(line, "retained navigation swap to ", " (") {
            if let Some((_, after)) = duration.rsplit_once(" after ") {
                summary.latest_swap = Some(after.to_string());
            }
            if let Some((target, _)) = duration.rsplit_once(" after ") {
                summary.latest_url = Some(target.to_string());
            }
        }
        if let Some(value) = extract_token_after(line, "verified repeat input direct frame in ") {
            summary.latest_input = Some(format!("repeat {value}"));
        } else if let Some(value) = extract_token_after(line, "verified input direct frame in ") {
            summary.latest_input = Some(format!("verify {value}"));
        } else if let Some(value) = extract_token_after(line, "input direct frame in ") {
            summary.latest_input = Some(value.to_string());
        } else if let Some(value) = extract_token_after(line, "search submit direct frame in ") {
            summary.latest_input = Some(format!("search {value}"));
        } else if let Some(value) = extract_token_after(line, "live search direct frame in ") {
            summary.latest_input = Some(format!("live {value}"));
        }
        if line.contains("slow direct frame ") {
            summary.slow_frames = summary.slow_frames.saturating_add(1);
            if let Some(label) = extract_token_after(line, " total=") {
                update_max_duration(
                    &mut summary.max_frame_ms,
                    &mut summary.max_frame_label,
                    label,
                );
            }
            if let Some(label) = extract_token_after(line, " paint=") {
                update_max_duration(
                    &mut summary.max_paint_ms,
                    &mut summary.max_paint_label,
                    label,
                );
            }
        }
        if let Some((_, raw_audit)) = line.split_once("resource audit ") {
            summary.latest_resource_audit = summarize_resource_audit(raw_audit);
        }
    }
    summary
}

pub fn summarize_resource_audit(raw_audit: &str) -> Option<String> {
    let image_count = json_u64_field(raw_audit, "imageCount");
    let broken_images = json_array_len_field(raw_audit, "brokenImages");
    let inline_svgs = json_u64_field(raw_audit, "inlineSvgCount");
    let zero_svgs = json_u64_field(raw_audit, "zeroSizeSvgCount");
    let stylesheets = json_u64_field(raw_audit, "stylesheetCount");
    let canvases = json_u64_field(raw_audit, "canvasCount");

    if image_count
        .or(broken_images)
        .or(inline_svgs)
        .or(zero_svgs)
        .or(stylesheets)
        .or(canvases)
        .is_none()
    {
        return None;
    }

    Some(format!(
        "audit img {} broken {} svg {}/{} css {} canvas {}",
        format_optional_count(image_count),
        format_optional_count(broken_images),
        format_optional_count(inline_svgs),
        format_optional_count(zero_svgs),
        format_optional_count(stylesheets),
        format_optional_count(canvases),
    ))
}

pub fn format_optional_count(value: Option<u64>) -> String {
    value
        .map(|count| count.to_string())
        .unwrap_or_else(|| "?".to_string())
}

pub fn json_u64_field(raw: &str, field: &str) -> Option<u64> {
    let marker = format!("\"{field}\"");
    let (_, rest) = raw.split_once(&marker)?;
    let (_, rest) = rest.split_once(':')?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub fn json_array_len_field(raw: &str, field: &str) -> Option<u64> {
    let marker = format!("\"{field}\"");
    let (_, rest) = raw.split_once(&marker)?;
    let (_, rest) = rest.split_once('[')?;
    let mut depth = 1u32;
    let mut in_string = false;
    let mut escape = false;
    let mut saw_value = false;
    let mut values = 0u64;
    for ch in rest.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"' => {
                in_string = !in_string;
                saw_value = true;
            }
            '[' if !in_string => {
                depth += 1;
                saw_value = true;
            }
            ']' if !in_string => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(if saw_value { values + 1 } else { 0 });
                }
            }
            ',' if !in_string && depth == 1 => {
                values += 1;
                saw_value = false;
            }
            ch if !in_string && !ch.is_whitespace() => saw_value = true,
            _ => {}
        }
    }
    None
}

pub fn extract_between<'a>(value: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let (_, rest) = value.split_once(start)?;
    let (between, _) = rest.split_once(end)?;
    Some(between.trim())
}

pub fn extract_after<'a>(value: &'a str, marker: &str) -> Option<&'a str> {
    let (_, rest) = value.split_once(marker)?;
    Some(rest.trim())
}

pub fn extract_token_after<'a>(value: &'a str, marker: &str) -> Option<&'a str> {
    let (_, rest) = value.split_once(marker)?;
    rest.split_whitespace().next()
}

pub fn compact_fingerprint(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() <= 16 {
        return value.to_string();
    }
    let prefix = value.chars().take(8).collect::<String>();
    let suffix = value
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{prefix}..{suffix}")
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub fn appliance_cert_entry_label(entry: &ApplianceCertTrustEntry) -> String {
    if let Some(label) = entry
        .label
        .as_deref()
        .filter(|label| !label.trim().is_empty())
    {
        return truncate(label.trim(), 42);
    }
    Url::parse(&entry.origin)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_string()))
        .unwrap_or_else(|| entry.origin.clone())
}

pub fn update_max_duration(max_ms: &mut Option<f64>, max_label: &mut Option<String>, label: &str) {
    let Some(duration_ms) = parse_duration_label_ms(label) else {
        return;
    };
    if max_ms.is_none_or(|current| duration_ms > current) {
        *max_ms = Some(duration_ms);
        *max_label = Some(label.to_string());
    }
}

pub fn parse_duration_label_ms(label: &str) -> Option<f64> {
    if let Some(value) = label.strip_suffix("ms") {
        return value.parse::<f64>().ok();
    }
    if let Some(value) = label.strip_suffix('s') {
        return value.parse::<f64>().ok().map(|seconds| seconds * 1000.0);
    }
    None
}

#[cfg(test)]
mod address_cursor_tests {
    use super::{clamp_char_boundary, next_char_boundary, HostedDirectShellState};

    #[test]
    fn push_address_text_keeps_cursor_on_char_boundary() {
        let mut shell = HostedDirectShellState::new("https://example.com".to_string());
        shell.address_focused = true;
        shell.push_address_text("café");
        shell.push_address_text("🙂");
        // Must not panic when rendering the caret.
        let _ = shell.address_display_text(80);
        assert!(shell.address_input.is_char_boundary(shell.address_cursor));
    }

    #[test]
    fn clamp_char_boundary_never_panics_on_mid_scalar() {
        let text = "a🙂b";
        let mid = 2; // inside the 4-byte emoji
        assert_eq!(clamp_char_boundary(text, mid), 1);
        assert!(text.is_char_boundary(clamp_char_boundary(text, mid)));
        assert_eq!(clamp_char_boundary(text, text.len()), text.len());
    }

    #[test]
    fn next_char_boundary_skips_whole_scalars() {
        let text = "a🙂b";
        assert_eq!(next_char_boundary(text, 1), 5);
        assert_eq!(next_char_boundary(text, 2), 5);
    }
}
