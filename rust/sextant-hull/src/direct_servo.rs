use std::cell::{Cell, RefCell};
use std::env;
use std::io::{BufRead, Read, Write};
use std::net::{Shutdown, TcpListener};
use std::num::NonZeroIsize;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;
use servo::{
    CompositionEvent, CompositionState, DevicePoint, ImeEvent, InputEvent, JSValue, Key, KeyState,
    KeyboardEvent, LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent, MouseMoveEvent,
    NamedKey as ServoNamedKey, Opts, Preferences, RenderingContext, Servo, ServoBuilder,
    UserContentManager, UserScript, WebView, WebViewBuilder, WebViewDelegate, WheelDelta,
    WheelEvent, WheelMode, WindowRenderingContext,
};
use sha2::{Digest, Sha256};
use url::Url;
use winit30::application::ApplicationHandler;
use winit30::dpi::{PhysicalPosition, PhysicalSize, Position};
use winit30::event::{
    ElementState, KeyEvent, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use winit30::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit30::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};
use winit30::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawWindowHandle, Win32WindowHandle,
};
use winit30::window::{Window, WindowAttributes, WindowId};

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{SetActiveWindow, SetFocus},
    WindowsAndMessaging::{BringWindowToTop, SetForegroundWindow},
};

const LINE_SCROLL_PIXELS: f32 = 76.0;
const VERIFIED_INPUT_TEXT: &str = "sextant";
const VERIFIED_INPUT_TITLE: &str = "typed:sextant";
const VERIFIED_INPUT_REPEAT_TEXT: &str = "2";
const VERIFIED_INPUT_REPEAT_TITLE: &str = "typed:sextant2";
const VERIFIED_INPUT_CLICK_X: f32 = 180.0;
const VERIFIED_INPUT_CLICK_Y: f32 = 128.0;
const SEARCH_SUBMIT_TEXT: &str = "sextant";
const SEARCH_SUBMIT_TITLE: &str = "search:sextant";
const LIVE_FORM_TEXT: &str = "Sextant";
const DIRECT_TAB_SMOKE_TITLE: &str = "Sextant-Direct-Tab-Smoke";
const RETAINED_NAVIGATION_SMOKE_TITLE: &str = "SextantRetainedNavigationSmoke";
const RETAINED_NAVIGATION_TARGET_TITLE: &str = "SextantRetainedNavigationTarget";
const RETAINED_NAVIGATION_CLICK_X: f32 = 150.0;
const RETAINED_NAVIGATION_CLICK_Y: f32 = 124.0;
const LIVE_SEARCH_TEXT: &str = "Sextant";
const LIVE_SEARCH_FALLBACK_CLICK_X: f32 = 590.0;
const LIVE_SEARCH_FALLBACK_CLICK_Y: f32 = 330.0;
const LIVE_SEARCH_GOOGLE_CLICK_Y: f32 = 285.0;
const LIVE_SEARCH_READY_DELAY: Duration = Duration::from_millis(3500);
const LIVE_SEARCH_CLICK_SETTLE_DELAY: Duration = Duration::from_millis(350);
const LIVE_SEARCH_FALLBACK_DELAY: Duration = Duration::from_secs(6);
const LIVE_FORM_READY_DELAY: Duration = Duration::from_millis(1500);
const LIVE_FORM_CLICK_SETTLE_DELAY: Duration = Duration::from_millis(350);
const LIVE_LINK_READY_DELAY: Duration = Duration::from_millis(800);
const APPLIANCE_CERT_TRUST_ONCE_TITLE: &str = "SextantTrustOnce";
const APPLIANCE_CERT_REMEMBER_TITLE: &str = "SextantTrustThisAppliance";
const RETAINED_NAVIGATION_TITLE_PREFIX: &str = "SextantRetainedNavigate:";
const EMBEDDED_STARTUP_PUMP: Duration = Duration::from_secs(4);
const INPUT_PUMP_AFTER_EVENT: Duration = Duration::from_millis(96);
const DIRECT_READY_FRAME_MIN_INTERVAL: Duration = Duration::from_millis(16);
const DIRECT_LOADING_READY_FRAME_MIN_INTERVAL: Duration = Duration::from_millis(50);
const DIRECT_SHELL_BACKGROUND_RGBA: [f64; 4] = [16.0 / 255.0, 22.0 / 255.0, 29.0 / 255.0, 1.0];
const DIRECT_COMPAT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:126.0) Gecko/20100101 Firefox/126.0";

#[derive(Clone)]
pub struct DirectApplianceCertificateActions {
    pub persist_allowed: bool,
    pub remember: Arc<dyn Fn(String) -> Result<(), String> + Send + Sync>,
}

pub struct DirectFixtureServer {
    url: Url,
    shutdown_tx: mpsc::Sender<()>,
    handle: Option<thread::JoinHandle<()>>,
}

impl DirectFixtureServer {
    pub fn url(&self) -> &Url {
        &self.url
    }
}

impl Drop for DirectFixtureServer {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(port) = self.url.port() {
            let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub struct DirectServoOptions {
    pub target: Url,
    pub smoke: bool,
    pub timeout: Duration,
    pub title: String,
    pub log_prefix: &'static str,
    pub setup_servo_logging: bool,
    pub scripted_text: Option<String>,
    pub scripted_first_interaction_smoke: bool,
    pub scripted_verified_input_smoke: bool,
    pub scripted_verified_input_repeat_smoke: bool,
    pub scripted_search_submit_smoke: bool,
    pub scripted_live_search_smoke: bool,
    pub scripted_live_form_smoke: bool,
    pub scripted_live_link_smoke: bool,
    pub scripted_retained_navigation_smoke: bool,
    pub scripted_location: Option<String>,
    pub scripted_history: Option<String>,
    pub scripted_load_smoke: bool,
    pub scripted_reload_smoke: bool,
    pub scripted_resize_smoke: bool,
    pub scripted_tab_smoke: bool,
    pub resource_audit: bool,
    pub config_dir: Option<PathBuf>,
    pub disable_http_cache: bool,
    pub certificate_path: Option<PathBuf>,
    pub ignore_certificate_errors: bool,
    pub appliance_certificate_actions: Option<DirectApplianceCertificateActions>,
    pub bypass_proxy_for_target: bool,
    pub embed_parent_hwnd: Option<isize>,
    pub embed_bounds: Option<DirectEmbedBounds>,
    pub host_command_rx: Option<mpsc::Receiver<String>>,
    pub read_host_commands_from_stdin: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DirectEmbedBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct DirectServoOutcome {
    pub first_present: Option<Duration>,
    pub direct_frame_count: u64,
    pub direct_slow_frame_count: u64,
    pub direct_max_frame: Option<Duration>,
    pub direct_max_spin: Option<Duration>,
    pub direct_max_paint: Option<Duration>,
    pub direct_max_present: Option<Duration>,
    pub first_interaction_frame: Option<Duration>,
    pub first_interaction_before_load_complete: Option<bool>,
    pub verified_input_frame: Option<Duration>,
    pub verified_input_second_frame: Option<Duration>,
    pub verified_input: Option<bool>,
    pub verified_input_before_load_complete: Option<bool>,
    pub search_submit_frame: Option<Duration>,
    pub search_submit: Option<bool>,
    pub search_submit_url: Option<String>,
    pub search_submit_title: Option<String>,
    pub live_search_frame: Option<Duration>,
    pub live_search: Option<bool>,
    pub live_search_url: Option<String>,
    pub live_search_title: Option<String>,
    pub live_form_frame: Option<Duration>,
    pub live_form: Option<bool>,
    pub live_form_url: Option<String>,
    pub live_form_title: Option<String>,
    pub live_link_frame: Option<Duration>,
    pub live_link: Option<bool>,
    pub live_link_url: Option<String>,
    pub live_link_title: Option<String>,
    pub retained_navigation_frame: Option<Duration>,
    #[allow(dead_code)]
    pub retained_navigation_url: Option<String>,
    #[allow(dead_code)]
    pub retained_navigation_title: Option<String>,
    pub input_frame: Option<Duration>,
    pub location_frame: Option<Duration>,
    pub location_url: Option<String>,
    pub location_title: Option<String>,
    pub history_navigation_frame: Option<Duration>,
    pub history_back_frame: Option<Duration>,
    pub history_forward_frame: Option<Duration>,
    pub history_final_url: Option<String>,
    pub history_final_title: Option<String>,
    pub load_complete: Option<Duration>,
    pub load_url: Option<String>,
    pub load_title: Option<String>,
    pub reload_frame: Option<Duration>,
    pub reload_url: Option<String>,
    pub reload_title: Option<String>,
    pub certificate_fingerprint_sha256: Option<String>,
    pub appliance_certificate_trust_once_requested: bool,
    pub appliance_certificate_remembered: Option<bool>,
    pub appliance_certificate_remember_error: Option<String>,
    pub resource_audit_json: Option<String>,
    pub resize_frame: Option<Duration>,
    pub tab_frame: Option<Duration>,
    pub tab_url: Option<String>,
    pub tab_title: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistorySmokePhase {
    WaitingInitial,
    NavigatingSecond,
    Back,
    Forward,
    Done,
}

#[derive(Clone, Copy, Debug)]
enum DirectServoUserEvent {
    HostCommand,
}

struct DirectServoApp {
    target: Url,
    smoke: bool,
    timeout: Duration,
    title: String,
    log_prefix: &'static str,
    setup_servo_logging: bool,
    scripted_text: Option<String>,
    scripted_first_interaction_smoke: bool,
    scripted_verified_input_smoke: bool,
    scripted_verified_input_repeat_smoke: bool,
    scripted_search_submit_smoke: bool,
    scripted_live_search_smoke: bool,
    scripted_live_form_smoke: bool,
    scripted_location: Option<String>,
    scripted_history: Option<String>,
    scripted_load_smoke: bool,
    scripted_reload_smoke: bool,
    scripted_resize_smoke: bool,
    scripted_tab_smoke: bool,
    resource_audit: bool,
    config_dir: Option<PathBuf>,
    disable_http_cache: bool,
    certificate_path: Option<PathBuf>,
    ignore_certificate_errors: bool,
    appliance_certificate_actions: Option<DirectApplianceCertificateActions>,
    bypass_proxy_for_target: bool,
    embed_parent_hwnd: Option<isize>,
    embed_bounds: Option<DirectEmbedBounds>,
    host_command_rx: Option<mpsc::Receiver<String>>,
    started: Instant,
    input_started: Option<Instant>,
    first_interaction_started: Option<Instant>,
    first_interaction_before_load_complete: Option<bool>,
    scripted_first_interaction_sent: bool,
    verified_input_started: Option<Instant>,
    verified_input_second_started: Option<Instant>,
    verified_input_before_load_complete: Option<bool>,
    scripted_verified_input_sent: bool,
    scripted_verified_input_second_sent: bool,
    scripted_verified_input_done: bool,
    search_submit_started: Option<Instant>,
    scripted_search_submit_sent: bool,
    scripted_search_submit_done: bool,
    live_search_started: Option<Instant>,
    live_search_last_attempt: Option<Instant>,
    live_search_pending_text_since: Option<Instant>,
    live_search_attempts: u8,
    live_search_dom_navigation_used: bool,
    scripted_live_search_sent: bool,
    scripted_live_search_done: bool,
    live_form_started: Option<Instant>,
    live_form_submit_pending_since: Option<Instant>,
    scripted_live_form_sent: bool,
    scripted_live_form_done: bool,
    scripted_live_link_smoke: bool,
    live_link_started: Option<Instant>,
    scripted_live_link_sent: bool,
    scripted_live_link_done: bool,
    retained_navigation_started: Option<Instant>,
    scripted_retained_navigation_smoke: bool,
    scripted_retained_navigation_sent: bool,
    scripted_retained_navigation_done: bool,
    scripted_text_sent: bool,
    location_started: Option<Instant>,
    location_expected: Option<Url>,
    scripted_location_started: bool,
    scripted_location_done: bool,
    history_phase: Option<HistorySmokePhase>,
    history_initial: Option<Url>,
    history_expected: Option<Url>,
    history_started: Option<Instant>,
    history_back_started: Option<Instant>,
    history_forward_started: Option<Instant>,
    scripted_load_done: bool,
    reload_started: Option<Instant>,
    scripted_reload_started: bool,
    scripted_reload_done: bool,
    resize_started: Option<Instant>,
    resize_target: PhysicalSize<u32>,
    scripted_resize_started: bool,
    scripted_resize_done: bool,
    tab_started: Option<Instant>,
    scripted_tab_smoke_started: bool,
    scripted_tab_smoke_done: bool,
    state: Option<Rc<DirectServoState>>,
}

struct DirectPendingNavigation {
    webview: WebView,
    target: Url,
    started: Instant,
}

struct DirectServoState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    self_handle: RefCell<Weak<DirectServoState>>,
    log_prefix: &'static str,
    webviews: RefCell<Vec<WebView>>,
    active_index: Cell<usize>,
    pending_navigation: RefCell<Option<DirectPendingNavigation>>,
    active_navigation_started: Cell<Option<Instant>>,
    frame_ready: Cell<bool>,
    first_present: Cell<Option<Duration>>,
    direct_frame_count: Cell<u64>,
    direct_slow_frame_count: Cell<u64>,
    // `direct_max_frame` is the worst observed total frame time; the spin/paint/
    // present cells hold that same frame's co-recorded breakdown (see
    // `keep_worst_total_frame`), not independent per-component maxima.
    direct_max_frame: Cell<Option<Duration>>,
    direct_max_spin: Cell<Option<Duration>>,
    direct_max_paint: Cell<Option<Duration>>,
    direct_max_present: Cell<Option<Duration>>,
    last_direct_present_at: Cell<Option<Instant>>,
    first_interaction_frame: Cell<Option<Duration>>,
    first_interaction_before_load_complete: Cell<Option<bool>>,
    verified_input_frame: Cell<Option<Duration>>,
    verified_input_second_frame: Cell<Option<Duration>>,
    verified_input: Cell<Option<bool>>,
    verified_input_before_load_complete: Cell<Option<bool>>,
    search_submit_frame: Cell<Option<Duration>>,
    search_submit: Cell<Option<bool>>,
    live_search_frame: Cell<Option<Duration>>,
    live_search: Cell<Option<bool>>,
    live_search_dom_fallback_url: RefCell<Option<Url>>,
    live_form_frame: Cell<Option<Duration>>,
    live_form: Cell<Option<bool>>,
    live_form_submit_target: RefCell<Option<(f32, f32)>>,
    live_link_frame: Cell<Option<Duration>>,
    live_link: Cell<Option<bool>>,
    live_link_target: RefCell<Option<(f32, f32)>>,
    live_link_expected_host: RefCell<Option<String>>,
    retained_navigation_frame: Cell<Option<Duration>>,
    input_frame: Cell<Option<Duration>>,
    location_frame: Cell<Option<Duration>>,
    history_navigation_frame: Cell<Option<Duration>>,
    history_back_frame: Cell<Option<Duration>>,
    history_forward_frame: Cell<Option<Duration>>,
    load_complete: Cell<Option<Duration>>,
    reload_frame: Cell<Option<Duration>>,
    certificate_probe_started: Cell<bool>,
    certificate_page_decorated: Cell<bool>,
    certificate_der_base64: Rc<RefCell<Option<String>>>,
    appliance_certificate_actions: Option<DirectApplianceCertificateActions>,
    appliance_certificate_trust_once_pending: Cell<bool>,
    appliance_certificate_remember_pending: Cell<bool>,
    appliance_certificate_remember_result: Rc<RefCell<Option<Result<(), String>>>>,
    resource_audit: bool,
    resource_audit_started: Rc<Cell<bool>>,
    resource_audit_last_started: Cell<Option<Instant>>,
    resource_audit_json: Rc<RefCell<Option<String>>>,
    text_paint_warmup_started: Cell<bool>,
    resize_frame: Cell<Option<Duration>>,
    tab_frame: Cell<Option<Duration>>,
    tab_smoke_url: RefCell<Option<String>>,
    tab_smoke_title: RefCell<Option<String>>,
    last_chrome_status: RefCell<String>,
    last_window_size: Cell<PhysicalSize<u32>>,
    cursor_position: Cell<(f32, f32)>,
    last_mouse_move_sent: Cell<Option<Instant>>,
    mouse_button_down: Cell<bool>,
    input_events: Cell<u64>,
    last_input_at: Cell<Option<Instant>>,
    modifiers: Cell<ModifiersState>,
    location_active: Cell<bool>,
    location_replace_on_text: Cell<bool>,
    location_buffer: RefCell<String>,
}

impl WebViewDelegate for DirectServoState {
    fn notify_url_changed(&self, webview: WebView, _url: Url) {
        if self.is_pending_webview(&webview) {
            self.update_window_title();
            return;
        }
        if self.is_active_webview(&webview) {
            self.certificate_probe_started.set(false);
            self.certificate_page_decorated.set(false);
            self.certificate_der_base64.borrow_mut().take();
            self.resource_audit_started.set(false);
            self.resource_audit_last_started.set(None);
            self.resource_audit_json.borrow_mut().take();
            self.update_window_title();
        }
    }

    fn notify_page_title_changed(&self, webview: WebView, _title: Option<String>) {
        if self.is_active_webview(&webview) {
            if let Some(url) = retained_navigation_title_target(_title.as_deref()) {
                self.begin_retained_navigation(url);
                return;
            }
        }
        if self.is_pending_webview(&webview) {
            self.update_window_title();
            return;
        }
        if self.is_active_webview(&webview) {
            if self.active_page_title().as_deref() == Some(APPLIANCE_CERT_TRUST_ONCE_TITLE) {
                self.appliance_certificate_trust_once_pending.set(true);
            }
            if self.active_page_title().as_deref() == Some(APPLIANCE_CERT_REMEMBER_TITLE) {
                self.appliance_certificate_remember_pending.set(true);
            }
            if self.active_page_title().as_deref() == Some("Certificate error") {
                self.capture_bad_certificate_bytes(webview.clone());
                self.decorate_bad_certificate_page(webview);
            }
            self.update_window_title();
            self.window.request_redraw();
        }
    }

    fn notify_history_changed(&self, webview: WebView, _entries: Vec<Url>, _current: usize) {
        if self.is_pending_webview(&webview) {
            self.update_window_title();
            return;
        }
        if self.is_active_webview(&webview) {
            self.update_window_title();
        }
    }

    fn notify_load_status_changed(&self, webview: WebView, status: LoadStatus) {
        if self.is_pending_webview(&webview) {
            self.log_pending_load_status(&webview, status);
            if status == LoadStatus::HeadParsed {
                self.activate_pending_navigation(&webview, "head parsed");
            } else if status == LoadStatus::Complete {
                self.activate_pending_navigation(&webview, "load complete");
            } else {
                self.update_window_title();
            }
            return;
        }
        if self.is_active_webview(&webview) {
            self.log_active_load_status(&webview, status);
            self.update_window_title();
            self.window.request_redraw();
            if status == LoadStatus::Complete
                && self.active_page_title().as_deref() == Some("Certificate error")
            {
                self.capture_bad_certificate_bytes(webview.clone());
                self.decorate_bad_certificate_page(webview);
            } else if status == LoadStatus::Complete && self.resource_audit_due() {
                self.capture_resource_audit(webview);
            } else if status == LoadStatus::Complete {
                self.warm_text_paint(webview);
            }
        }
    }

    fn notify_new_frame_ready(&self, webview: WebView) {
        if self.is_pending_webview(&webview) {
            if webview.load_status() != LoadStatus::Started {
                self.activate_pending_navigation(&webview, "first content frame");
            } else {
                self.update_window_title();
            }
            return;
        }
        if self.is_active_webview(&webview) {
            self.frame_ready.set(true);
            self.window.request_redraw();
        }
    }
}

impl DirectServoApp {
    fn new(options: DirectServoOptions) -> Self {
        Self {
            target: options.target,
            smoke: options.smoke,
            timeout: options.timeout,
            title: options.title,
            log_prefix: options.log_prefix,
            setup_servo_logging: options.setup_servo_logging,
            scripted_text: options.scripted_text,
            scripted_first_interaction_smoke: options.scripted_first_interaction_smoke,
            scripted_verified_input_smoke: options.scripted_verified_input_smoke,
            scripted_verified_input_repeat_smoke: options.scripted_verified_input_repeat_smoke,
            scripted_search_submit_smoke: options.scripted_search_submit_smoke,
            scripted_live_search_smoke: options.scripted_live_search_smoke,
            scripted_live_form_smoke: options.scripted_live_form_smoke,
            scripted_retained_navigation_smoke: options.scripted_retained_navigation_smoke,
            scripted_location: options.scripted_location,
            scripted_history: options.scripted_history,
            scripted_load_smoke: options.scripted_load_smoke,
            scripted_reload_smoke: options.scripted_reload_smoke,
            scripted_resize_smoke: options.scripted_resize_smoke,
            scripted_tab_smoke: options.scripted_tab_smoke,
            resource_audit: options.resource_audit,
            config_dir: options.config_dir,
            disable_http_cache: options.disable_http_cache,
            certificate_path: options.certificate_path,
            ignore_certificate_errors: options.ignore_certificate_errors,
            appliance_certificate_actions: options.appliance_certificate_actions,
            bypass_proxy_for_target: options.bypass_proxy_for_target,
            embed_parent_hwnd: options.embed_parent_hwnd,
            embed_bounds: options.embed_bounds,
            host_command_rx: options.host_command_rx,
            started: Instant::now(),
            input_started: None,
            first_interaction_started: None,
            first_interaction_before_load_complete: None,
            scripted_first_interaction_sent: false,
            verified_input_started: None,
            verified_input_second_started: None,
            verified_input_before_load_complete: None,
            scripted_verified_input_sent: false,
            scripted_verified_input_second_sent: false,
            scripted_verified_input_done: false,
            search_submit_started: None,
            scripted_search_submit_sent: false,
            scripted_search_submit_done: false,
            live_search_started: None,
            live_search_last_attempt: None,
            live_search_pending_text_since: None,
            live_search_attempts: 0,
            live_search_dom_navigation_used: false,
            scripted_live_search_sent: false,
            scripted_live_search_done: false,
            live_form_started: None,
            live_form_submit_pending_since: None,
            scripted_live_form_sent: false,
            scripted_live_form_done: false,
            scripted_live_link_smoke: options.scripted_live_link_smoke,
            live_link_started: None,
            scripted_live_link_sent: false,
            scripted_live_link_done: false,
            retained_navigation_started: None,
            scripted_retained_navigation_sent: false,
            scripted_retained_navigation_done: false,
            scripted_text_sent: false,
            location_started: None,
            location_expected: None,
            scripted_location_started: false,
            scripted_location_done: false,
            history_phase: None,
            history_initial: None,
            history_expected: None,
            history_started: None,
            history_back_started: None,
            history_forward_started: None,
            scripted_load_done: false,
            reload_started: None,
            scripted_reload_started: false,
            scripted_reload_done: false,
            resize_started: None,
            resize_target: PhysicalSize::new(640, 420),
            scripted_resize_started: false,
            scripted_resize_done: false,
            tab_started: None,
            scripted_tab_smoke_started: false,
            scripted_tab_smoke_done: false,
            state: None,
        }
    }

    fn first_present(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.first_present.get())
    }

    fn direct_frame_count(&self) -> u64 {
        self.state
            .as_ref()
            .map(|state| state.direct_frame_count.get())
            .unwrap_or(0)
    }

    fn direct_slow_frame_count(&self) -> u64 {
        self.state
            .as_ref()
            .map(|state| state.direct_slow_frame_count.get())
            .unwrap_or(0)
    }

    fn direct_max_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.direct_max_frame.get())
    }

    fn direct_max_spin(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.direct_max_spin.get())
    }

    fn direct_max_paint(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.direct_max_paint.get())
    }

    fn direct_max_present(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.direct_max_present.get())
    }

    fn input_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.input_frame.get())
    }

    fn first_interaction_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.first_interaction_frame.get())
    }

    fn first_interaction_before_load_complete(&self) -> Option<bool> {
        self.state
            .as_ref()
            .and_then(|state| state.first_interaction_before_load_complete.get())
    }

    fn verified_input_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.verified_input_frame.get())
    }

    fn verified_input_second_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.verified_input_second_frame.get())
    }

    fn verified_input(&self) -> Option<bool> {
        self.state
            .as_ref()
            .and_then(|state| state.verified_input.get())
    }

    fn verified_input_before_load_complete(&self) -> Option<bool> {
        self.state
            .as_ref()
            .and_then(|state| state.verified_input_before_load_complete.get())
    }

    fn search_submit_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.search_submit_frame.get())
    }

    fn search_submit(&self) -> Option<bool> {
        self.state
            .as_ref()
            .and_then(|state| state.search_submit.get())
    }

    fn search_submit_url(&self) -> Option<String> {
        if self.search_submit() == Some(true) {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn search_submit_title(&self) -> Option<String> {
        if self.search_submit() == Some(true) {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn live_search_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.live_search_frame.get())
    }

    fn live_search(&self) -> Option<bool> {
        self.state
            .as_ref()
            .and_then(|state| state.live_search.get())
    }

    fn live_search_url(&self) -> Option<String> {
        if self.live_search() == Some(true) {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn live_search_title(&self) -> Option<String> {
        if self.live_search() == Some(true) {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn live_form_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.live_form_frame.get())
    }

    fn live_form(&self) -> Option<bool> {
        self.state.as_ref().and_then(|state| state.live_form.get())
    }

    fn live_form_url(&self) -> Option<String> {
        if self.live_form() == Some(true) {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn live_form_title(&self) -> Option<String> {
        if self.live_form() == Some(true) {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn live_link_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.live_link_frame.get())
    }

    fn live_link(&self) -> Option<bool> {
        self.state.as_ref().and_then(|state| state.live_link.get())
    }

    fn live_link_url(&self) -> Option<String> {
        if self.live_link() == Some(true) {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn live_link_title(&self) -> Option<String> {
        if self.live_link() == Some(true) {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn retained_navigation_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.retained_navigation_frame.get())
    }

    fn retained_navigation_url(&self) -> Option<String> {
        if self.retained_navigation_frame().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn retained_navigation_title(&self) -> Option<String> {
        if self.retained_navigation_frame().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn location_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.location_frame.get())
    }

    fn location_url(&self) -> Option<String> {
        if self.location_frame().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn location_title(&self) -> Option<String> {
        if self.location_frame().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn history_navigation_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.history_navigation_frame.get())
    }

    fn history_back_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.history_back_frame.get())
    }

    fn history_forward_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.history_forward_frame.get())
    }

    fn history_final_url(&self) -> Option<String> {
        if self.history_forward_frame().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn history_final_title(&self) -> Option<String> {
        if self.history_forward_frame().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn load_complete(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.load_complete.get())
    }

    fn load_url(&self) -> Option<String> {
        if self.load_complete().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn load_title(&self) -> Option<String> {
        if self.load_complete().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn reload_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.reload_frame.get())
    }

    fn reload_url(&self) -> Option<String> {
        if self.reload_frame().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string())
        } else {
            None
        }
    }

    fn reload_title(&self) -> Option<String> {
        if self.reload_frame().is_some() {
            self.state
                .as_ref()
                .and_then(|state| state.active_page_title())
        } else {
            None
        }
    }

    fn certificate_der_base64(&self) -> Option<String> {
        self.state
            .as_ref()
            .and_then(|state| state.certificate_der_base64.borrow().clone())
    }

    fn certificate_fingerprint_sha256(&self) -> Option<String> {
        self.certificate_der_base64()
            .and_then(|value| certificate_fingerprint_sha256_from_base64(&value).ok())
    }

    fn appliance_certificate_remember_result(&self) -> Option<Result<(), String>> {
        self.state
            .as_ref()
            .and_then(|state| state.appliance_certificate_remember_result.borrow().clone())
    }

    fn resource_audit_json(&self) -> Option<String> {
        self.state
            .as_ref()
            .and_then(|state| state.resource_audit_json.borrow().clone())
    }

    fn resize_frame(&self) -> Option<Duration> {
        self.state
            .as_ref()
            .and_then(|state| state.resize_frame.get())
    }

    fn tab_frame(&self) -> Option<Duration> {
        self.state.as_ref().and_then(|state| state.tab_frame.get())
    }

    fn tab_url(&self) -> Option<String> {
        self.state
            .as_ref()
            .and_then(|state| state.tab_smoke_url.borrow().clone())
    }

    fn tab_title(&self) -> Option<String> {
        self.state
            .as_ref()
            .and_then(|state| state.tab_smoke_title.borrow().clone())
    }

    fn process_host_commands(&mut self, state: &Rc<DirectServoState>) -> bool {
        let Some(rx) = self.host_command_rx.as_ref() else {
            return false;
        };
        let commands: Vec<String> = rx.try_iter().collect();
        if commands.is_empty() {
            return false;
        }
        for command in commands {
            self.handle_host_command(state, command);
        }
        true
    }

    fn handle_host_command(&mut self, state: &Rc<DirectServoState>, command: String) {
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        let (verb, rest) = command
            .split_once(' ')
            .map(|(verb, rest)| (verb, rest.trim()))
            .unwrap_or((command, ""));
        match verb {
            "navigate" => match parse_direct_navigation_target(rest) {
                Ok(url) => {
                    println!("[{}] host command navigate {}", self.log_prefix, url);
                    state.load(url);
                    state.wake_after_browser_command();
                }
                Err(error) => {
                    eprintln!(
                        "[{}] host command navigate rejected '{}': {error}",
                        self.log_prefix, rest
                    );
                }
            },
            "reload" => {
                println!("[{}] host command reload", self.log_prefix);
                state.reload();
            }
            "back" => {
                println!("[{}] host command back", self.log_prefix);
                state.go_back();
            }
            "forward" => {
                println!("[{}] host command forward", self.log_prefix);
                state.go_forward();
            }
            "new-tab" => {
                println!("[{}] host command new-tab", self.log_prefix);
                if rest.is_empty() {
                    state.new_tab();
                } else if let Ok(url) = parse_direct_navigation_target(rest) {
                    state.new_tab_with_url(url);
                }
            }
            "close-tab" => {
                println!("[{}] host command close-tab", self.log_prefix);
                state.close_tab();
            }
            "next-tab" => {
                println!("[{}] host command next-tab", self.log_prefix);
                state.next_tab();
            }
            "previous-tab" => {
                println!("[{}] host command previous-tab", self.log_prefix);
                state.previous_tab();
            }
            "select-tab" => {
                if let Ok(index) = rest.parse::<usize>() {
                    println!("[{}] host command select-tab {index}", self.log_prefix);
                    state.select_tab(index);
                }
            }
            "trust-once" => {
                println!("[{}] host command trust-once", self.log_prefix);
                state.request_appliance_certificate_trust_once();
            }
            "trust-this-appliance" => {
                println!("[{}] host command trust-this-appliance", self.log_prefix);
                state.request_appliance_certificate_remember();
            }
            "certificate-go-back" => {
                println!("[{}] host command certificate-go-back", self.log_prefix);
                state.leave_appliance_certificate_warning();
            }
            "focus-location" => {
                println!("[{}] host command focus-location", self.log_prefix);
                state.enter_location_mode();
                state.wake_after_browser_command();
            }
            other => eprintln!("[{}] unknown host command {other}", self.log_prefix),
        }
    }

    fn drive_verified_input_smoke(
        &mut self,
        state: &Rc<DirectServoState>,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if !self.scripted_verified_input_smoke || self.scripted_verified_input_done {
            return false;
        }
        if state.first_present.get().is_none() {
            return false;
        }

        if !self.scripted_verified_input_sent {
            let ready_for_input = self.started.elapsed() >= Duration::from_millis(500);
            if ready_for_input {
                let before_load_complete = !state.active_load_complete();
                self.verified_input_before_load_complete = Some(before_load_complete);
                state
                    .verified_input_before_load_complete
                    .set(Some(before_load_complete));
                self.verified_input_started = Some(Instant::now());
                self.scripted_verified_input_sent = true;
                state.click_at(VERIFIED_INPUT_CLICK_X, VERIFIED_INPUT_CLICK_Y);
                state.send_text(VERIFIED_INPUT_TEXT.to_string());
            } else if self.started.elapsed() >= self.timeout {
                state.verified_input.set(Some(false));
                self.scripted_verified_input_done = true;
                eprintln!(
                    "[{}] verified input failed before input-ready state (last title {:?})",
                    self.log_prefix,
                    state.active_page_title()
                );
                if self.smoke {
                    event_loop.exit();
                }
            } else {
            }
            return true;
        }

        if state.active_title_matches(VERIFIED_INPUT_TITLE) {
            if state.verified_input_frame.get().is_none() {
                if let Some(started) = self.verified_input_started {
                    let elapsed = started.elapsed();
                    state.verified_input_frame.set(Some(elapsed));
                    println!(
                        "[{}] verified input direct frame in {}",
                        self.log_prefix,
                        format_duration(elapsed)
                    );
                    println!(
                        "[{}] verified input before load complete {}",
                        self.log_prefix,
                        self.verified_input_before_load_complete.unwrap_or(false)
                    );
                }
            }
            if self.scripted_verified_input_repeat_smoke
                && !self.scripted_verified_input_second_sent
            {
                self.verified_input_second_started = Some(Instant::now());
                self.scripted_verified_input_second_sent = true;
                state.send_text(VERIFIED_INPUT_REPEAT_TEXT.to_string());
                return true;
            }
            if !self.scripted_verified_input_repeat_smoke {
                state.verified_input.set(Some(true));
                self.scripted_verified_input_done = true;
                println!("[{}] verified input true", self.log_prefix);
                if self.smoke {
                    event_loop.exit();
                }
            }
        } else if self.scripted_verified_input_repeat_smoke
            && self.scripted_verified_input_second_sent
            && state.active_title_matches(VERIFIED_INPUT_REPEAT_TITLE)
        {
            if let Some(started) = self.verified_input_second_started {
                let elapsed = started.elapsed();
                state.verified_input_second_frame.set(Some(elapsed));
                state.verified_input.set(Some(true));
                self.scripted_verified_input_done = true;
                println!(
                    "[{}] verified repeat input direct frame in {}",
                    self.log_prefix,
                    format_duration(elapsed)
                );
                println!("[{}] verified input true", self.log_prefix);
                if self.smoke {
                    event_loop.exit();
                }
            }
        } else {
            let timed_out = self
                .verified_input_started
                .map(|started| started.elapsed() >= self.timeout)
                .unwrap_or(false);
            if timed_out {
                state.verified_input.set(Some(false));
                self.scripted_verified_input_done = true;
                eprintln!(
                    "[{}] verified input failed before title '{}' (last title {:?})",
                    self.log_prefix,
                    VERIFIED_INPUT_TITLE,
                    state.active_page_title()
                );
                if self.smoke {
                    event_loop.exit();
                }
            } else {
            }
        }
        true
    }

    fn drive_search_submit_smoke(
        &mut self,
        state: &Rc<DirectServoState>,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if !self.scripted_search_submit_smoke || self.scripted_search_submit_done {
            return false;
        }
        if state.first_present.get().is_none() {
            return false;
        }

        if !self.scripted_search_submit_sent {
            let ready_for_input = self.started.elapsed() >= Duration::from_millis(500);
            if ready_for_input {
                self.search_submit_started = Some(Instant::now());
                self.scripted_search_submit_sent = true;
                state.click_at(VERIFIED_INPUT_CLICK_X, VERIFIED_INPUT_CLICK_Y);
                state.send_text(SEARCH_SUBMIT_TEXT.to_string());
                state.send_named_key(ServoNamedKey::Enter);
            } else if self.started.elapsed() >= self.timeout {
                state.search_submit.set(Some(false));
                self.scripted_search_submit_done = true;
                eprintln!(
                    "[{}] search submit failed before input-ready state (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            }
            return true;
        }

        let verified = state.active_title_matches(SEARCH_SUBMIT_TITLE)
            || state
                .active_url()
                .map(|url| url.as_str().contains("q=sextant"))
                .unwrap_or(false);
        if verified {
            if let Some(started) = self.search_submit_started {
                let elapsed = started.elapsed();
                state.search_submit_frame.set(Some(elapsed));
                state.search_submit.set(Some(true));
                self.scripted_search_submit_done = true;
                println!(
                    "[{}] search submit direct frame in {}",
                    self.log_prefix,
                    format_duration(elapsed)
                );
                println!("[{}] search submit true", self.log_prefix);
                if let Some(url) = state.active_url() {
                    println!("[{}] search submit url {}", self.log_prefix, url);
                }
                if let Some(title) = state.active_page_title() {
                    println!("[{}] search submit title {}", self.log_prefix, title);
                }
                if self.smoke {
                    event_loop.exit();
                }
            }
        } else {
            let timed_out = self
                .search_submit_started
                .map(|started| started.elapsed() >= self.timeout)
                .unwrap_or(false);
            if timed_out {
                state.search_submit.set(Some(false));
                self.scripted_search_submit_done = true;
                eprintln!(
                    "[{}] search submit failed before query verification (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            }
        }
        true
    }

    fn drive_live_search_smoke(
        &mut self,
        state: &Rc<DirectServoState>,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if !self.scripted_live_search_smoke || self.scripted_live_search_done {
            return false;
        }
        if state.first_present.get().is_none() {
            return false;
        }

        if !self.scripted_live_search_sent {
            let ready_for_input = self.started.elapsed() >= LIVE_SEARCH_READY_DELAY
                && state
                    .active_page_title()
                    .map(|title| !title.trim().is_empty())
                    .unwrap_or(false);
            if ready_for_input {
                self.live_search_started = Some(Instant::now());
                self.send_live_search_attempt(state);
                self.scripted_live_search_sent = true;
            } else if self.started.elapsed() >= self.timeout {
                state.live_search.set(Some(false));
                self.scripted_live_search_done = true;
                eprintln!(
                    "[{}] live search failed before input-ready state (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            }
            return true;
        }

        if state.active_live_search_verified() {
            if let Some(started) = self.live_search_started {
                let elapsed = started.elapsed();
                state.live_search_frame.set(Some(elapsed));
                state.live_search.set(Some(true));
                self.scripted_live_search_done = true;
                println!(
                    "[{}] live search direct frame in {}",
                    self.log_prefix,
                    format_duration(elapsed)
                );
                println!("[{}] live search true", self.log_prefix);
                if let Some(url) = state.active_url() {
                    println!("[{}] live search url {}", self.log_prefix, url);
                }
                if let Some(title) = state.active_page_title() {
                    println!("[{}] live search title {}", self.log_prefix, title);
                }
                if self.smoke {
                    event_loop.exit();
                }
            }
        } else {
            if self
                .live_search_pending_text_since
                .map(|started| started.elapsed() >= LIVE_SEARCH_CLICK_SETTLE_DELAY)
                .unwrap_or(false)
            {
                self.live_search_pending_text_since = None;
                self.send_live_search_text(state);
                state.window.request_redraw();
                return true;
            }
            if !self.live_search_dom_navigation_used
                && self.live_search_attempts >= 2
                && self
                    .live_search_last_attempt
                    .map(|started| started.elapsed() >= LIVE_SEARCH_CLICK_SETTLE_DELAY)
                    .unwrap_or(false)
            {
                let target = state.live_search_dom_fallback_url.borrow().clone();
                if let Some(target) = target {
                    self.live_search_dom_navigation_used = true;
                    println!(
                        "[{}] live search attempt dom form navigation {}",
                        self.log_prefix, target
                    );
                    state.load(target);
                    state.window.request_redraw();
                    return true;
                }
            }
            let should_retry = self.live_search_attempts < 5
                && self
                    .live_search_last_attempt
                    .map(|started| started.elapsed() >= LIVE_SEARCH_FALLBACK_DELAY)
                    .unwrap_or(false);
            if should_retry {
                self.send_live_search_attempt(state);
                state.window.request_redraw();
                return true;
            }
            let timed_out = self
                .live_search_started
                .map(|started| started.elapsed() >= self.timeout)
                .unwrap_or(false);
            if timed_out {
                state.live_search.set(Some(false));
                self.scripted_live_search_done = true;
                eprintln!(
                    "[{}] live search failed before query verification (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            }
        }
        true
    }

    fn drive_live_form_smoke(
        &mut self,
        state: &Rc<DirectServoState>,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if !self.scripted_live_form_smoke || self.scripted_live_form_done {
            return false;
        }
        if state.first_present.get().is_none() {
            return false;
        }

        if !self.scripted_live_form_sent {
            let ready_for_input =
                self.started.elapsed() >= LIVE_FORM_READY_DELAY && state.active_load_complete();
            if ready_for_input {
                self.live_form_started = Some(Instant::now());
                self.scripted_live_form_sent = true;
                println!(
                    "[{}] live form attempt dom form field click",
                    self.log_prefix
                );
                if state.click_live_form_field(self.log_prefix) {
                    self.live_form_submit_pending_since = Some(Instant::now());
                } else {
                    state.live_form.set(Some(false));
                    self.scripted_live_form_done = true;
                    eprintln!(
                        "[{}] live form failed before input target (last title {:?}, last url {:?})",
                        self.log_prefix,
                        state.active_page_title(),
                        state.active_url()
                    );
                    if self.smoke {
                        event_loop.exit();
                    }
                }
            } else if self.started.elapsed() >= self.timeout {
                state.live_form.set(Some(false));
                self.scripted_live_form_done = true;
                eprintln!(
                    "[{}] live form failed before input-ready state (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            }
            return true;
        }

        if state.active_live_form_verified() {
            if let Some(started) = self.live_form_started {
                let elapsed = started.elapsed();
                state.live_form_frame.set(Some(elapsed));
                state.live_form.set(Some(true));
                self.scripted_live_form_done = true;
                println!(
                    "[{}] live form direct frame in {}",
                    self.log_prefix,
                    format_duration(elapsed)
                );
                println!("[{}] live form true", self.log_prefix);
                if let Some(url) = state.active_url() {
                    println!("[{}] live form url {}", self.log_prefix, url);
                }
                if let Some(title) = state.active_page_title() {
                    println!("[{}] live form title {}", self.log_prefix, title);
                }
                if self.smoke {
                    event_loop.exit();
                }
            }
        } else {
            if self
                .live_form_submit_pending_since
                .map(|started| started.elapsed() >= LIVE_FORM_CLICK_SETTLE_DELAY)
                .unwrap_or(false)
            {
                self.live_form_submit_pending_since = None;
                if let Some((x, y)) = state.live_form_submit_target.borrow_mut().take() {
                    println!("[{}] live form attempt submit click", self.log_prefix);
                    state.click_at(x, y);
                } else {
                    println!("[{}] live form attempt submit enter", self.log_prefix);
                    state.send_named_key(ServoNamedKey::Enter);
                }
                state.window.request_redraw();
                return true;
            }
            let timed_out = self
                .live_form_started
                .map(|started| started.elapsed() >= self.timeout)
                .unwrap_or(false);
            if timed_out {
                state.live_form.set(Some(false));
                self.scripted_live_form_done = true;
                eprintln!(
                    "[{}] live form failed before post verification (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            }
        }
        true
    }

    fn drive_live_link_smoke(
        &mut self,
        state: &Rc<DirectServoState>,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if !self.scripted_live_link_smoke || self.scripted_live_link_done {
            return false;
        }
        if state.first_present.get().is_none() {
            return false;
        }

        if !self.scripted_live_link_sent {
            let ready_for_click =
                self.started.elapsed() >= LIVE_LINK_READY_DELAY && state.active_load_complete();
            if ready_for_click {
                self.live_link_started = Some(Instant::now());
                self.scripted_live_link_sent = true;
                println!("[{}] live link attempt dom anchor click", self.log_prefix);
                if !state.click_live_link(self.log_prefix) {
                    state.live_link.set(Some(false));
                    self.scripted_live_link_done = true;
                    eprintln!(
                        "[{}] live link failed before anchor target (last title {:?}, last url {:?})",
                        self.log_prefix,
                        state.active_page_title(),
                        state.active_url()
                    );
                    if self.smoke {
                        event_loop.exit();
                    }
                }
            } else if self.started.elapsed() >= self.timeout {
                state.live_link.set(Some(false));
                self.scripted_live_link_done = true;
                eprintln!(
                    "[{}] live link failed before click-ready state (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            }
            return true;
        }

        if state.active_live_link_verified() {
            if let Some(started) = self.live_link_started {
                let elapsed = started.elapsed();
                state.live_link_frame.set(Some(elapsed));
                state.live_link.set(Some(true));
                self.scripted_live_link_done = true;
                println!(
                    "[{}] live link direct frame in {}",
                    self.log_prefix,
                    format_duration(elapsed)
                );
                println!("[{}] live link true", self.log_prefix);
                if let Some(url) = state.active_url() {
                    println!("[{}] live link url {}", self.log_prefix, url);
                }
                if let Some(title) = state.active_page_title() {
                    println!("[{}] live link title {}", self.log_prefix, title);
                }
                if self.smoke {
                    event_loop.exit();
                }
            }
        } else {
            let timed_out = self
                .live_link_started
                .map(|started| started.elapsed() >= self.timeout)
                .unwrap_or(false);
            if timed_out {
                state.live_link.set(Some(false));
                self.scripted_live_link_done = true;
                eprintln!(
                    "[{}] live link failed before navigation (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            } else {
                state.window.request_redraw();
            }
        }
        true
    }

    fn send_live_search_attempt(&mut self, state: &Rc<DirectServoState>) {
        self.live_search_attempts = self.live_search_attempts.saturating_add(1);
        self.live_search_last_attempt = Some(Instant::now());
        match self.live_search_attempts {
            1 => {
                println!("[{}] live search attempt keyboard focus", self.log_prefix);
                state.send_named_key(ServoNamedKey::Tab);
                self.send_live_search_text(state);
            }
            2 => {
                println!(
                    "[{}] live search attempt dom search box click",
                    self.log_prefix
                );
                if !state.click_live_search_box(self.log_prefix) {
                    println!(
                        "[{}] live search dom search box unavailable; using fixed search box click",
                        self.log_prefix
                    );
                    state.click_at(
                        LIVE_SEARCH_FALLBACK_CLICK_X,
                        self.live_search_click_y(state),
                    );
                    self.live_search_pending_text_since = Some(Instant::now());
                }
            }
            3 => {
                println!("[{}] live search attempt search box click", self.log_prefix);
                state.click_at(
                    LIVE_SEARCH_FALLBACK_CLICK_X,
                    self.live_search_click_y(state),
                );
                self.live_search_pending_text_since = Some(Instant::now());
            }
            4 => {
                println!(
                    "[{}] live search attempt keyboard focus sweep",
                    self.log_prefix
                );
                for _ in 0..5 {
                    state.send_named_key(ServoNamedKey::Tab);
                }
                self.send_live_search_text(state);
            }
            _ => {
                println!("[{}] live search attempt center click", self.log_prefix);
                state.click_at(LIVE_SEARCH_FALLBACK_CLICK_X, LIVE_SEARCH_FALLBACK_CLICK_Y);
                self.live_search_pending_text_since = Some(Instant::now());
            }
        }
    }

    fn drive_retained_navigation_smoke(
        &mut self,
        state: &Rc<DirectServoState>,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if !self.scripted_retained_navigation_smoke || self.scripted_retained_navigation_done {
            return false;
        }
        if state.first_present.get().is_none() {
            return false;
        }

        if !self.scripted_retained_navigation_sent {
            let ready_for_click = self.started.elapsed() >= Duration::from_millis(500)
                && state.active_load_complete();
            if ready_for_click {
                self.retained_navigation_started = Some(Instant::now());
                self.scripted_retained_navigation_sent = true;
                state.click_at(RETAINED_NAVIGATION_CLICK_X, RETAINED_NAVIGATION_CLICK_Y);
            } else if self.started.elapsed() >= self.timeout {
                self.scripted_retained_navigation_done = true;
                eprintln!(
                    "[{}] retained navigation failed before click-ready state (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            } else {
                state.window.request_redraw();
            }
            return true;
        }

        let verified = state.active_title_matches(RETAINED_NAVIGATION_TARGET_TITLE)
            && state
                .active_url()
                .map(|url| url.path().ends_with("/next"))
                .unwrap_or(false);
        if verified {
            if let Some(started) = self.retained_navigation_started {
                let elapsed = started.elapsed();
                state.retained_navigation_frame.set(Some(elapsed));
                self.scripted_retained_navigation_done = true;
                println!(
                    "[{}] retained navigation direct frame in {}",
                    self.log_prefix,
                    format_duration(elapsed)
                );
                if let Some(url) = state.active_url() {
                    println!("[{}] retained navigation url {}", self.log_prefix, url);
                }
                if let Some(title) = state.active_page_title() {
                    println!("[{}] retained navigation title {}", self.log_prefix, title);
                }
                if self.smoke {
                    event_loop.exit();
                }
            }
        } else {
            let timed_out = self
                .retained_navigation_started
                .map(|started| started.elapsed() >= self.timeout)
                .unwrap_or(false);
            if timed_out {
                self.scripted_retained_navigation_done = true;
                eprintln!(
                    "[{}] retained navigation failed before target verification (last title {:?}, last url {:?})",
                    self.log_prefix,
                    state.active_page_title(),
                    state.active_url()
                );
                if self.smoke {
                    event_loop.exit();
                }
            } else {
                state.window.request_redraw();
            }
        }
        true
    }

    fn send_live_search_text(&self, state: &Rc<DirectServoState>) {
        state.send_text(LIVE_SEARCH_TEXT.to_string());
        state.send_named_key(ServoNamedKey::Enter);
    }

    fn live_search_click_y(&self, state: &DirectServoState) -> f32 {
        if state
            .active_url()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .map(|host| host.contains("google."))
            .unwrap_or(false)
        {
            LIVE_SEARCH_GOOGLE_CLICK_Y
        } else {
            LIVE_SEARCH_FALLBACK_CLICK_Y
        }
    }
}

impl ApplicationHandler<DirectServoUserEvent> for DirectServoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let display_handle = event_loop
            .display_handle()
            .expect("failed to get display handle");
        let embed_bounds = self.embed_bounds.unwrap_or(DirectEmbedBounds {
            x: 0,
            y: 0,
            width: 1180,
            height: 760,
        });
        let mut window_attributes = WindowAttributes::default()
            .with_title(self.title.clone())
            .with_inner_size(PhysicalSize::new(embed_bounds.width, embed_bounds.height));
        if let Some(parent_hwnd) = self.embed_parent_hwnd {
            if let Some(hwnd) = NonZeroIsize::new(parent_hwnd) {
                window_attributes = window_attributes
                    .with_decorations(false)
                    .with_resizable(false)
                    .with_position(Position::Physical(PhysicalPosition::new(
                        embed_bounds.x,
                        embed_bounds.y,
                    )));
                let parent = RawWindowHandle::Win32(Win32WindowHandle::new(hwnd));
                window_attributes = unsafe { window_attributes.with_parent_window(Some(parent)) };
            }
        }
        let window = event_loop
            .create_window(window_attributes)
            .expect("failed to create direct Servo window");
        force_focus_window(&window);
        let initial_window_size = window.inner_size();
        let window_handle = window.window_handle().expect("failed to get window handle");
        let rendering_context = Rc::new(
            WindowRenderingContext::new(display_handle, window_handle, initial_window_size)
                .expect("failed to create Servo window rendering context"),
        );
        rendering_context
            .make_current()
            .expect("failed to activate Servo window rendering context");

        let mut opts = Opts::default();
        opts.config_dir = self.config_dir.clone();
        opts.certificate_path = self
            .certificate_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string());
        opts.ignore_certificate_errors = self.ignore_certificate_errors;
        let mut preferences = Preferences::default();
        preferences.shell_background_color_rgba = DIRECT_SHELL_BACKGROUND_RGBA;
        preferences.user_agent = env::var("SEXTANT_DIRECT_USER_AGENT")
            .unwrap_or_else(|_| DIRECT_COMPAT_USER_AGENT.to_string());
        if self.disable_http_cache {
            preferences.network_http_cache_disabled = true;
        }
        if self.bypass_proxy_for_target {
            if let Some(host) = self.target.host_str() {
                append_no_proxy_host(&mut preferences.network_http_no_proxy, host);
            }
        }
        let servo = ServoBuilder::default()
            .opts(opts)
            .preferences(preferences)
            .build();
        if self.setup_servo_logging {
            servo.setup_logging();
        }

        let state = Rc::new(DirectServoState {
            window,
            servo,
            rendering_context,
            self_handle: RefCell::new(Weak::new()),
            log_prefix: self.log_prefix,
            webviews: RefCell::new(Vec::new()),
            active_index: Cell::new(0),
            pending_navigation: RefCell::new(None),
            active_navigation_started: Cell::new(Some(Instant::now())),
            frame_ready: Cell::new(false),
            first_present: Cell::new(None),
            direct_frame_count: Cell::new(0),
            direct_slow_frame_count: Cell::new(0),
            direct_max_frame: Cell::new(None),
            direct_max_spin: Cell::new(None),
            direct_max_paint: Cell::new(None),
            direct_max_present: Cell::new(None),
            last_direct_present_at: Cell::new(None),
            first_interaction_frame: Cell::new(None),
            first_interaction_before_load_complete: Cell::new(None),
            verified_input_frame: Cell::new(None),
            verified_input_second_frame: Cell::new(None),
            verified_input: Cell::new(None),
            verified_input_before_load_complete: Cell::new(None),
            search_submit_frame: Cell::new(None),
            search_submit: Cell::new(None),
            live_search_frame: Cell::new(None),
            live_search: Cell::new(None),
            live_search_dom_fallback_url: RefCell::new(None),
            live_form_frame: Cell::new(None),
            live_form: Cell::new(None),
            live_form_submit_target: RefCell::new(None),
            live_link_frame: Cell::new(None),
            live_link: Cell::new(None),
            live_link_target: RefCell::new(None),
            live_link_expected_host: RefCell::new(None),
            retained_navigation_frame: Cell::new(None),
            input_frame: Cell::new(None),
            location_frame: Cell::new(None),
            history_navigation_frame: Cell::new(None),
            history_back_frame: Cell::new(None),
            history_forward_frame: Cell::new(None),
            load_complete: Cell::new(None),
            reload_frame: Cell::new(None),
            certificate_probe_started: Cell::new(false),
            certificate_page_decorated: Cell::new(false),
            certificate_der_base64: Rc::new(RefCell::new(None)),
            appliance_certificate_actions: self.appliance_certificate_actions.clone(),
            appliance_certificate_trust_once_pending: Cell::new(false),
            appliance_certificate_remember_pending: Cell::new(false),
            appliance_certificate_remember_result: Rc::new(RefCell::new(None)),
            resource_audit: self.resource_audit,
            resource_audit_started: Rc::new(Cell::new(false)),
            resource_audit_last_started: Cell::new(None),
            resource_audit_json: Rc::new(RefCell::new(None)),
            text_paint_warmup_started: Cell::new(false),
            resize_frame: Cell::new(None),
            tab_frame: Cell::new(None),
            tab_smoke_url: RefCell::new(None),
            tab_smoke_title: RefCell::new(None),
            last_chrome_status: RefCell::new(String::new()),
            last_window_size: Cell::new(initial_window_size),
            cursor_position: Cell::new((0.0, 0.0)),
            last_mouse_move_sent: Cell::new(None),
            mouse_button_down: Cell::new(false),
            input_events: Cell::new(0),
            last_input_at: Cell::new(None),
            modifiers: Cell::new(ModifiersState::empty()),
            location_active: Cell::new(false),
            location_replace_on_text: Cell::new(false),
            location_buffer: RefCell::new(String::new()),
        });
        *state.self_handle.borrow_mut() = Rc::downgrade(&state);

        let webview = state.create_webview(self.target.clone());
        state.webviews.borrow_mut().push(webview);
        state.active_index.set(0);
        state.focus_active_webview();
        state.update_window_title();
        state.window.request_redraw();
        self.state = Some(state);
        println!(
            "[{}] opening {} with Servo WindowRenderingContext",
            self.log_prefix, self.target
        );
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: DirectServoUserEvent) {
        match event {
            DirectServoUserEvent::HostCommand => {
                if let Some(state) = self.state.clone() {
                    if self.process_host_commands(&state) {
                        state.window.request_redraw();
                        event_loop.set_control_flow(ControlFlow::Poll);
                    }
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(state) = self.state.as_ref() {
                    state.handle_mouse_move(position.x as f32, position.y as f32);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(direct_state) = self.state.as_ref() {
                    force_focus_window(&direct_state.window);
                    direct_state.handle_mouse_button(button, state);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(state) = self.state.as_ref() {
                    state.handle_mouse_wheel(delta);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                if let Some(state) = self.state.as_ref() {
                    state.modifiers.set(modifiers.state());
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(state) = self.state.as_ref() {
                    state.handle_keyboard(event);
                }
            }
            WindowEvent::Ime(winit30::event::Ime::Commit(text)) => {
                if let Some(state) = self.state.as_ref() {
                    state.send_text(text);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(state) = self.state.clone() {
                    {
                        let frame_start = Instant::now();
                        state.servo.spin_event_loop();
                        let after_spin = Instant::now();
                        if let Err(error) = state.rendering_context.make_current() {
                            eprintln!(
                                "[{}] failed to activate Servo rendering context before paint: {error:?}",
                                self.log_prefix
                            );
                        }
                        let after_make_current = Instant::now();
                        if let Some(webview) = state.active_webview() {
                            webview.paint();
                        }
                        let after_paint = Instant::now();
                        state.rendering_context.present();
                        let after_present = Instant::now();
                        state.last_direct_present_at.set(Some(after_present));
                        state.record_direct_frame_timing(
                            after_present.duration_since(frame_start),
                            after_spin.duration_since(frame_start),
                            after_paint.duration_since(after_make_current),
                            after_present.duration_since(after_paint),
                        );
                    }
                    if state.complete_pending_appliance_certificate_decision() {
                        event_loop.exit();
                        return;
                    }
                    if state.active_load_complete() && state.resource_audit_due() {
                        if let Some(webview) = state.active_webview() {
                            state.capture_resource_audit(webview);
                        }
                    }
                    if state.first_present.get().is_none() {
                        let elapsed = self.started.elapsed();
                        state.first_present.set(Some(elapsed));
                        println!(
                            "[{}] first direct present in {}",
                            self.log_prefix,
                            format_duration(elapsed)
                        );
                        if self.scripted_tab_smoke {
                            self.tab_started = Some(Instant::now());
                            self.scripted_tab_smoke_started = true;
                            state.run_tab_smoke();
                        } else if self.scripted_resize_smoke {
                            self.resize_started = Some(Instant::now());
                            self.scripted_resize_started = true;
                            state.request_window_resize(self.resize_target);
                        } else if self.scripted_first_interaction_smoke {
                            let before_load_complete = !state.active_load_complete();
                            self.first_interaction_before_load_complete =
                                Some(before_load_complete);
                            state
                                .first_interaction_before_load_complete
                                .set(Some(before_load_complete));
                            self.first_interaction_started = Some(Instant::now());
                            self.scripted_first_interaction_sent = true;
                            state.send_text("sextant".to_string());
                        } else if self.scripted_verified_input_smoke {
                            let settle_until = Instant::now() + Duration::from_millis(500);
                            while Instant::now() < settle_until {
                                state.servo.spin_event_loop();
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            let before_load_complete = !state.active_load_complete();
                            self.verified_input_before_load_complete = Some(before_load_complete);
                            state
                                .verified_input_before_load_complete
                                .set(Some(before_load_complete));
                            self.verified_input_started = Some(Instant::now());
                            self.scripted_verified_input_sent = true;
                            state.click_at(VERIFIED_INPUT_CLICK_X, VERIFIED_INPUT_CLICK_Y);
                            state.send_text(VERIFIED_INPUT_TEXT.to_string());
                            state.window.request_redraw();
                        } else if self.scripted_search_submit_smoke {
                            let settle_until = Instant::now() + Duration::from_millis(500);
                            while Instant::now() < settle_until {
                                state.servo.spin_event_loop();
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            self.search_submit_started = Some(Instant::now());
                            self.scripted_search_submit_sent = true;
                            state.click_at(VERIFIED_INPUT_CLICK_X, VERIFIED_INPUT_CLICK_Y);
                            state.send_text(SEARCH_SUBMIT_TEXT.to_string());
                            state.send_named_key(ServoNamedKey::Enter);
                            state.window.request_redraw();
                        } else if self.scripted_live_search_smoke {
                            state.window.request_redraw();
                        } else if self.scripted_live_form_smoke {
                            state.window.request_redraw();
                        } else if self.scripted_live_link_smoke {
                            state.window.request_redraw();
                        } else if self.scripted_retained_navigation_smoke {
                            state.window.request_redraw();
                        } else if self.scripted_history.is_some() {
                            self.history_phase = Some(HistorySmokePhase::WaitingInitial);
                            state.window.request_redraw();
                        } else if self.scripted_load_smoke {
                            state.window.request_redraw();
                        } else if self.scripted_reload_smoke {
                            state.window.request_redraw();
                        } else if self.scripted_location.is_some() {
                            state.window.request_redraw();
                        } else if self.scripted_text.is_some() {
                            state.window.request_redraw();
                        } else if self.smoke {
                            event_loop.exit();
                        }
                    } else if self.scripted_tab_smoke_started && !self.scripted_tab_smoke_done {
                        if let Some(tab_started) = self.tab_started {
                            if !state.active_tab_smoke_ready() {
                                state.window.request_redraw();
                                return;
                            }
                            let elapsed = tab_started.elapsed();
                            state.tab_frame.set(Some(elapsed));
                            self.scripted_tab_smoke_done = true;
                            println!(
                                "[{}] tab direct frame in {}",
                                self.log_prefix,
                                format_duration(elapsed)
                            );
                            if let Some(url) = state.active_url() {
                                println!("[{}] tab url {}", self.log_prefix, url);
                                *state.tab_smoke_url.borrow_mut() = Some(url.to_string());
                            }
                            if let Some(title) = state.active_page_title() {
                                println!("[{}] tab title {}", self.log_prefix, title);
                                *state.tab_smoke_title.borrow_mut() = Some(title);
                            }
                            state.close_tab();
                            if self.smoke {
                                event_loop.exit();
                            }
                        }
                    } else if self.scripted_resize_started && !self.scripted_resize_done {
                        if let Some(resize_started) = self.resize_started {
                            if state.last_window_size.get() == self.resize_target {
                                let elapsed = resize_started.elapsed();
                                state.resize_frame.set(Some(elapsed));
                                self.scripted_resize_done = true;
                                println!(
                                    "[{}] resize direct frame in {}",
                                    self.log_prefix,
                                    format_duration(elapsed)
                                );
                                if self.smoke {
                                    event_loop.exit();
                                }
                            } else {
                                state.window.request_redraw();
                            }
                        }
                    } else if self.scripted_first_interaction_sent
                        && state.first_interaction_frame.get().is_none()
                    {
                        if let Some(started) = self.first_interaction_started {
                            let elapsed = started.elapsed();
                            state.first_interaction_frame.set(Some(elapsed));
                            println!(
                                "[{}] first interaction direct frame in {}",
                                self.log_prefix,
                                format_duration(elapsed)
                            );
                            println!(
                                "[{}] first interaction before load complete {}",
                                self.log_prefix,
                                self.first_interaction_before_load_complete.unwrap_or(false)
                            );
                            if self.smoke {
                                event_loop.exit();
                            }
                        }
                    } else if self.drive_verified_input_smoke(&state, event_loop) {
                    } else if self.drive_search_submit_smoke(&state, event_loop) {
                    } else if self.scripted_load_smoke && !self.scripted_load_done {
                        if state.active_load_complete() {
                            if state.active_page_title().as_deref() == Some("Certificate error")
                                && state.certificate_der_base64.borrow().is_none()
                            {
                                if let Some(webview) = state.active_webview() {
                                    state.capture_bad_certificate_bytes(webview);
                                }
                                state.window.request_redraw();
                                return;
                            }
                            if state.resource_audit && state.resource_audit_json.borrow().is_none()
                            {
                                if state.resource_audit_due() {
                                    if let Some(webview) = state.active_webview() {
                                        state.capture_resource_audit(webview);
                                    }
                                }
                                self.keep_scripted_load_pumping(event_loop);
                                return;
                            }
                            let elapsed = self.started.elapsed();
                            state.load_complete.set(Some(elapsed));
                            self.scripted_load_done = true;
                            println!(
                                "[{}] load complete in {}",
                                self.log_prefix,
                                format_duration(elapsed)
                            );
                            if let Some(url) = state.active_url() {
                                println!("[{}] load url {}", self.log_prefix, url);
                            }
                            if let Some(title) = state.active_page_title() {
                                println!("[{}] load title {}", self.log_prefix, title);
                            }
                            if self.smoke {
                                event_loop.exit();
                            }
                        } else {
                            self.keep_scripted_load_pumping(event_loop);
                        }
                    } else if self.history_phase == Some(HistorySmokePhase::WaitingInitial) {
                        if let Some(target) = self.scripted_history.clone() {
                            if state.active_load_complete() {
                                self.history_initial = state.active_url();
                                match state.run_location_smoke(&target) {
                                    Ok(expected) => {
                                        self.history_started = Some(Instant::now());
                                        self.history_expected = Some(expected);
                                        self.history_phase =
                                            Some(HistorySmokePhase::NavigatingSecond);
                                    }
                                    Err(error) => {
                                        eprintln!(
                                            "[{}] history smoke failed: {error}",
                                            self.log_prefix
                                        );
                                        if self.smoke {
                                            event_loop.exit();
                                        }
                                    }
                                }
                            } else {
                                state.window.request_redraw();
                            }
                        }
                    } else if self.history_phase == Some(HistorySmokePhase::NavigatingSecond) {
                        if let (Some(started), Some(expected)) =
                            (self.history_started, self.history_expected.as_ref())
                        {
                            if state.active_url_matches(expected)
                                && state.active_load_complete()
                                && state.can_go_back()
                            {
                                let elapsed = started.elapsed();
                                state.history_navigation_frame.set(Some(elapsed));
                                println!(
                                    "[{}] history navigation direct frame in {}",
                                    self.log_prefix,
                                    format_duration(elapsed)
                                );
                                self.history_back_started = Some(Instant::now());
                                self.history_phase = Some(HistorySmokePhase::Back);
                                state.go_back();
                            } else {
                                state.window.request_redraw();
                            }
                        }
                    } else if self.history_phase == Some(HistorySmokePhase::Back) {
                        if let (Some(started), Some(initial)) =
                            (self.history_back_started, self.history_initial.as_ref())
                        {
                            if state.active_url_matches(initial)
                                && state.active_load_complete()
                                && state.can_go_forward()
                            {
                                let elapsed = started.elapsed();
                                state.history_back_frame.set(Some(elapsed));
                                println!(
                                    "[{}] history back direct frame in {}",
                                    self.log_prefix,
                                    format_duration(elapsed)
                                );
                                self.history_forward_started = Some(Instant::now());
                                self.history_phase = Some(HistorySmokePhase::Forward);
                                state.go_forward();
                            } else {
                                state.window.request_redraw();
                            }
                        }
                    } else if self.history_phase == Some(HistorySmokePhase::Forward) {
                        if let (Some(started), Some(expected)) =
                            (self.history_forward_started, self.history_expected.as_ref())
                        {
                            if state.active_url_matches(expected) && state.active_load_complete() {
                                let elapsed = started.elapsed();
                                state.history_forward_frame.set(Some(elapsed));
                                self.history_phase = Some(HistorySmokePhase::Done);
                                println!(
                                    "[{}] history forward direct frame in {}",
                                    self.log_prefix,
                                    format_duration(elapsed)
                                );
                                if let Some(url) = state.active_url() {
                                    println!("[{}] history final url {}", self.log_prefix, url);
                                }
                                if let Some(title) = state.active_page_title() {
                                    println!("[{}] history final title {}", self.log_prefix, title);
                                }
                                if self.smoke {
                                    event_loop.exit();
                                }
                            } else {
                                state.window.request_redraw();
                            }
                        }
                    } else if self.scripted_reload_smoke && !self.scripted_reload_started {
                        if state.active_load_complete() {
                            self.reload_started = Some(Instant::now());
                            self.scripted_reload_started = true;
                            state.reload();
                        } else {
                            state.window.request_redraw();
                        }
                    } else if self.scripted_reload_started && !self.scripted_reload_done {
                        if let Some(started) = self.reload_started {
                            let elapsed = started.elapsed();
                            state.reload_frame.set(Some(elapsed));
                            self.scripted_reload_done = true;
                            println!(
                                "[{}] reload direct frame in {}",
                                self.log_prefix,
                                format_duration(elapsed)
                            );
                            if let Some(url) = state.active_url() {
                                println!("[{}] reload url {}", self.log_prefix, url);
                            }
                            if let Some(title) = state.active_page_title() {
                                println!("[{}] reload title {}", self.log_prefix, title);
                            }
                            if self.smoke {
                                event_loop.exit();
                            }
                        }
                    } else if self.scripted_location.is_some() && !self.scripted_location_started {
                        if let Some(target) = self.scripted_location.clone() {
                            if state.active_load_complete() {
                                match state.run_location_smoke(&target) {
                                    Ok(expected) => {
                                        self.location_started = Some(Instant::now());
                                        self.location_expected = Some(expected);
                                        self.scripted_location_started = true;
                                    }
                                    Err(error) => {
                                        eprintln!(
                                            "[{}] location smoke failed: {error}",
                                            self.log_prefix
                                        );
                                        if self.smoke {
                                            event_loop.exit();
                                        }
                                    }
                                }
                            } else {
                                state.window.request_redraw();
                            }
                        }
                    } else if self.scripted_location_started && !self.scripted_location_done {
                        if let (Some(location_started), Some(expected)) =
                            (self.location_started, self.location_expected.as_ref())
                        {
                            if state.active_url_matches(expected) && state.active_load_complete() {
                                let elapsed = location_started.elapsed();
                                state.location_frame.set(Some(elapsed));
                                self.scripted_location_done = true;
                                println!(
                                    "[{}] location direct frame in {}",
                                    self.log_prefix,
                                    format_duration(elapsed)
                                );
                                if let Some(url) = state.active_url() {
                                    println!("[{}] location url {}", self.log_prefix, url);
                                }
                                if let Some(title) = state.active_page_title() {
                                    println!("[{}] location title {}", self.log_prefix, title);
                                }
                                if self.smoke {
                                    event_loop.exit();
                                }
                            } else {
                                state.window.request_redraw();
                            }
                        }
                    } else if self.drive_retained_navigation_smoke(&state, event_loop) {
                    } else if self.drive_live_search_smoke(&state, event_loop) {
                    } else if self.drive_live_form_smoke(&state, event_loop) {
                    } else if self.drive_live_link_smoke(&state, event_loop) {
                    } else if self.scripted_text.is_some() && !self.scripted_text_sent {
                        if let Some(text) = self.scripted_text.clone() {
                            if state.active_load_complete() {
                                self.input_started = Some(Instant::now());
                                self.scripted_text_sent = true;
                                state.send_text(text);
                            } else {
                                state.window.request_redraw();
                            }
                        }
                    } else if self.scripted_text_sent && state.input_frame.get().is_none() {
                        if let Some(input_started) = self.input_started {
                            let elapsed = input_started.elapsed();
                            state.input_frame.set(Some(elapsed));
                            println!(
                                "[{}] input direct frame in {}",
                                self.log_prefix,
                                format_duration(elapsed)
                            );
                            if self.smoke {
                                event_loop.exit();
                            }
                        }
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(state) = self.state.as_ref() {
                    state.resize_all_webviews(size);
                    state.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.clone() {
            state.servo.spin_event_loop();
            let host_command_processed = self.process_host_commands(&state);
            let mut frame_redraw_deferred = false;
            if state.frame_ready.replace(false) {
                let delay = state.ready_frame_delay();
                if delay.is_zero() {
                    state.window.request_redraw();
                } else {
                    state.frame_ready.set(true);
                    frame_redraw_deferred = true;
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + delay));
                }
            }
            let scripted_active = (self.scripted_verified_input_smoke
                && !self.scripted_verified_input_done)
                || (self.scripted_search_submit_smoke && !self.scripted_search_submit_done)
                || (self.scripted_live_search_smoke && !self.scripted_live_search_done)
                || (self.scripted_live_form_smoke && !self.scripted_live_form_done)
                || (self.scripted_live_link_smoke && !self.scripted_live_link_done)
                || (self.scripted_retained_navigation_smoke
                    && !self.scripted_retained_navigation_done);
            if self.embed_parent_hwnd.is_some() && self.started.elapsed() <= EMBEDDED_STARTUP_PUMP {
                event_loop.set_control_flow(ControlFlow::Poll);
                state.window.request_redraw();
            } else if state.input_pump_active() {
                event_loop.set_control_flow(ControlFlow::Poll);
            } else if host_command_processed {
                event_loop.set_control_flow(ControlFlow::Poll);
            } else if !scripted_active && !frame_redraw_deferred {
                if self.embed_parent_hwnd.is_some() {
                    // `request_redraw` does not reliably wake a `Wait` loop for an
                    // embedded child window on Windows, so async navigation/load
                    // frames would otherwise not appear until an OS input event
                    // arrived. Keep a light tick instead: fast while the page is
                    // loading, slow once it is idle.
                    let tick = if state.active_load_complete() {
                        Duration::from_millis(250)
                    } else {
                        Duration::from_millis(16)
                    };
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + tick));
                } else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
            if self.scripted_load_needs_progress() {
                event_loop.set_control_flow(ControlFlow::Poll);
                if state.active_load_complete() {
                    if state.resource_audit && state.resource_audit_json.borrow().is_none() {
                        if state.resource_audit_due() {
                            if let Some(webview) = state.active_webview() {
                                state.capture_resource_audit(webview);
                            }
                        }
                    } else {
                        state.window.request_redraw();
                    }
                }
            }
            if host_command_processed {
                state.window.request_redraw();
            }
            if self.scripted_verified_input_smoke && !self.scripted_verified_input_done {
                self.drive_verified_input_smoke(&state, event_loop);
                state.window.request_redraw();
            }
            if self.scripted_search_submit_smoke && !self.scripted_search_submit_done {
                self.drive_search_submit_smoke(&state, event_loop);
                state.window.request_redraw();
            }
            if self.scripted_live_search_smoke && !self.scripted_live_search_done {
                self.drive_live_search_smoke(&state, event_loop);
            }
            if self.scripted_live_form_smoke && !self.scripted_live_form_done {
                self.drive_live_form_smoke(&state, event_loop);
            }
            if self.scripted_live_link_smoke && !self.scripted_live_link_done {
                self.drive_live_link_smoke(&state, event_loop);
            }
            if self.scripted_retained_navigation_smoke && !self.scripted_retained_navigation_done {
                self.drive_retained_navigation_smoke(&state, event_loop);
                state.window.request_redraw();
            }
        }

        if self.smoke && self.started.elapsed() >= self.timeout {
            let phase = if self.first_present().is_some() {
                "during scripted smoke"
            } else {
                "before first direct present"
            };
            let title = self
                .state
                .as_ref()
                .and_then(|state| state.active_page_title());
            let url = self
                .state
                .as_ref()
                .and_then(|state| state.active_url())
                .map(|url| url.to_string());
            eprintln!(
                "[{}] timed out after {} {} (last title {:?}, last url {:?})",
                self.log_prefix,
                format_duration(self.timeout),
                phase,
                title,
                url
            );
            event_loop.exit();
        }
    }
}

impl DirectServoApp {
    fn scripted_load_needs_progress(&self) -> bool {
        self.scripted_load_smoke && !self.scripted_load_done
    }

    fn keep_scripted_load_pumping(&self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
    }
}

pub fn run(mut options: DirectServoOptions) -> Result<DirectServoOutcome, String> {
    install_rustls_crypto_provider();
    prime_windows_angle_runtime()?;

    let event_loop: EventLoop<DirectServoUserEvent> = EventLoop::with_user_event()
        .build()
        .map_err(|error| error.to_string())?;
    if options.read_host_commands_from_stdin {
        let proxy = event_loop.create_proxy();
        options.host_command_rx = Some(spawn_stdin_host_command_reader(proxy));
    }
    let smoke = options.smoke;
    let verified_input_smoke = options.scripted_verified_input_smoke;
    let search_submit_smoke = options.scripted_search_submit_smoke;
    let live_search_smoke = options.scripted_live_search_smoke;
    let live_form_smoke = options.scripted_live_form_smoke;
    let live_link_smoke = options.scripted_live_link_smoke;
    let embedded = options.embed_parent_hwnd.is_some();
    if embedded
        || verified_input_smoke
        || search_submit_smoke
        || live_search_smoke
        || live_form_smoke
        || live_link_smoke
    {
        event_loop.set_control_flow(ControlFlow::Poll);
    } else {
        event_loop.set_control_flow(ControlFlow::Wait);
    }
    let mut app = DirectServoApp::new(options);
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("direct Servo event loop failed: {error}"))?;

    let outcome = DirectServoOutcome {
        first_present: app.first_present(),
        direct_frame_count: app.direct_frame_count(),
        direct_slow_frame_count: app.direct_slow_frame_count(),
        direct_max_frame: app.direct_max_frame(),
        direct_max_spin: app.direct_max_spin(),
        direct_max_paint: app.direct_max_paint(),
        direct_max_present: app.direct_max_present(),
        first_interaction_frame: app.first_interaction_frame(),
        first_interaction_before_load_complete: app.first_interaction_before_load_complete(),
        verified_input_frame: app.verified_input_frame(),
        verified_input_second_frame: app.verified_input_second_frame(),
        verified_input: app.verified_input(),
        verified_input_before_load_complete: app.verified_input_before_load_complete(),
        search_submit_frame: app.search_submit_frame(),
        search_submit: app.search_submit(),
        search_submit_url: app.search_submit_url(),
        search_submit_title: app.search_submit_title(),
        live_search_frame: app.live_search_frame(),
        live_search: app.live_search(),
        live_search_url: app.live_search_url(),
        live_search_title: app.live_search_title(),
        live_form_frame: app.live_form_frame(),
        live_form: app.live_form(),
        live_form_url: app.live_form_url(),
        live_form_title: app.live_form_title(),
        live_link_frame: app.live_link_frame(),
        live_link: app.live_link(),
        live_link_url: app.live_link_url(),
        live_link_title: app.live_link_title(),
        retained_navigation_frame: app.retained_navigation_frame(),
        retained_navigation_url: app.retained_navigation_url(),
        retained_navigation_title: app.retained_navigation_title(),
        input_frame: app.input_frame(),
        location_frame: app.location_frame(),
        location_url: app.location_url(),
        location_title: app.location_title(),
        history_navigation_frame: app.history_navigation_frame(),
        history_back_frame: app.history_back_frame(),
        history_forward_frame: app.history_forward_frame(),
        history_final_url: app.history_final_url(),
        history_final_title: app.history_final_title(),
        load_complete: app.load_complete(),
        load_url: app.load_url(),
        load_title: app.load_title(),
        reload_frame: app.reload_frame(),
        reload_url: app.reload_url(),
        reload_title: app.reload_title(),
        certificate_fingerprint_sha256: app.certificate_fingerprint_sha256(),
        appliance_certificate_trust_once_requested: app
            .state
            .as_ref()
            .map(|state| state.appliance_certificate_trust_once_pending.get())
            .unwrap_or(false),
        appliance_certificate_remembered: app
            .appliance_certificate_remember_result()
            .as_ref()
            .map(Result::is_ok),
        appliance_certificate_remember_error: app
            .appliance_certificate_remember_result()
            .and_then(Result::err),
        resource_audit_json: app.resource_audit_json(),
        resize_frame: app.resize_frame(),
        tab_frame: app.tab_frame(),
        tab_url: app.tab_url(),
        tab_title: app.tab_title(),
    };
    if smoke && outcome.first_present.is_none() {
        return Err("direct Servo smoke exited without first present".to_string());
    }
    if smoke && app.scripted_text.is_some() && outcome.input_frame.is_none() {
        return Err("direct Servo input smoke exited without input follow-up frame".to_string());
    }
    if smoke && app.scripted_first_interaction_smoke && outcome.first_interaction_frame.is_none() {
        return Err(
            "direct Servo first-interaction smoke exited without follow-up frame".to_string(),
        );
    }
    if smoke
        && app.scripted_verified_input_smoke
        && (outcome.verified_input_frame.is_none() || outcome.verified_input != Some(true))
    {
        return Err("direct Servo verified-input smoke exited without verified text".to_string());
    }
    if smoke
        && app.scripted_search_submit_smoke
        && (outcome.search_submit_frame.is_none() || outcome.search_submit != Some(true))
    {
        return Err("direct Servo search-submit smoke exited without verified query".to_string());
    }
    if smoke
        && app.scripted_live_search_smoke
        && (outcome.live_search_frame.is_none() || outcome.live_search != Some(true))
    {
        return Err("direct Servo live-search smoke exited without verified query".to_string());
    }
    if smoke
        && app.scripted_live_form_smoke
        && (outcome.live_form_frame.is_none() || outcome.live_form != Some(true))
    {
        return Err("direct Servo live-form smoke exited without verified post".to_string());
    }
    if smoke
        && app.scripted_live_link_smoke
        && (outcome.live_link_frame.is_none() || outcome.live_link != Some(true))
    {
        return Err("direct Servo live-link smoke exited without verified navigation".to_string());
    }
    if smoke
        && app.scripted_retained_navigation_smoke
        && outcome.retained_navigation_frame.is_none()
    {
        return Err(
            "direct Servo retained-navigation smoke exited without retained target".to_string(),
        );
    }
    if smoke && app.scripted_location.is_some() && outcome.location_frame.is_none() {
        return Err(
            "direct Servo location smoke exited without location follow-up frame".to_string(),
        );
    }
    if smoke
        && app.scripted_history.is_some()
        && (outcome.history_navigation_frame.is_none()
            || outcome.history_back_frame.is_none()
            || outcome.history_forward_frame.is_none())
    {
        return Err(
            "direct Servo history smoke exited without complete history frames".to_string(),
        );
    }
    if smoke && app.scripted_load_smoke && outcome.load_complete.is_none() {
        return Err("direct Servo load smoke exited without load completion".to_string());
    }
    if smoke && app.scripted_reload_smoke && outcome.reload_frame.is_none() {
        return Err("direct Servo reload smoke exited without reload follow-up frame".to_string());
    }
    if smoke && app.scripted_resize_smoke && outcome.resize_frame.is_none() {
        return Err("direct Servo resize smoke exited without resize follow-up frame".to_string());
    }
    if smoke && app.scripted_tab_smoke && outcome.tab_frame.is_none() {
        return Err("direct Servo tab smoke exited without tab follow-up frame".to_string());
    }
    Ok(outcome)
}

pub fn start_verified_input_fixture_server() -> Result<DirectFixtureServer, String> {
    start_direct_fixture_server(verified_input_fixture_response)
}

pub fn start_search_submit_fixture_server() -> Result<DirectFixtureServer, String> {
    start_direct_fixture_server(search_submit_fixture_response)
}

#[allow(dead_code)]
pub fn start_retained_navigation_fixture_server() -> Result<DirectFixtureServer, String> {
    start_direct_fixture_server(retained_navigation_fixture_response)
}

fn start_direct_fixture_server(
    response_for_path: fn(&str) -> String,
) -> Result<DirectFixtureServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let url =
        Url::parse(&format!("http://127.0.0.1:{port}/")).map_err(|error| error.to_string())?;
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("sextant-verified-input-fixture".to_string())
        .spawn(move || loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0; 1024];
                    let read = stream.read(&mut buffer).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");
                    let response = response_for_path(path);
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(DirectFixtureServer {
        url,
        shutdown_tx,
        handle: Some(handle),
    })
}

fn http_html_response(html: &str) -> String {
    format!(
        "HTTP/1.0 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    )
}

fn verified_input_fixture_response(_path: &str) -> String {
    http_html_response(&verified_input_fixture_html())
}

fn search_submit_fixture_response(path: &str) -> String {
    if path.contains("q=sextant") {
        return http_html_response(
            "<!doctype html><meta charset='utf-8'><title>search:sextant</title>\
<body style='margin:0;background:#10161d;color:white;font:24px sans-serif;display:grid;place-items:center;height:100vh'>Search complete</body>",
        );
    }
    http_html_response(
        "<!doctype html><meta charset='utf-8'><title>SextantSearchSubmitSmoke</title>\
<body style='margin:0;background:#10161d;color:white;font:20px sans-serif;height:100vh'>\
<form method='get' action='/search'><input name='q' autofocus style='position:absolute;left:80px;top:80px;width:420px;padding:18px;font:24px sans-serif' value=''></form></body>",
    )
}

#[allow(dead_code)]
fn retained_navigation_fixture_response(path: &str) -> String {
    if path.contains("/next") {
        return http_html_response(&format!(
            "<!doctype html><meta charset='utf-8'><title>{RETAINED_NAVIGATION_TARGET_TITLE}</title>\
<body style='margin:0;background:#10161d;color:white;font:24px sans-serif;display:grid;place-items:center;height:100vh'>Retained target</body>"
        ));
    }
    http_html_response(&format!(
        "<!doctype html><meta charset='utf-8'><title>{RETAINED_NAVIGATION_SMOKE_TITLE}</title>\
<body style='margin:0;background:#10161d;color:white;font:20px sans-serif;height:100vh'>\
<a href='/next' style='position:absolute;left:80px;top:88px;color:#93c5fd;font:24px sans-serif;padding:24px'>Open retained target</a></body>"
    ))
}

fn verified_input_fixture_html() -> String {
    "<!doctype html><meta charset='utf-8'><title>SextantVerifiedInputSmoke</title>\
<body style='margin:0;background:#10161d;color:white;font:20px sans-serif;height:100vh'>\
<input id='q' style='position:absolute;left:80px;top:80px;width:420px;padding:18px;font:24px sans-serif' value='' onmousedown=\"document.title='clicked'\" onfocus=\"document.title='focused'\" oninput=\"document.title='typed:'+this.value\"></body>"
        .to_string()
}

fn retained_navigation_script() -> String {
    format!(
        r#"
(function() {{
  if (window.__sextantRetainedNavigation) return;
  window.__sextantRetainedNavigation = true;
  const prefix = '{RETAINED_NAVIGATION_TITLE_PREFIX}';

  function encodeUrl(url) {{
    return btoa(unescape(encodeURIComponent(url)));
  }}

  function route(url) {{
    try {{
      const target = new URL(url, document.baseURI);
      if (target.protocol !== 'http:' && target.protocol !== 'https:') return false;
      if (target.href === location.href || (target.origin === location.origin && target.pathname === location.pathname && target.search === location.search && target.hash)) return false;
      document.title = prefix + encodeUrl(target.href);
      return true;
    }} catch (_) {{
      return false;
    }}
  }}

  function sameWindowTarget(target) {{
    if (!target || target === '_self') return true;
    return window.name && target === window.name;
  }}

  window.addEventListener('click', function(event) {{
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const anchor = event.target && event.target.closest ? event.target.closest('a[href]') : null;
    if (!anchor || anchor.download || !sameWindowTarget(anchor.getAttribute('target'))) return;
    if (route(anchor.href)) event.preventDefault();
  }});

  window.addEventListener('submit', function(event) {{
    if (event.defaultPrevented) return;
    const form = event.target;
    if (!form || form.tagName !== 'FORM') return;
    const method = (form.getAttribute('method') || 'get').toLowerCase();
    if (method !== 'get' || !sameWindowTarget(form.getAttribute('target'))) return;
    try {{
      const url = new URL(form.getAttribute('action') || location.href, document.baseURI);
      const params = new URLSearchParams(new FormData(form));
      url.search = params.toString();
      if (route(url.href)) event.preventDefault();
    }} catch (_) {{}}
  }});
}})();
"#
    )
}

fn retained_navigation_title_target(title: Option<&str>) -> Option<Url> {
    let encoded = title?.strip_prefix(RETAINED_NAVIGATION_TITLE_PREFIX)?;
    let bytes = general_purpose::STANDARD.decode(encoded.as_bytes()).ok()?;
    let target = String::from_utf8(bytes).ok()?;
    Url::parse(&target).ok()
}

impl DirectServoState {
    fn create_webview(self: &Rc<Self>, url: Url) -> WebView {
        let user_content_manager = Rc::new(UserContentManager::new(&self.servo));
        user_content_manager.add_script(Rc::new(UserScript::from(retained_navigation_script())));
        WebViewBuilder::new(&self.servo, self.rendering_context.clone())
            .url(url)
            .delegate(self.clone())
            .user_content_manager(user_content_manager)
            .build()
    }

    fn active_webview(&self) -> Option<WebView> {
        self.webviews.borrow().get(self.active_index.get()).cloned()
    }

    fn active_url(&self) -> Option<Url> {
        self.active_webview().and_then(|webview| webview.url())
    }

    fn active_page_title(&self) -> Option<String> {
        self.active_webview()
            .and_then(|webview| webview.page_title())
            .filter(|title| retained_navigation_title_target(Some(title.as_str())).is_none())
    }

    fn is_active_webview(&self, webview: &WebView) -> bool {
        self.active_webview()
            .map(|active| active.id() == webview.id())
            .unwrap_or(false)
    }

    fn is_pending_webview(&self, webview: &WebView) -> bool {
        self.pending_navigation
            .borrow()
            .as_ref()
            .map(|pending| pending.webview.id() == webview.id())
            .unwrap_or(false)
    }

    fn log_active_load_status(&self, webview: &WebView, status: LoadStatus) {
        if status == LoadStatus::Started {
            self.active_navigation_started.set(Some(Instant::now()));
        }
        let elapsed = self
            .active_navigation_started
            .get()
            .map(|started| format_duration(started.elapsed()))
            .unwrap_or_else(|| "n/a".to_string());
        let url = webview
            .url()
            .map(|url| url.to_string())
            .unwrap_or_else(|| "pending-url".to_string());
        println!(
            "[{}] active load status {:?} after {} url {}",
            self.log_prefix,
            status,
            elapsed,
            truncate_text(&url, 120)
        );
    }

    fn log_pending_load_status(&self, webview: &WebView, status: LoadStatus) {
        let pending = self.pending_navigation.borrow();
        let Some(pending) = pending.as_ref() else {
            return;
        };
        if pending.webview.id() != webview.id() {
            return;
        }
        println!(
            "[{}] pending load status {:?} after {} url {}",
            self.log_prefix,
            status,
            format_duration(pending.started.elapsed()),
            truncate_text(pending.target.as_str(), 120)
        );
    }

    fn begin_retained_navigation(&self, url: Url) {
        if let Some(state) = self.self_handle.borrow().upgrade() {
            // In-page link/form navigation: navigate the active webview in place
            // (not a fresh swapped-in webview) so Servo records session history
            // and back/forward work after link clicks, matching address-bar nav.
            state.load(url);
        }
    }

    fn start_retained_navigation(self: &Rc<Self>, url: Url) {
        if self.active_webview().is_none() {
            return;
        }
        let webview = self.create_webview(url.clone());
        webview.resize(self.window.inner_size());
        webview.hide();
        self.pending_navigation
            .replace(Some(DirectPendingNavigation {
                webview,
                target: url,
                started: Instant::now(),
            }));
        self.wake_after_browser_command();
        self.update_window_title();
    }

    fn reset_navigation_side_effects(&self) {
        self.certificate_probe_started.set(false);
        self.certificate_page_decorated.set(false);
        self.certificate_der_base64.borrow_mut().take();
        self.resource_audit_started.set(false);
        self.resource_audit_last_started.set(None);
        self.resource_audit_json.borrow_mut().take();
        self.text_paint_warmup_started.set(false);
    }

    fn activate_pending_navigation(&self, webview: &WebView, reason: &str) -> bool {
        let mut pending_navigation = self.pending_navigation.borrow_mut();
        let Some(pending) = pending_navigation.take() else {
            return false;
        };
        if pending.webview.id() != webview.id() {
            *pending_navigation = Some(pending);
            return false;
        }
        drop(pending_navigation);

        if let Some(active) = self.active_webview() {
            active.hide();
        }
        let activated_webview = pending.webview.clone();
        let mut webviews = self.webviews.borrow_mut();
        let active_index = self
            .active_index
            .get()
            .min(webviews.len().saturating_sub(1));
        if webviews.is_empty() {
            webviews.push(pending.webview.clone());
            self.active_index.set(0);
        } else {
            webviews[active_index] = pending.webview.clone();
            self.active_index.set(active_index);
        }
        drop(webviews);

        self.active_navigation_started.set(Some(pending.started));
        self.reset_navigation_side_effects();
        self.focus_active_webview();
        if activated_webview.load_status() == LoadStatus::Complete {
            if self.active_page_title().as_deref() == Some("Certificate error") {
                self.capture_bad_certificate_bytes(activated_webview.clone());
                self.decorate_bad_certificate_page(activated_webview.clone());
            } else if self.resource_audit_due() {
                self.capture_resource_audit(activated_webview.clone());
            } else {
                self.warm_text_paint(activated_webview);
            }
        }
        self.frame_ready.set(true);
        self.window.request_redraw();
        self.update_window_title();
        println!(
            "[{}] retained navigation swap to {} after {} ({reason})",
            self.log_prefix,
            pending.target,
            format_duration(pending.started.elapsed())
        );
        true
    }

    fn active_load_complete(&self) -> bool {
        self.active_webview()
            .map(|webview| webview.load_status() == LoadStatus::Complete)
            .unwrap_or(false)
    }

    fn resource_audit_due(&self) -> bool {
        if !self.resource_audit || self.resource_audit_started.get() {
            return false;
        }
        self.resource_audit_last_started
            .get()
            .map(|started| started.elapsed() >= Duration::from_secs(5))
            .unwrap_or(true)
    }

    fn active_url_matches(&self, expected: &Url) -> bool {
        self.active_webview()
            .and_then(|webview| webview.url())
            .map(|url| url == *expected)
            .unwrap_or(false)
    }

    fn active_title_matches(&self, expected: &str) -> bool {
        self.active_page_title()
            .map(|title| title == expected)
            .unwrap_or(false)
    }

    fn record_direct_frame_timing(
        &self,
        total: Duration,
        spin: Duration,
        paint: Duration,
        present: Duration,
    ) {
        self.direct_frame_count
            .set(self.direct_frame_count.get().saturating_add(1));
        // Co-record the spin/paint/present breakdown of the single worst-total
        // frame. Tracking the components as independent maxima would mix phases
        // from different frames, so the reported decomposition could not be
        // attributed to one frame and overstated where the heavy frame's time
        // actually went.
        let current = match (
            self.direct_max_frame.get(),
            self.direct_max_spin.get(),
            self.direct_max_paint.get(),
            self.direct_max_present.get(),
        ) {
            (Some(total), Some(spin), Some(paint), Some(present)) => {
                Some((total, spin, paint, present))
            }
            _ => None,
        };
        let (worst_total, worst_spin, worst_paint, worst_present) =
            keep_worst_total_frame(current, (total, spin, paint, present));
        self.direct_max_frame.set(Some(worst_total));
        self.direct_max_spin.set(Some(worst_spin));
        self.direct_max_paint.set(Some(worst_paint));
        self.direct_max_present.set(Some(worst_present));

        if total >= Duration::from_millis(100) {
            let slow_count = self.direct_slow_frame_count.get().saturating_add(1);
            self.direct_slow_frame_count.set(slow_count);
            if slow_count <= 12 || slow_count % 30 == 0 {
                let phase = self.direct_frame_phase();
                println!(
                    "[{}] slow direct frame {} phase={} total={} spin={} paint={} present={}",
                    self.log_prefix,
                    slow_count,
                    phase,
                    format_duration(total),
                    format_duration(spin),
                    format_duration(paint),
                    format_duration(present)
                );
            }
        }
    }

    fn direct_frame_phase(&self) -> &'static str {
        if self.first_present.get().is_none() {
            "first-present"
        } else if self.input_pump_active() {
            "input"
        } else if !self.active_load_complete() {
            "loading"
        } else if self.resource_audit && self.resource_audit_json.borrow().is_none() {
            "post-load-audit"
        } else {
            "idle"
        }
    }

    fn capture_bad_certificate_bytes(&self, webview: WebView) {
        if self.certificate_probe_started.replace(true) {
            return;
        }
        let certificate_der_base64 = self.certificate_der_base64.clone();
        let log_prefix = self.log_prefix;
        webview.evaluate_javascript(
            "document.getElementById('bytes') ? document.getElementById('bytes').textContent : ''",
            move |result| {
                if let Ok(JSValue::String(value)) = result {
                    let trimmed = value.trim();
                    if !trimmed.is_empty() {
                        if let Ok(fingerprint) = certificate_fingerprint_sha256_from_base64(trimmed)
                        {
                            println!("[{log_prefix}] certificate fingerprint sha256 {fingerprint}");
                        }
                        *certificate_der_base64.borrow_mut() = Some(trimmed.to_string());
                    }
                }
            },
        );
    }

    fn certificate_fingerprint_sha256(&self) -> Option<String> {
        self.certificate_der_base64
            .borrow()
            .clone()
            .and_then(|value| certificate_fingerprint_sha256_from_base64(&value).ok())
    }

    fn capture_resource_audit(&self, webview: WebView) {
        if self.resource_audit_started.replace(true) {
            return;
        }
        self.resource_audit_last_started.set(Some(Instant::now()));
        let resource_audit_json = self.resource_audit_json.clone();
        let resource_audit_started = self.resource_audit_started.clone();
        let log_prefix = self.log_prefix;
        webview.evaluate_javascript(
            r#"
(() => {
  try {
  const describeElement = (el) => {
    if (!el) return null;
    const rect = el.getBoundingClientRect ? el.getBoundingClientRect() : {x:0,y:0,width:0,height:0};
    const style = getComputedStyle(el);
    return {
      tag: el.tagName || '',
      id: el.id || '',
      className: typeof el.className === 'string' ? el.className : '',
      role: el.getAttribute ? (el.getAttribute('role') || '') : '',
      ariaLabel: el.getAttribute ? (el.getAttribute('aria-label') || '') : '',
      text: (el.textContent || '').trim().slice(0, 80),
      rect: {x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height)},
      display: style.display,
      visibility: style.visibility,
      opacity: style.opacity,
      pointerEvents: style.pointerEvents
    };
  };
  const describeSvg = (svg) => {
    const rect = svg.getBoundingClientRect();
    const style = getComputedStyle(svg);
    return {
      id: svg.id || '',
      className: typeof svg.className === 'string' ? svg.className : (svg.getAttribute('class') || ''),
      ariaLabel: svg.getAttribute('aria-label') || '',
      role: svg.getAttribute('role') || '',
      rect: {x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height)},
      display: style.display,
      visibility: style.visibility,
      opacity: style.opacity,
      fill: style.fill,
      stroke: style.stroke,
      pathCount: svg.querySelectorAll('path,circle,rect,line,polyline,polygon,use').length
    };
  };
  const viewportPoints = [
    [20, 20],
    [80, 80],
    [Math.round(innerWidth / 2), Math.round(innerHeight / 2)],
    [Math.max(0, innerWidth - 40), 80],
    [80, Math.max(0, innerHeight - 80)]
  ];
  const svgs = Array.from(document.querySelectorAll('svg'));
  const elements = Array.from(document.getElementsByTagName('*'));
  const zeroSizeSvgs = svgs.filter(svg => {
    const rect = svg.getBoundingClientRect();
    return rect.width === 0 || rect.height === 0;
  });
  const stylesheets = Array.from(document.styleSheets || []).map(sheet => {
    let ruleCount = null;
    let accessible = true;
    try { ruleCount = sheet.cssRules ? sheet.cssRules.length : null; } catch (e) { accessible = false; }
    return {href: sheet.href || '', accessible, ruleCount};
  });
  const scripts = Array.from(document.scripts || []);
  const resources = Array.from(performance.getEntriesByType ? performance.getEntriesByType('resource') : []);
  const resourceSummary = resources.reduce((summary, entry) => {
    const type = entry.initiatorType || 'other';
    summary.count++;
    summary.transferSize += entry.transferSize || 0;
    summary.encodedBodySize += entry.encodedBodySize || 0;
    summary.decodedBodySize += entry.decodedBodySize || 0;
    summary.duration += entry.duration || 0;
    summary.byInitiator[type] = (summary.byInitiator[type] || 0) + 1;
    return summary;
  }, {count: 0, transferSize: 0, encodedBodySize: 0, decodedBodySize: 0, duration: 0, byInitiator: {}});
  return JSON.stringify({
    title: document.title || '',
    url: location.href,
    readyState: document.readyState,
    viewport: {width: innerWidth, height: innerHeight, devicePixelRatio: devicePixelRatio},
    documentSize: {scrollWidth: document.documentElement.scrollWidth, scrollHeight: document.documentElement.scrollHeight, bodyWidth: document.body ? document.body.scrollWidth : null, bodyHeight: document.body ? document.body.scrollHeight : null},
    elementCount: elements.length,
    interactiveElementCount: document.querySelectorAll('a,button,input,select,textarea,[role=button],[tabindex]').length,
    bodyStyle: document.body ? {
      fontFamily: getComputedStyle(document.body).fontFamily,
      color: getComputedStyle(document.body).color,
      backgroundColor: getComputedStyle(document.body).backgroundColor,
      display: getComputedStyle(document.body).display
    } : null,
    fonts: document.fonts ? {status: document.fonts.status, count: Array.from(document.fonts).length, families: Array.from(document.fonts).map(font => font.family).slice(0, 20)} : null,
    stylesheetCount: stylesheets.length,
    stylesheetRuleCount: stylesheets.reduce((count, sheet) => count + (sheet.ruleCount || 0), 0),
    inaccessibleStylesheetCount: stylesheets.filter(sheet => !sheet.accessible).length,
    stylesheets: stylesheets.slice(0, 20),
    scriptCount: scripts.length,
    moduleScriptCount: scripts.filter(script => (script.type || '').toLowerCase() === 'module').length,
    asyncScriptCount: scripts.filter(script => script.async).length,
    deferScriptCount: scripts.filter(script => script.defer).length,
    imageCount: document.images ? document.images.length : null,
    brokenImages: Array.from(document.images || []).filter(img => img.complete && img.naturalWidth === 0).map(img => img.currentSrc || img.src).slice(0, 25),
    inlineSvgCount: svgs.length,
    zeroSizeSvgCount: zeroSizeSvgs.length,
    svgSamples: svgs.slice(0, 30).map(describeSvg),
    zeroSizeSvgSamples: zeroSizeSvgs.slice(0, 20).map(describeSvg),
    canvasCount: document.querySelectorAll('canvas').length,
    interactiveSamples: Array.from(document.querySelectorAll('a,button,input,select,textarea,[role=button],[tabindex]')).slice(0, 40).map(describeElement),
    hitTests: viewportPoints.map(([x, y]) => ({x, y, element: describeElement(document.elementFromPoint(x, y))})),
    resourceSummary: {
      count: resourceSummary.count,
      transferSize: Math.round(resourceSummary.transferSize),
      encodedBodySize: Math.round(resourceSummary.encodedBodySize),
      decodedBodySize: Math.round(resourceSummary.decodedBodySize),
      duration: Math.round(resourceSummary.duration),
      byInitiator: resourceSummary.byInitiator
    },
    resources: resources.map(entry => ({
      name: entry.name,
      initiatorType: entry.initiatorType || '',
      duration: Math.round(entry.duration || 0),
      transferSize: entry.transferSize || 0,
      encodedBodySize: entry.encodedBodySize || 0,
      decodedBodySize: entry.decodedBodySize || 0
    })).slice(-100)
  });
  } catch (error) {
    return JSON.stringify({
      auditError: String(error),
      auditStack: error && error.stack ? String(error.stack).slice(0, 500) : ''
    });
  }
})()
"#,
            move |result| match result {
                Ok(JSValue::String(value)) => {
                    println!("[{log_prefix}] resource audit {value}");
                    *resource_audit_json.borrow_mut() = Some(value);
                    resource_audit_started.set(false);
                }
                Ok(other) => {
                    println!("[{log_prefix}] resource audit unexpected result {other:?}");
                    *resource_audit_json.borrow_mut() = Some("{}".to_string());
                    resource_audit_started.set(false);
                }
                Err(error) => {
                    println!("[{log_prefix}] resource audit failed {error:?}");
                    *resource_audit_json.borrow_mut() = Some("{}".to_string());
                    resource_audit_started.set(false);
                }
            },
        );
    }

    fn warm_text_paint(&self, webview: WebView) {
        if self.text_paint_warmup_started.replace(true) {
            return;
        }
        let log_prefix = self.log_prefix;
        webview.evaluate_javascript(
            r#"
(() => {
  if (!document.documentElement || document.getElementById('__sextant_text_paint_warmup')) {
    return 'skipped';
  }
  const input = document.createElement('input');
  input.id = '__sextant_text_paint_warmup';
  input.value = 'Sextant warmup text 123';
  input.setAttribute('aria-hidden', 'true');
  input.tabIndex = -1;
  input.style.cssText = [
    'position:fixed',
    'left:0',
    'top:0',
    'width:2px',
    'height:2px',
    'opacity:0.01',
    'pointer-events:none',
    'z-index:-2147483648',
    'border:0',
    'padding:0',
    'font:16px sans-serif',
    'background:transparent',
    'color:transparent'
  ].join(';');
  document.documentElement.appendChild(input);
  requestAnimationFrame(() => {
    input.value = 'Sextant warmup text 456';
    setTimeout(() => input.remove(), 160);
  });
  return 'started';
})()
"#,
            move |result| {
                if let Err(error) = result {
                    println!("[{log_prefix}] text paint warmup failed {error:?}");
                }
            },
        );
    }

    fn decorate_bad_certificate_page(&self, webview: WebView) {
        if self.certificate_page_decorated.replace(true) {
            return;
        }
        let persist_allowed = self
            .appliance_certificate_actions
            .as_ref()
            .map(|actions| actions.persist_allowed)
            .unwrap_or(false);
        let script = format!(
            r#"
(function() {{
  if (document.getElementById('sextant-cert-warning')) return;
  const allow = document.getElementById('allow');
  const leave = document.getElementById('leave');
  const bytes = document.getElementById('bytes');
  if (!allow || !leave || !bytes) return;
  document.body.style.margin = '0';
  document.body.style.fontFamily = 'system-ui, -apple-system, Segoe UI, sans-serif';
  document.body.style.background = '#111827';
  document.body.style.color = '#f9fafb';
  const root = document.createElement('main');
  root.id = 'sextant-cert-warning';
  root.style.maxWidth = '780px';
  root.style.margin = '0 auto';
  root.style.padding = '56px 24px';
  root.innerHTML = `
    <h1 style="font-size:28px;line-height:1.2;margin:0 0 16px">Certificate warning</h1>
    <p style="font-size:16px;line-height:1.55;color:#d1d5db;margin:0 0 20px">Sextant blocked this local appliance because its TLS certificate is not trusted yet.</p>
    <p style="font-size:13px;line-height:1.5;color:#9ca3af;margin:0 0 24px">Use one-time trust only while testing. Persisted trust is pinned to this appliance origin and certificate fingerprint.</p>
    <div id="sextant-cert-actions" style="display:flex;gap:12px;flex-wrap:wrap;margin-bottom:20px"></div>
    <details style="color:#d1d5db"><summary>Certificate bytes</summary></details>
  `;
  document.body.prepend(root);
  const actions = document.getElementById('sextant-cert-actions');
  leave.textContent = 'Go Back';
  allow.textContent = 'Trust Once';
  for (const button of [leave, allow]) {{
    button.style.border = '1px solid #4b5563';
    button.style.background = button === allow ? '#2563eb' : '#1f2937';
    button.style.color = '#f9fafb';
    button.style.borderRadius = '6px';
    button.style.padding = '10px 14px';
    button.style.font = '600 14px system-ui, -apple-system, Segoe UI, sans-serif';
    actions.appendChild(button);
  }}
  const remember = document.createElement('button');
  remember.textContent = 'Trust This Appliance';
  remember.disabled = !{persist_allowed};
  remember.style.border = '1px solid #4b5563';
  remember.style.background = {persist_allowed} ? '#047857' : '#374151';
  remember.style.color = '#f9fafb';
  remember.style.borderRadius = '6px';
  remember.style.padding = '10px 14px';
  remember.style.font = '600 14px system-ui, -apple-system, Segoe UI, sans-serif';
  allow.onclick = function() {{
    document.title = '{APPLIANCE_CERT_TRUST_ONCE_TITLE}';
  }};
  remember.onclick = function() {{
    document.title = '{APPLIANCE_CERT_REMEMBER_TITLE}';
  }};
  actions.appendChild(remember);
  if (!{persist_allowed}) {{
    const note = document.createElement('p');
    note.textContent = 'Persistent certificate exceptions are disabled in Incognito.';
    note.style.color = '#fbbf24';
    note.style.margin = '0 0 20px';
    root.insertBefore(note, root.querySelector('details'));
  }}
  root.querySelector('details').appendChild(bytes);
}})();
"#
        );
        webview.evaluate_javascript(&script, |_| {});
    }

    fn complete_pending_appliance_certificate_decision(&self) -> bool {
        if self.appliance_certificate_trust_once_pending.get() {
            return self.certificate_fingerprint_sha256().is_some();
        }
        if !self.appliance_certificate_remember_pending.get() {
            return false;
        }
        if self
            .appliance_certificate_remember_result
            .borrow()
            .is_some()
        {
            return true;
        }
        let Some(actions) = self.appliance_certificate_actions.as_ref() else {
            self.appliance_certificate_remember_pending.set(false);
            *self.appliance_certificate_remember_result.borrow_mut() = Some(Err(
                "no local appliance certificate trust action is available".to_string(),
            ));
            return true;
        };
        if !actions.persist_allowed {
            self.appliance_certificate_remember_pending.set(false);
            *self.appliance_certificate_remember_result.borrow_mut() = Some(Err(
                "persistent certificate trust is disabled in Incognito".to_string(),
            ));
            return true;
        }
        let Some(fingerprint) = self.certificate_fingerprint_sha256() else {
            if let Some(webview) = self.active_webview() {
                self.capture_bad_certificate_bytes(webview);
            }
            self.window.request_redraw();
            return false;
        };
        self.appliance_certificate_remember_pending.set(false);
        let result = (actions.remember)(fingerprint);
        *self.appliance_certificate_remember_result.borrow_mut() = Some(result);
        true
    }

    fn request_appliance_certificate_trust_once(&self) {
        self.appliance_certificate_trust_once_pending.set(true);
        if let Some(webview) = self.active_webview() {
            self.capture_bad_certificate_bytes(webview);
        }
        self.wake_after_browser_command();
    }

    fn request_appliance_certificate_remember(&self) {
        self.appliance_certificate_remember_pending.set(true);
        if let Some(webview) = self.active_webview() {
            self.capture_bad_certificate_bytes(webview);
        }
        self.wake_after_browser_command();
    }

    fn leave_appliance_certificate_warning(self: &Rc<Self>) {
        if self.can_go_back() {
            self.go_back();
        } else {
            self.load(Url::parse("about:blank").expect("about:blank should parse"));
        }
    }

    fn active_live_search_verified(&self) -> bool {
        let query = LIVE_SEARCH_TEXT.to_ascii_lowercase();
        let title_matches = self
            .active_page_title()
            .map(|title| title.to_ascii_lowercase().contains(&query))
            .unwrap_or(false);
        let url_matches = self
            .active_url()
            .map(|url| {
                let value = url.as_str().to_ascii_lowercase();
                value.contains("/wiki/sextant")
                    || value.contains("search=sextant")
                    || value.contains("q=sextant")
            })
            .unwrap_or(false);
        title_matches || url_matches
    }

    fn active_live_form_verified(&self) -> bool {
        self.active_url()
            .map(|url| url.path() == "/post")
            .unwrap_or(false)
    }

    fn active_live_link_verified(&self) -> bool {
        let Some(expected_host) = self.live_link_expected_host.borrow().clone() else {
            return false;
        };
        let Some(active_host) = self
            .active_url()
            .and_then(|url| url.host_str().map(|host| host.to_string()))
        else {
            return false;
        };
        // Accept the anchor host exactly, or a redirect within the same
        // registrable domain (e.g. `iana.org` -> `www.iana.org`), so a normal
        // apex/`www` redirect after the click still counts as verified.
        active_host == expected_host
            || active_host.ends_with(&format!(".{expected_host}"))
            || expected_host.ends_with(&format!(".{active_host}"))
    }

    fn active_tab_smoke_ready(&self) -> bool {
        self.active_url_matches(&direct_tab_smoke_url())
            && self.active_title_matches(DIRECT_TAB_SMOKE_TITLE)
    }

    fn focus_active_webview(&self) {
        if let Some(webview) = self.active_webview() {
            webview.resize(self.window.inner_size());
            webview.show();
            webview.focus();
        }
    }

    fn update_window_title(&self) {
        if self.location_active.get() {
            let buffer = self.location_buffer.borrow();
            self.window
                .set_title(&format!("Address: {buffer} - Sextant Direct [Enter]"));
            return;
        }

        let active = self.active_webview();
        let pending_target = self
            .pending_navigation
            .borrow()
            .as_ref()
            .map(|pending| pending.target.to_string());
        let label = active
            .as_ref()
            .and_then(WebView::page_title)
            .filter(|title| !title.trim().is_empty())
            .or_else(|| {
                active
                    .as_ref()
                    .and_then(WebView::url)
                    .map(|url| url.to_string())
            })
            .unwrap_or_else(|| "about:blank".to_string());
        let label = pending_target
            .map(|target| format!("Loading {target} - {label}"))
            .unwrap_or(label);
        let back = if self.can_go_back() { "<" } else { "-" };
        let forward = if self.can_go_forward() { ">" } else { "-" };
        let index = self.active_index.get() + 1;
        let total = self.webviews.borrow().len().max(1);
        self.window.set_title(&format!(
            "{label} - Sextant Direct [{back}{forward}] Tab {index}/{total}"
        ));
        let url = active
            .as_ref()
            .and_then(WebView::url)
            .map(|url| url.to_string())
            .unwrap_or_else(|| "about:blank".to_string());
        let status = format!(
            "tab {index}/{total} back {} forward {} title {} url {}",
            self.can_go_back(),
            self.can_go_forward(),
            sanitize_direct_chrome_label(&label),
            url
        );
        // Per-tab lines for the chrome tab strip: `chrome tab <i> <active> <url>`.
        let tab_lines: Vec<String> = {
            let webviews = self.webviews.borrow();
            let active_idx = self.active_index.get();
            webviews
                .iter()
                .enumerate()
                .map(|(i, webview)| {
                    let tab_url = webview
                        .url()
                        .map(|url| url.to_string())
                        .unwrap_or_else(|| "about:blank".to_string());
                    format!("chrome tab {i} {} {tab_url}", usize::from(i == active_idx))
                })
                .collect()
        };
        let combined = format!("{status}\n{}", tab_lines.join("\n"));
        if *self.last_chrome_status.borrow() != combined {
            println!("[{}] chrome status {status}", self.log_prefix);
            for line in &tab_lines {
                println!("[{}] {line}", self.log_prefix);
            }
            *self.last_chrome_status.borrow_mut() = combined;
        }
    }

    fn notify_input(&self, event: InputEvent) {
        self.notify_input_with_wake(event, true);
    }

    fn notify_interactive_input(&self, event: InputEvent) {
        self.notify_inputs_with_render(vec![event]);
    }

    fn notify_interactive_inputs(&self, events: Vec<InputEvent>) {
        self.notify_inputs_with_render(events);
    }

    fn notify_inputs_with_render(&self, events: Vec<InputEvent>) {
        if events.is_empty() {
            return;
        }
        if let Some(webview) = self.active_webview() {
            let event_count = events.len() as u64;
            for event in events {
                webview.notify_input_event(event);
            }
            self.last_input_at.set(Some(Instant::now()));
            self.input_events.set(self.input_events.get() + event_count);
            self.frame_ready.set(true);
            self.window.request_redraw();
        }
    }

    fn notify_passive_input(&self, event: InputEvent) {
        self.notify_input_with_wake(event, false);
    }

    fn notify_input_with_wake(&self, event: InputEvent, wake_after_input: bool) {
        if let Some(webview) = self.active_webview() {
            webview.notify_input_event(event);
            self.last_input_at.set(Some(Instant::now()));
            self.input_events.set(self.input_events.get() + 1);
            if wake_after_input {
                self.servo.spin_event_loop();
                self.frame_ready.set(true);
                self.window.request_redraw();
            }
        }
    }

    fn wake_after_browser_command(&self) {
        self.servo.spin_event_loop();
        self.frame_ready.set(true);
        self.window.request_redraw();
    }

    fn input_pump_active(&self) -> bool {
        self.last_input_at
            .get()
            .map(|last| last.elapsed() <= INPUT_PUMP_AFTER_EVENT)
            .unwrap_or(false)
    }

    fn ready_frame_delay(&self) -> Duration {
        if self.input_pump_active() {
            return Duration::ZERO;
        }
        let min_interval = if self.first_present.get().is_some() && !self.active_load_complete() {
            DIRECT_LOADING_READY_FRAME_MIN_INTERVAL
        } else {
            DIRECT_READY_FRAME_MIN_INTERVAL
        };
        self.last_direct_present_at
            .get()
            .map(|last| min_interval.saturating_sub(last.elapsed()))
            .unwrap_or(Duration::ZERO)
    }

    fn handle_mouse_move(&self, x: f32, y: f32) {
        self.cursor_position.set((x.max(0.0), y.max(0.0)));
        let now = Instant::now();
        let should_send = self.mouse_button_down.get()
            || self
                .last_mouse_move_sent
                .get()
                .map(|last| last.elapsed() >= Duration::from_millis(16))
                .unwrap_or(true);
        if !should_send {
            return;
        }
        self.last_mouse_move_sent.set(Some(now));
        self.notify_passive_input(InputEvent::MouseMove(MouseMoveEvent::new(
            DevicePoint::new(x.max(0.0), y.max(0.0)).into(),
        )));
    }

    fn handle_mouse_button(&self, button: WinitMouseButton, state: ElementState) {
        let Some(button) = servo_mouse_button(button) else {
            return;
        };
        let action = match state {
            ElementState::Pressed => {
                self.mouse_button_down.set(true);
                MouseButtonAction::Down
            }
            ElementState::Released => {
                self.mouse_button_down.set(false);
                MouseButtonAction::Up
            }
        };
        let (x, y) = self.cursor_position.get();
        self.notify_input(InputEvent::MouseButton(MouseButtonEvent::new(
            action,
            button,
            DevicePoint::new(x, y).into(),
        )));
    }

    fn click_at(&self, x: f32, y: f32) {
        self.handle_mouse_move(x, y);
        self.handle_mouse_button(WinitMouseButton::Left, ElementState::Pressed);
        self.handle_mouse_button(WinitMouseButton::Left, ElementState::Released);
    }

    fn click_live_search_box(self: &Rc<Self>, log_prefix: &'static str) -> bool {
        let Some(webview) = self.active_webview() else {
            return false;
        };
        let state = Rc::clone(self);
        webview.evaluate_javascript(
            r#"
(() => {
  const selectors = [
    'textarea[name="q"]',
    'input[name="q"]',
    'input[type="search"]',
    'textarea[aria-label*="Search" i]',
    'input[aria-label*="Search" i]',
    '[role="searchbox"]'
  ];
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width >= 40 && rect.height >= 12 && style.visibility !== 'hidden' && style.display !== 'none' && style.pointerEvents !== 'none';
  };
  const targetUrl = (el) => {
    try {
      const form = el.form || el.closest('form');
      const method = ((form && form.getAttribute('method')) || 'get').toLowerCase();
      if (method && method !== 'get') return '';
      const action = (form && form.getAttribute('action')) || location.href;
      const url = new URL(action, location.href);
      const controls = form ? Array.from(form.elements || []) : [el];
      for (const control of controls) {
        if (!control || control.disabled) continue;
        const name = control.getAttribute && control.getAttribute('name');
        if (!name) continue;
        const type = ((control.getAttribute('type') || control.type || '') + '').toLowerCase();
        if (control === el) {
          url.searchParams.set(name, 'Sextant');
        } else if (type === 'hidden' || type === 'submit') {
          const value = control.getAttribute('value') || control.value || '';
          if (value) url.searchParams.set(name, value);
        }
      }
      if (el.getAttribute('name') && !url.searchParams.has(el.getAttribute('name'))) {
        url.searchParams.set(el.getAttribute('name'), 'Sextant');
      }
      return url.href;
    } catch (_) {
      return '';
    }
  };
  for (const selector of selectors) {
    for (const el of Array.from(document.querySelectorAll(selector))) {
      if (!visible(el)) continue;
      try { el.scrollIntoView({block: 'center', inline: 'center'}); } catch (_) {}
      try { el.focus({preventScroll: true}); } catch (_) { try { el.focus(); } catch (_) {} }
      const rect = el.getBoundingClientRect();
      return JSON.stringify({
        selector,
        tag: el.tagName || '',
        name: el.getAttribute('name') || '',
        type: el.getAttribute('type') || '',
        ariaLabel: el.getAttribute('aria-label') || '',
        targetUrl: targetUrl(el),
        x: Math.round(rect.left + rect.width / 2),
        y: Math.round(rect.top + rect.height / 2),
        w: Math.round(rect.width),
        h: Math.round(rect.height)
      });
    }
  }
  return '';
})()
"#,
            move |result| match result {
                Ok(JSValue::String(value)) => {
                    let value = value.trim();
                    if value.is_empty() {
                        println!("[{log_prefix}] live search dom search box not found");
                        return;
                    }
                    let Ok(candidate) = serde_json::from_str::<Value>(value) else {
                        println!("[{log_prefix}] live search dom search box parse failed {value}");
                        return;
                    };
                    let x = candidate
                        .get("x")
                        .and_then(Value::as_f64)
                        .unwrap_or(LIVE_SEARCH_FALLBACK_CLICK_X as f64)
                        as f32;
                    let y = candidate
                        .get("y")
                        .and_then(Value::as_f64)
                        .unwrap_or(LIVE_SEARCH_FALLBACK_CLICK_Y as f64)
                        as f32;
                    println!(
                        "[{log_prefix}] live search dom search box target x={} y={} selector {} size {}x{}",
                        x.round() as i32,
                        y.round() as i32,
                        candidate
                            .get("selector")
                            .and_then(Value::as_str)
                            .unwrap_or("?"),
                        candidate.get("w").and_then(Value::as_i64).unwrap_or(0),
                        candidate.get("h").and_then(Value::as_i64).unwrap_or(0)
                    );
                    if let Some(target) = candidate.get("targetUrl").and_then(Value::as_str) {
                        if !target.trim().is_empty() {
                            match Url::parse(target.trim()) {
                                Ok(url) => {
                                    println!(
                                        "[{log_prefix}] live search dom form target url {}",
                                        url
                                    );
                                    *state.live_search_dom_fallback_url.borrow_mut() = Some(url);
                                }
                                Err(error) => {
                                    println!(
                                        "[{log_prefix}] live search dom form target url rejected {target}: {error}"
                                    );
                                }
                            }
                        }
                    }
                    state.click_at(x, y);
                    state.send_text(LIVE_SEARCH_TEXT.to_string());
                    state.send_named_key(ServoNamedKey::Enter);
                    state.window.request_redraw();
                }
                Ok(other) => {
                    println!("[{log_prefix}] live search dom search box unexpected result {other:?}");
                }
                Err(error) => {
                    println!("[{log_prefix}] live search dom search box failed {error:?}");
                }
            },
        );
        true
    }

    fn click_live_form_field(self: &Rc<Self>, log_prefix: &'static str) -> bool {
        let Some(webview) = self.active_webview() else {
            return false;
        };
        let state = Rc::clone(self);
        webview.evaluate_javascript(
            r#"
(() => {
  const fieldSelectors = [
    'input[name="custname"]',
    'input[type="text"]',
    'input:not([type])',
    'textarea'
  ];
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width >= 30 && rect.height >= 10 && style.visibility !== 'hidden' && style.display !== 'none' && style.pointerEvents !== 'none';
  };
  const center = (el) => {
    const rect = el.getBoundingClientRect();
    return {
      x: Math.round(rect.left + rect.width / 2),
      y: Math.round(rect.top + rect.height / 2),
      w: Math.round(rect.width),
      h: Math.round(rect.height)
    };
  };
  const submitCandidate = (field) => {
    const form = field.form || field.closest('form');
    const selectors = [
      'button[type="submit"]',
      'button:not([type])',
      'input[type="submit"]'
    ];
    const root = form || document;
    for (const selector of selectors) {
      for (const el of Array.from(root.querySelectorAll(selector))) {
        if (visible(el)) return { el, selector };
      }
    }
    if (form && visible(form)) return { el: form, selector: 'form' };
    return null;
  };
  for (const selector of fieldSelectors) {
    for (const field of Array.from(document.querySelectorAll(selector))) {
      if (!visible(field) || field.disabled || field.readOnly) continue;
      try { field.scrollIntoView({block: 'center', inline: 'center'}); } catch (_) {}
      try { field.focus({preventScroll: true}); } catch (_) { try { field.focus(); } catch (_) {} }
      const input = center(field);
      const submit = submitCandidate(field);
      const result = {
        selector,
        tag: field.tagName || '',
        name: field.getAttribute('name') || '',
        type: field.getAttribute('type') || '',
        x: input.x,
        y: input.y,
        w: input.w,
        h: input.h
      };
      if (submit) {
        const submitCenter = center(submit.el);
        result.submitSelector = submit.selector;
        result.submitX = submitCenter.x;
        result.submitY = submitCenter.y;
        result.submitW = submitCenter.w;
        result.submitH = submitCenter.h;
      }
      return JSON.stringify(result);
    }
  }
  return '';
})()
"#,
            move |result| match result {
                Ok(JSValue::String(value)) => {
                    let value = value.trim();
                    if value.is_empty() {
                        println!("[{log_prefix}] live form field not found");
                        return;
                    }
                    let Ok(candidate) = serde_json::from_str::<Value>(value) else {
                        println!("[{log_prefix}] live form field parse failed {value}");
                        return;
                    };
                    let x = candidate.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    let y = candidate.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    let submit_x = candidate.get("submitX").and_then(Value::as_f64);
                    let submit_y = candidate.get("submitY").and_then(Value::as_f64);
                    if let (Some(submit_x), Some(submit_y)) = (submit_x, submit_y) {
                        *state.live_form_submit_target.borrow_mut() =
                            Some((submit_x as f32, submit_y as f32));
                    }
                    println!(
                        "[{log_prefix}] live form target x={} y={} selector {} size {}x{} submit x={} y={} selector {}",
                        x.round() as i32,
                        y.round() as i32,
                        candidate
                            .get("selector")
                            .and_then(Value::as_str)
                            .unwrap_or("?"),
                        candidate.get("w").and_then(Value::as_i64).unwrap_or(0),
                        candidate.get("h").and_then(Value::as_i64).unwrap_or(0),
                        candidate
                            .get("submitX")
                            .and_then(Value::as_f64)
                            .map(|value| value.round() as i32)
                            .unwrap_or(0),
                        candidate
                            .get("submitY")
                            .and_then(Value::as_f64)
                            .map(|value| value.round() as i32)
                            .unwrap_or(0),
                        candidate
                            .get("submitSelector")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                    );
                    if x > 0.0 && y > 0.0 {
                        state.click_at(x, y);
                    }
                    state.send_text(LIVE_FORM_TEXT.to_string());
                    state.window.request_redraw();
                }
                Ok(other) => {
                    println!("[{log_prefix}] live form field unexpected result {other:?}");
                }
                Err(error) => {
                    println!("[{log_prefix}] live form field failed {error:?}");
                }
            },
        );
        true
    }

    fn click_live_link(self: &Rc<Self>, log_prefix: &'static str) -> bool {
        let Some(webview) = self.active_webview() else {
            return false;
        };
        let state = Rc::clone(self);
        webview.evaluate_javascript(
            r#"
(() => {
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width >= 8 && rect.height >= 6 && style.visibility !== 'hidden' && style.display !== 'none' && style.pointerEvents !== 'none';
  };
  const stripFragment = (value) => value.split('#')[0];
  const here = stripFragment(location.href);
  for (const anchor of Array.from(document.querySelectorAll('a[href]'))) {
    if (!visible(anchor)) continue;
    let url;
    try { url = new URL(anchor.href, location.href); } catch (_) { continue; }
    if (url.protocol !== 'http:' && url.protocol !== 'https:') continue;
    if (stripFragment(url.href) === here) continue;
    try { anchor.scrollIntoView({block: 'center', inline: 'center'}); } catch (_) {}
    const rect = anchor.getBoundingClientRect();
    return JSON.stringify({
      x: Math.round(rect.left + rect.width / 2),
      y: Math.round(rect.top + rect.height / 2),
      w: Math.round(rect.width),
      h: Math.round(rect.height),
      host: url.host,
      href: url.href
    });
  }
  return '';
})()
"#,
            move |result| match result {
                Ok(JSValue::String(value)) => {
                    let value = value.trim();
                    if value.is_empty() {
                        println!("[{log_prefix}] live link anchor not found");
                        return;
                    }
                    let Ok(candidate) = serde_json::from_str::<Value>(value) else {
                        println!("[{log_prefix}] live link anchor parse failed {value}");
                        return;
                    };
                    let x = candidate.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    let y = candidate.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    if let Some(host) = candidate.get("host").and_then(Value::as_str) {
                        *state.live_link_expected_host.borrow_mut() = Some(host.to_string());
                    }
                    *state.live_link_target.borrow_mut() = Some((x, y));
                    println!(
                        "[{log_prefix}] live link target x={} y={} size {}x{} host {} href {}",
                        x.round() as i32,
                        y.round() as i32,
                        candidate.get("w").and_then(Value::as_i64).unwrap_or(0),
                        candidate.get("h").and_then(Value::as_i64).unwrap_or(0),
                        candidate.get("host").and_then(Value::as_str).unwrap_or("?"),
                        candidate.get("href").and_then(Value::as_str).unwrap_or("?")
                    );
                    if x > 0.0 && y > 0.0 {
                        state.click_at(x, y);
                    }
                    state.window.request_redraw();
                }
                Ok(other) => {
                    println!("[{log_prefix}] live link anchor unexpected result {other:?}");
                }
                Err(error) => {
                    println!("[{log_prefix}] live link anchor failed {error:?}");
                }
            },
        );
        true
    }

    fn handle_mouse_wheel(&self, delta: MouseScrollDelta) {
        let (delta_x, delta_y) = match delta {
            MouseScrollDelta::LineDelta(delta_x, delta_y) => (
                (delta_x * LINE_SCROLL_PIXELS) as f64,
                (delta_y * LINE_SCROLL_PIXELS) as f64,
            ),
            MouseScrollDelta::PixelDelta(delta) => (delta.x, delta.y),
        };
        let (x, y) = self.cursor_position.get();
        self.notify_input(InputEvent::Wheel(WheelEvent::new(
            WheelDelta {
                x: delta_x,
                y: delta_y,
                z: 0.0,
                mode: WheelMode::DeltaPixel,
            },
            DevicePoint::new(x, y).into(),
        )));
    }

    fn send_named_key(&self, key: ServoNamedKey) {
        self.notify_interactive_inputs(vec![
            InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
                KeyState::Down,
                Key::Named(key),
            )),
            InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
                KeyState::Up,
                Key::Named(key),
            )),
        ]);
    }

    fn handle_keyboard(self: &Rc<Self>, event: KeyEvent) {
        if self.location_active.get() {
            self.handle_location_key(event);
            return;
        }

        if event.state == ElementState::Pressed && self.handle_browser_shortcut(&event.logical_key)
        {
            return;
        }

        if event.state == ElementState::Pressed {
            if let Some(text) = text_from_key(&event.logical_key) {
                self.send_text(text);
                return;
            }
        }

        if let Some(key) = servo_key_from_winit(&event.logical_key) {
            let state = match event.state {
                ElementState::Pressed => KeyState::Down,
                ElementState::Released => KeyState::Up,
            };
            self.notify_interactive_input(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
                state, key,
            )));
        }
    }

    fn handle_browser_shortcut(self: &Rc<Self>, key: &WinitKey) -> bool {
        let modifiers = self.modifiers.get();
        match key {
            WinitKey::Character(value)
                if modifiers.control_key() && value.eq_ignore_ascii_case("l") =>
            {
                self.enter_location_mode();
                true
            }
            WinitKey::Character(value)
                if modifiers.control_key() && value.eq_ignore_ascii_case("t") =>
            {
                self.new_tab();
                true
            }
            WinitKey::Character(value)
                if modifiers.control_key() && value.eq_ignore_ascii_case("w") =>
            {
                self.close_tab();
                true
            }
            WinitKey::Character(value)
                if modifiers.control_key() && value.eq_ignore_ascii_case("r") =>
            {
                self.reload();
                true
            }
            WinitKey::Named(WinitNamedKey::F5) => {
                self.reload();
                true
            }
            WinitKey::Named(WinitNamedKey::ArrowLeft) if modifiers.alt_key() => {
                self.go_back();
                true
            }
            WinitKey::Named(WinitNamedKey::ArrowRight) if modifiers.alt_key() => {
                self.go_forward();
                true
            }
            WinitKey::Named(WinitNamedKey::Tab) if modifiers.control_key() => {
                self.next_tab();
                true
            }
            WinitKey::Named(WinitNamedKey::PageDown) if modifiers.control_key() => {
                self.next_tab();
                true
            }
            WinitKey::Named(WinitNamedKey::PageUp) if modifiers.control_key() => {
                self.previous_tab();
                true
            }
            _ => false,
        }
    }

    fn enter_location_mode(&self) {
        let text = self
            .active_webview()
            .and_then(|webview| webview.url())
            .map(|url| url.to_string())
            .unwrap_or_default();
        self.location_buffer.replace(text);
        self.location_active.set(true);
        self.location_replace_on_text.set(true);
        self.update_window_title();
    }

    fn exit_location_mode(&self) {
        self.location_active.set(false);
        self.location_replace_on_text.set(false);
        self.update_window_title();
    }

    fn handle_location_key(self: &Rc<Self>, event: KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }

        match &event.logical_key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                let target = self.location_buffer.borrow().trim().to_string();
                if !target.is_empty() {
                    if let Ok(url) = parse_direct_navigation_target(&target) {
                        self.load(url);
                    }
                }
                self.exit_location_mode();
            }
            WinitKey::Named(WinitNamedKey::Escape) => self.exit_location_mode(),
            WinitKey::Named(WinitNamedKey::Backspace) => {
                if self.location_replace_on_text.replace(false) {
                    self.location_buffer.borrow_mut().clear();
                } else {
                    self.location_buffer.borrow_mut().pop();
                }
                self.update_window_title();
            }
            WinitKey::Named(WinitNamedKey::Space) => {
                self.push_location_text(" ");
            }
            WinitKey::Character(value) if value.chars().all(|c| !c.is_control()) => {
                self.push_location_text(value);
            }
            _ => {}
        }
    }

    fn push_location_text(&self, text: &str) {
        if self.location_replace_on_text.replace(false) {
            self.location_buffer.borrow_mut().clear();
        }
        self.location_buffer.borrow_mut().push_str(text);
        self.update_window_title();
    }

    fn load(self: &Rc<Self>, url: Url) {
        self.location_active.set(false);
        self.location_replace_on_text.set(false);
        // Navigate the active webview in place so Servo records session history
        // and back/forward work. All webviews share the window rendering context,
        // so this keeps the embedded window stable; only fall back to a retained
        // navigation (new webview) when there is no active webview yet.
        if let Some(webview) = self.active_webview() {
            self.pending_navigation.borrow_mut().take();
            webview.load(url);
            self.wake_after_browser_command();
            self.update_window_title();
        } else {
            self.start_retained_navigation(url);
        }
    }

    fn request_window_resize(&self, target: PhysicalSize<u32>) {
        if let Some(applied_size) = self.window.request_inner_size(target) {
            self.resize_all_webviews(applied_size);
        }
        self.window.request_redraw();
    }

    fn resize_all_webviews(&self, size: PhysicalSize<u32>) {
        self.last_window_size.set(size);
        for webview in self.webviews.borrow().iter() {
            webview.resize(size);
        }
        if let Some(pending) = self.pending_navigation.borrow().as_ref() {
            pending.webview.resize(size);
        }
    }

    fn reload(&self) {
        if let Some(webview) = self.active_webview() {
            self.pending_navigation.borrow_mut().take();
            webview.reload();
            self.wake_after_browser_command();
        }
    }

    fn can_go_back(&self) -> bool {
        self.active_webview()
            .map(|webview| webview.can_go_back())
            .unwrap_or(false)
    }

    fn can_go_forward(&self) -> bool {
        self.active_webview()
            .map(|webview| webview.can_go_forward())
            .unwrap_or(false)
    }

    fn go_back(&self) {
        if !self.can_go_back() {
            return;
        }
        if let Some(webview) = self.active_webview() {
            self.pending_navigation.borrow_mut().take();
            webview.go_back(1);
            self.wake_after_browser_command();
        }
    }

    fn go_forward(&self) {
        if !self.can_go_forward() {
            return;
        }
        if let Some(webview) = self.active_webview() {
            self.pending_navigation.borrow_mut().take();
            webview.go_forward(1);
            self.wake_after_browser_command();
        }
    }

    fn new_tab(self: &Rc<Self>) {
        self.new_tab_with_url(Url::parse("about:blank").expect("about:blank should parse"));
        self.location_active.set(true);
        self.location_replace_on_text.set(false);
        self.update_window_title();
    }

    fn new_tab_with_url(self: &Rc<Self>, url: Url) {
        if let Some(active) = self.active_webview() {
            active.hide();
        }
        self.pending_navigation.borrow_mut().take();
        let webview = self.create_webview(url);
        self.webviews.borrow_mut().push(webview);
        self.active_index
            .set(self.webviews.borrow().len().saturating_sub(1));
        self.location_buffer.borrow_mut().clear();
        self.location_active.set(false);
        self.location_replace_on_text.set(false);
        self.focus_active_webview();
        self.wake_after_browser_command();
        self.update_window_title();
    }

    fn close_tab(self: &Rc<Self>) {
        let mut webviews = self.webviews.borrow_mut();
        self.pending_navigation.borrow_mut().take();
        if webviews.len() <= 1 {
            drop(webviews);
            self.load(Url::parse("about:blank").expect("about:blank should parse"));
            return;
        }
        let index = self.active_index.get().min(webviews.len() - 1);
        webviews.remove(index);
        let next_index = index.min(webviews.len() - 1);
        self.active_index.set(next_index);
        drop(webviews);
        self.focus_active_webview();
        self.wake_after_browser_command();
        self.update_window_title();
    }

    fn next_tab(&self) {
        let len = self.webviews.borrow().len();
        if len <= 1 {
            return;
        }
        if let Some(active) = self.active_webview() {
            active.hide();
        }
        self.pending_navigation.borrow_mut().take();
        self.active_index.set((self.active_index.get() + 1) % len);
        self.focus_active_webview();
        self.wake_after_browser_command();
        self.update_window_title();
    }

    fn previous_tab(&self) {
        let len = self.webviews.borrow().len();
        if len <= 1 {
            return;
        }
        if let Some(active) = self.active_webview() {
            active.hide();
        }
        self.pending_navigation.borrow_mut().take();
        self.active_index
            .set((self.active_index.get() + len - 1) % len);
        self.focus_active_webview();
        self.wake_after_browser_command();
        self.update_window_title();
    }

    fn select_tab(&self, index: usize) {
        let len = self.webviews.borrow().len();
        if index >= len || index == self.active_index.get() {
            return;
        }
        if let Some(active) = self.active_webview() {
            active.hide();
        }
        self.pending_navigation.borrow_mut().take();
        self.active_index.set(index);
        self.focus_active_webview();
        self.wake_after_browser_command();
        self.update_window_title();
    }

    fn run_tab_smoke(self: &Rc<Self>) {
        self.new_tab_with_url(direct_tab_smoke_url());
        self.previous_tab();
        self.next_tab();
        self.wake_after_browser_command();
    }

    fn run_location_smoke(self: &Rc<Self>, target: &str) -> Result<Url, String> {
        self.enter_location_mode();
        self.push_location_text(target);
        let expected = parse_direct_navigation_target(&self.location_buffer.borrow())?;
        self.load(expected.clone());
        self.wake_after_browser_command();
        Ok(expected)
    }

    fn send_text(&self, text: String) {
        if text.is_empty() {
            return;
        }
        self.notify_interactive_inputs(vec![
            InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
                KeyState::Down,
                Key::Named(ServoNamedKey::Process),
            )),
            InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
                state: CompositionState::End,
                data: text,
            })),
            InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
                KeyState::Up,
                Key::Named(ServoNamedKey::Process),
            )),
        ]);
    }
}

fn text_from_key(key: &WinitKey) -> Option<String> {
    match key {
        WinitKey::Character(value) if value.chars().all(|c| !c.is_control()) => {
            Some(value.to_string())
        }
        WinitKey::Named(WinitNamedKey::Space) => Some(" ".to_string()),
        _ => None,
    }
}

fn servo_key_from_winit(key: &WinitKey) -> Option<Key> {
    match key {
        WinitKey::Named(WinitNamedKey::Enter) => Some(Key::Named(ServoNamedKey::Enter)),
        WinitKey::Named(WinitNamedKey::Backspace) => Some(Key::Named(ServoNamedKey::Backspace)),
        WinitKey::Named(WinitNamedKey::Tab) => Some(Key::Named(ServoNamedKey::Tab)),
        WinitKey::Named(WinitNamedKey::Escape) => Some(Key::Named(ServoNamedKey::Escape)),
        WinitKey::Named(WinitNamedKey::ArrowLeft) => Some(Key::Named(ServoNamedKey::ArrowLeft)),
        WinitKey::Named(WinitNamedKey::ArrowRight) => Some(Key::Named(ServoNamedKey::ArrowRight)),
        WinitKey::Named(WinitNamedKey::ArrowUp) => Some(Key::Named(ServoNamedKey::ArrowUp)),
        WinitKey::Named(WinitNamedKey::ArrowDown) => Some(Key::Named(ServoNamedKey::ArrowDown)),
        WinitKey::Named(WinitNamedKey::Delete) => Some(Key::Named(ServoNamedKey::Delete)),
        WinitKey::Named(WinitNamedKey::Home) => Some(Key::Named(ServoNamedKey::Home)),
        WinitKey::Named(WinitNamedKey::End) => Some(Key::Named(ServoNamedKey::End)),
        WinitKey::Named(WinitNamedKey::PageUp) => Some(Key::Named(ServoNamedKey::PageUp)),
        WinitKey::Named(WinitNamedKey::PageDown) => Some(Key::Named(ServoNamedKey::PageDown)),
        _ => None,
    }
}

fn servo_mouse_button(button: WinitMouseButton) -> Option<MouseButton> {
    match button {
        WinitMouseButton::Left => Some(MouseButton::Left),
        WinitMouseButton::Right => Some(MouseButton::Right),
        WinitMouseButton::Middle => Some(MouseButton::Middle),
        WinitMouseButton::Back => Some(MouseButton::Back),
        WinitMouseButton::Forward => Some(MouseButton::Forward),
        WinitMouseButton::Other(value) => Some(MouseButton::Other(value)),
    }
}

fn parse_direct_navigation_target(input: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if let Ok(url) = Url::parse(trimmed) {
        return Ok(url);
    }
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return Url::parse(&format!("https://{}", trimmed))
            .map_err(|error| format!("URL parse failed: {error}"));
    }
    let query = url::form_urlencoded::byte_serialize(trimmed.as_bytes()).collect::<String>();
    Url::parse(&format!("https://duckduckgo.com/?q={query}"))
        .map_err(|error| format!("Search URL parse failed: {error}"))
}

fn certificate_fingerprint_sha256_from_base64(value: &str) -> Result<String, String> {
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    let bytes = general_purpose::STANDARD_NO_PAD
        .decode(compact.as_bytes())
        .or_else(|_| general_purpose::STANDARD.decode(compact.as_bytes()))
        .map_err(|error| format!("certificate DER base64 decode failed: {error}"))?;
    Ok(hex_lower(&Sha256::digest(bytes)))
}

fn append_no_proxy_host(no_proxy: &mut String, host: &str) {
    if host.trim().is_empty() {
        return;
    }
    let already_present = no_proxy
        .split(',')
        .map(str::trim)
        .any(|entry| entry.eq_ignore_ascii_case(host));
    if already_present {
        return;
    }
    if !no_proxy.trim().is_empty() {
        no_proxy.push(',');
    }
    no_proxy.push_str(host);
}

fn sanitize_direct_chrome_label(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            _ => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn direct_tab_smoke_url() -> Url {
    let html = format!(
        "<!doctype html><meta charset='utf-8'><title>{DIRECT_TAB_SMOKE_TITLE}</title>\
<body style='margin:0;background:#10161d;color:white;font:24px sans-serif;display:grid;place-items:center;height:100vh'>\
<main>Direct tab smoke</main></body>"
    );
    let encoded: String = url::form_urlencoded::byte_serialize(html.as_bytes()).collect();
    Url::parse(&format!("data:text/html,{encoded}")).expect("direct tab smoke URL should parse")
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose, Engine as _};

    use std::time::Duration;

    use super::{
        certificate_fingerprint_sha256_from_base64, keep_worst_total_frame,
        parse_direct_navigation_target, retained_navigation_title_target,
        RETAINED_NAVIGATION_TITLE_PREFIX,
    };

    #[test]
    fn keeps_breakdown_of_worst_total_frame() {
        let ms = Duration::from_millis;
        // First heavy frame: 1.3s total, paint-dominated.
        let first = keep_worst_total_frame(None, (ms(1300), ms(4), ms(1200), ms(20)));
        assert_eq!(first, (ms(1300), ms(4), ms(1200), ms(20)));
        // A later, lighter frame with a momentarily higher spin must NOT replace
        // the recorded breakdown; otherwise spin/paint/present mix across frames
        // and the reported decomposition no longer belongs to one real frame.
        let kept = keep_worst_total_frame(Some(first), (ms(200), ms(150), ms(40), ms(10)));
        assert_eq!(kept, (ms(1300), ms(4), ms(1200), ms(20)));
        // A new worst-total frame replaces the whole co-recorded breakdown.
        let replaced = keep_worst_total_frame(Some(kept), (ms(1500), ms(900), ms(560), ms(40)));
        assert_eq!(replaced, (ms(1500), ms(900), ms(560), ms(40)));
    }

    #[test]
    fn parses_direct_location_targets() {
        assert_eq!(
            parse_direct_navigation_target("https://example.com/path")
                .unwrap()
                .as_str(),
            "https://example.com/path"
        );
        assert_eq!(
            parse_direct_navigation_target("example.com")
                .unwrap()
                .as_str(),
            "https://example.com/"
        );
        assert_eq!(
            parse_direct_navigation_target("servo browser")
                .unwrap()
                .as_str(),
            "https://duckduckgo.com/?q=servo+browser"
        );
    }

    #[test]
    fn hashes_badcert_base64_certificate_bytes() {
        let fingerprint = certificate_fingerprint_sha256_from_base64("aGVsbG8").unwrap();
        assert_eq!(
            fingerprint,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn decodes_retained_navigation_title_target() {
        let target = "https://example.com/path?q=servo%20browser";
        let encoded = general_purpose::STANDARD.encode(target.as_bytes());
        let title = format!("{RETAINED_NAVIGATION_TITLE_PREFIX}{encoded}");
        assert_eq!(
            retained_navigation_title_target(Some(&title))
                .unwrap()
                .as_str(),
            target
        );
        assert!(retained_navigation_title_target(Some("Example Domain")).is_none());
    }
}

fn spawn_stdin_host_command_reader(
    proxy: EventLoopProxy<DirectServoUserEvent>,
) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if tx.send(line).is_err() {
                break;
            }
            let _ = proxy.send_event(DirectServoUserEvent::HostCommand);
        }
    });
    rx
}

fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(target_os = "windows")]
fn prime_windows_angle_runtime() -> Result<(), String> {
    let mut current_path = env::var_os("PATH").unwrap_or_default();
    for candidate in windows_angle_runtime_candidates() {
        if candidate.join("libEGL.dll").is_file() && candidate.join("libGLESv2.dll").is_file() {
            let already_present = env::split_paths(&current_path).any(|path| path == candidate);
            if !already_present {
                let mut paths = vec![candidate.clone()];
                paths.extend(env::split_paths(&current_path));
                current_path = env::join_paths(paths).map_err(|error| {
                    format!("failed to extend PATH for Servo ANGLE runtime: {error}")
                })?;
                env::set_var("PATH", &current_path);
            }
            return Ok(());
        }
    }

    Err(format!(
        "Servo requires libEGL.dll and libGLESv2.dll on PATH. Checked: {}",
        windows_angle_runtime_candidates()
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

#[cfg(not(target_os = "windows"))]
fn prime_windows_angle_runtime() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_angle_runtime_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    candidates.extend(existing_subdirectories(
        r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application",
    ));
    candidates.extend(existing_subdirectories(
        r"C:\Program Files (x86)\Microsoft\EdgeCore",
    ));
    candidates.extend(existing_subdirectories(
        r"C:\Program Files (x86)\Microsoft\Edge\Application",
    ));
    candidates.extend(existing_subdirectories(
        r"C:\Program Files\Google\Chrome\Application",
    ));
    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\Microsoft\EdgeCore\Optimized",
    ));
    candidates.push(PathBuf::from(r"C:\Program Files\Mozilla Firefox"));
    candidates.push(PathBuf::from(r"C:\Program Files\Firefox Developer Edition"));
    candidates
}

#[cfg(target_os = "windows")]
fn existing_subdirectories(root: &str) -> Vec<PathBuf> {
    std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

pub fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis >= 1000 {
        format!("{:.1}s", millis as f64 / 1000.0)
    } else {
        format!("{millis}ms")
    }
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let mut out: String = value.chars().take(keep).collect();
    out.push_str("...");
    out
}

/// Timing breakdown of one direct frame: `(total, spin, paint, present)`.
type DirectFrameTiming = (Duration, Duration, Duration, Duration);

/// Keep whichever frame has the larger `total`, returning its full
/// `(total, spin, paint, present)` breakdown. This co-records the spin/paint/
/// present split with a single worst-total frame instead of mixing independent
/// component maxima from different frames, so the reported decomposition can be
/// attributed to one real frame.
fn keep_worst_total_frame(
    current: Option<DirectFrameTiming>,
    candidate: DirectFrameTiming,
) -> DirectFrameTiming {
    match current {
        Some(current) if current.0 >= candidate.0 => current,
        _ => candidate,
    }
}

fn force_focus_window(window: &Window) {
    window.focus_window();
    focus_window_platform(window);
}

#[cfg(target_os = "windows")]
fn focus_window_platform(window: &Window) {
    let Some(hwnd) = hwnd_from_window(window) else {
        return;
    };
    unsafe {
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        SetActiveWindow(hwnd);
        SetFocus(hwnd);
    }
}

#[cfg(target_os = "windows")]
fn hwnd_from_window(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as _),
        _ => None,
    }
}

#[cfg(not(target_os = "windows"))]
fn focus_window_platform(_window: &Window) {}
