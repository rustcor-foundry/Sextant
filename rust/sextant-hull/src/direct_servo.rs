use std::cell::{Cell, RefCell};
use std::env;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use servo::{
    CompositionEvent, CompositionState, DevicePoint, ImeEvent, InputEvent, Key, KeyState,
    KeyboardEvent, LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent, MouseMoveEvent,
    NamedKey as ServoNamedKey, Opts, Preferences, RenderingContext, Servo, ServoBuilder, WebView,
    WebViewBuilder, WebViewDelegate, WheelDelta, WheelEvent, WheelMode, WindowRenderingContext,
};
use url::Url;
use winit30::application::ApplicationHandler;
use winit30::dpi::PhysicalSize;
use winit30::event::{
    ElementState, KeyEvent, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use winit30::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit30::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};
use winit30::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit30::window::{Window, WindowAttributes, WindowId};

const LINE_SCROLL_PIXELS: f32 = 76.0;
const VERIFIED_INPUT_TEXT: &str = "sextant";
const VERIFIED_INPUT_TITLE: &str = "typed:sextant";
const VERIFIED_INPUT_CLICK_X: f32 = 180.0;
const VERIFIED_INPUT_CLICK_Y: f32 = 128.0;
const SEARCH_SUBMIT_TEXT: &str = "sextant";
const SEARCH_SUBMIT_TITLE: &str = "search:sextant";
const DIRECT_TAB_SMOKE_TITLE: &str = "Sextant-Direct-Tab-Smoke";
const LIVE_SEARCH_TEXT: &str = "Sextant";
const LIVE_SEARCH_FALLBACK_CLICK_X: f32 = 590.0;
const LIVE_SEARCH_FALLBACK_CLICK_Y: f32 = 330.0;
const LIVE_SEARCH_GOOGLE_CLICK_Y: f32 = 285.0;
const LIVE_SEARCH_READY_DELAY: Duration = Duration::from_millis(3500);
const LIVE_SEARCH_CLICK_SETTLE_DELAY: Duration = Duration::from_millis(350);
const LIVE_SEARCH_FALLBACK_DELAY: Duration = Duration::from_secs(6);

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
    pub scripted_search_submit_smoke: bool,
    pub scripted_live_search_smoke: bool,
    pub scripted_location: Option<String>,
    pub scripted_history: Option<String>,
    pub scripted_load_smoke: bool,
    pub scripted_reload_smoke: bool,
    pub scripted_resize_smoke: bool,
    pub scripted_tab_smoke: bool,
    pub config_dir: Option<PathBuf>,
    pub disable_http_cache: bool,
}

pub struct DirectServoOutcome {
    pub first_present: Option<Duration>,
    pub first_interaction_frame: Option<Duration>,
    pub first_interaction_before_load_complete: Option<bool>,
    pub verified_input_frame: Option<Duration>,
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
    scripted_search_submit_smoke: bool,
    scripted_live_search_smoke: bool,
    scripted_location: Option<String>,
    scripted_history: Option<String>,
    scripted_load_smoke: bool,
    scripted_reload_smoke: bool,
    scripted_resize_smoke: bool,
    scripted_tab_smoke: bool,
    config_dir: Option<PathBuf>,
    disable_http_cache: bool,
    started: Instant,
    input_started: Option<Instant>,
    first_interaction_started: Option<Instant>,
    first_interaction_before_load_complete: Option<bool>,
    scripted_first_interaction_sent: bool,
    verified_input_started: Option<Instant>,
    verified_input_before_load_complete: Option<bool>,
    scripted_verified_input_sent: bool,
    scripted_verified_input_done: bool,
    search_submit_started: Option<Instant>,
    scripted_search_submit_sent: bool,
    scripted_search_submit_done: bool,
    live_search_started: Option<Instant>,
    live_search_last_attempt: Option<Instant>,
    live_search_pending_text_since: Option<Instant>,
    live_search_attempts: u8,
    scripted_live_search_sent: bool,
    scripted_live_search_done: bool,
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

struct DirectServoState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webviews: RefCell<Vec<WebView>>,
    active_index: Cell<usize>,
    frame_ready: Cell<bool>,
    first_present: Cell<Option<Duration>>,
    first_interaction_frame: Cell<Option<Duration>>,
    first_interaction_before_load_complete: Cell<Option<bool>>,
    verified_input_frame: Cell<Option<Duration>>,
    verified_input: Cell<Option<bool>>,
    verified_input_before_load_complete: Cell<Option<bool>>,
    search_submit_frame: Cell<Option<Duration>>,
    search_submit: Cell<Option<bool>>,
    live_search_frame: Cell<Option<Duration>>,
    live_search: Cell<Option<bool>>,
    input_frame: Cell<Option<Duration>>,
    location_frame: Cell<Option<Duration>>,
    history_navigation_frame: Cell<Option<Duration>>,
    history_back_frame: Cell<Option<Duration>>,
    history_forward_frame: Cell<Option<Duration>>,
    load_complete: Cell<Option<Duration>>,
    reload_frame: Cell<Option<Duration>>,
    resize_frame: Cell<Option<Duration>>,
    tab_frame: Cell<Option<Duration>>,
    tab_smoke_url: RefCell<Option<String>>,
    tab_smoke_title: RefCell<Option<String>>,
    last_window_size: Cell<PhysicalSize<u32>>,
    cursor_position: Cell<(f32, f32)>,
    input_events: Cell<u64>,
    modifiers: Cell<ModifiersState>,
    location_active: Cell<bool>,
    location_replace_on_text: Cell<bool>,
    location_buffer: RefCell<String>,
}

impl WebViewDelegate for DirectServoState {
    fn notify_url_changed(&self, webview: WebView, _url: Url) {
        if self.is_active_webview(&webview) {
            self.update_window_title();
        }
    }

    fn notify_page_title_changed(&self, webview: WebView, _title: Option<String>) {
        if self.is_active_webview(&webview) {
            self.update_window_title();
        }
    }

    fn notify_history_changed(&self, webview: WebView, _entries: Vec<Url>, _current: usize) {
        if self.is_active_webview(&webview) {
            self.update_window_title();
        }
    }

    fn notify_load_status_changed(&self, webview: WebView, _status: LoadStatus) {
        if self.is_active_webview(&webview) {
            self.update_window_title();
            self.window.request_redraw();
        }
    }

    fn notify_new_frame_ready(&self, webview: WebView) {
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
            scripted_search_submit_smoke: options.scripted_search_submit_smoke,
            scripted_live_search_smoke: options.scripted_live_search_smoke,
            scripted_location: options.scripted_location,
            scripted_history: options.scripted_history,
            scripted_load_smoke: options.scripted_load_smoke,
            scripted_reload_smoke: options.scripted_reload_smoke,
            scripted_resize_smoke: options.scripted_resize_smoke,
            scripted_tab_smoke: options.scripted_tab_smoke,
            config_dir: options.config_dir,
            disable_http_cache: options.disable_http_cache,
            started: Instant::now(),
            input_started: None,
            first_interaction_started: None,
            first_interaction_before_load_complete: None,
            scripted_first_interaction_sent: false,
            verified_input_started: None,
            verified_input_before_load_complete: None,
            scripted_verified_input_sent: false,
            scripted_verified_input_done: false,
            search_submit_started: None,
            scripted_search_submit_sent: false,
            scripted_search_submit_done: false,
            live_search_started: None,
            live_search_last_attempt: None,
            live_search_pending_text_since: None,
            live_search_attempts: 0,
            scripted_live_search_sent: false,
            scripted_live_search_done: false,
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
                state.window.request_redraw();
            }
            return true;
        }

        if state.active_title_matches(VERIFIED_INPUT_TITLE) {
            if let Some(started) = self.verified_input_started {
                let elapsed = started.elapsed();
                state.verified_input_frame.set(Some(elapsed));
                state.verified_input.set(Some(true));
                self.scripted_verified_input_done = true;
                println!(
                    "[{}] verified input direct frame in {}",
                    self.log_prefix,
                    format_duration(elapsed)
                );
                println!("[{}] verified input true", self.log_prefix);
                println!(
                    "[{}] verified input before load complete {}",
                    self.log_prefix,
                    self.verified_input_before_load_complete.unwrap_or(false)
                );
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
                state.window.request_redraw();
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
            } else {
                state.window.request_redraw();
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
            } else {
                state.window.request_redraw();
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
            } else {
                state.window.request_redraw();
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
            let should_retry = self.live_search_attempts < 4
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
                println!("[{}] live search attempt search box click", self.log_prefix);
                state.click_at(
                    LIVE_SEARCH_FALLBACK_CLICK_X,
                    self.live_search_click_y(state),
                );
                self.live_search_pending_text_since = Some(Instant::now());
            }
            3 => {
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

impl ApplicationHandler for DirectServoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let display_handle = event_loop
            .display_handle()
            .expect("failed to get display handle");
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title(self.title.clone())
                    .with_inner_size(PhysicalSize::new(1180, 760)),
            )
            .expect("failed to create direct Servo window");
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
        let mut preferences = Preferences::default();
        if self.disable_http_cache {
            preferences.network_http_cache_disabled = true;
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
            webviews: RefCell::new(Vec::new()),
            active_index: Cell::new(0),
            frame_ready: Cell::new(false),
            first_present: Cell::new(None),
            first_interaction_frame: Cell::new(None),
            first_interaction_before_load_complete: Cell::new(None),
            verified_input_frame: Cell::new(None),
            verified_input: Cell::new(None),
            verified_input_before_load_complete: Cell::new(None),
            search_submit_frame: Cell::new(None),
            search_submit: Cell::new(None),
            live_search_frame: Cell::new(None),
            live_search: Cell::new(None),
            input_frame: Cell::new(None),
            location_frame: Cell::new(None),
            history_navigation_frame: Cell::new(None),
            history_back_frame: Cell::new(None),
            history_forward_frame: Cell::new(None),
            load_complete: Cell::new(None),
            reload_frame: Cell::new(None),
            resize_frame: Cell::new(None),
            tab_frame: Cell::new(None),
            tab_smoke_url: RefCell::new(None),
            tab_smoke_title: RefCell::new(None),
            last_window_size: Cell::new(initial_window_size),
            cursor_position: Cell::new((0.0, 0.0)),
            input_events: Cell::new(0),
            modifiers: Cell::new(ModifiersState::empty()),
            location_active: Cell::new(false),
            location_replace_on_text: Cell::new(false),
            location_buffer: RefCell::new(String::new()),
        });

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

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(state) = self.state.as_ref() {
            state.servo.spin_event_loop();
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(state) = self.state.as_ref() {
                    state.handle_mouse_move(position.x as f32, position.y as f32);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(direct_state) = self.state.as_ref() {
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
                        if let Some(webview) = state.active_webview() {
                            webview.paint();
                            state.rendering_context.present();
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
                            state.window.request_redraw();
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
                    } else if self.drive_live_search_smoke(&state, event_loop) {
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
            if state.frame_ready.replace(false) {
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

pub fn run(options: DirectServoOptions) -> Result<DirectServoOutcome, String> {
    install_rustls_crypto_provider();
    prime_windows_angle_runtime()?;

    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    let smoke = options.smoke;
    let verified_input_smoke = options.scripted_verified_input_smoke;
    let search_submit_smoke = options.scripted_search_submit_smoke;
    let live_search_smoke = options.scripted_live_search_smoke;
    if verified_input_smoke || search_submit_smoke || live_search_smoke {
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
        first_interaction_frame: app.first_interaction_frame(),
        first_interaction_before_load_complete: app.first_interaction_before_load_complete(),
        verified_input_frame: app.verified_input_frame(),
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

fn verified_input_fixture_html() -> String {
    "<!doctype html><meta charset='utf-8'><title>SextantVerifiedInputSmoke</title>\
<body style='margin:0;background:#10161d;color:white;font:20px sans-serif;height:100vh'>\
<input id='q' style='position:absolute;left:80px;top:80px;width:420px;padding:18px;font:24px sans-serif' value='' onmousedown=\"document.title='clicked'\" onfocus=\"document.title='focused'\" oninput=\"document.title='typed:'+this.value\"></body>"
        .to_string()
}

impl DirectServoState {
    fn create_webview(self: &Rc<Self>, url: Url) -> WebView {
        WebViewBuilder::new(&self.servo, self.rendering_context.clone())
            .url(url)
            .delegate(self.clone())
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
    }

    fn is_active_webview(&self, webview: &WebView) -> bool {
        self.active_webview()
            .map(|active| active.id() == webview.id())
            .unwrap_or(false)
    }

    fn active_load_complete(&self) -> bool {
        self.active_webview()
            .map(|webview| webview.load_status() == LoadStatus::Complete)
            .unwrap_or(false)
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
        let back = if self.can_go_back() { "<" } else { "-" };
        let forward = if self.can_go_forward() { ">" } else { "-" };
        let index = self.active_index.get() + 1;
        let total = self.webviews.borrow().len().max(1);
        self.window.set_title(&format!(
            "{label} - Sextant Direct [{back}{forward}] Tab {index}/{total}"
        ));
    }

    fn notify_input(&self, event: InputEvent) {
        if let Some(webview) = self.active_webview() {
            webview.notify_input_event(event);
            self.input_events.set(self.input_events.get() + 1);
            self.servo.spin_event_loop();
            self.frame_ready.set(true);
            self.window.request_redraw();
        }
    }

    fn wake_after_browser_command(&self) {
        self.servo.spin_event_loop();
        self.frame_ready.set(true);
        self.window.request_redraw();
    }

    fn handle_mouse_move(&self, x: f32, y: f32) {
        self.cursor_position.set((x.max(0.0), y.max(0.0)));
        self.notify_input(InputEvent::MouseMove(MouseMoveEvent::new(
            DevicePoint::new(x.max(0.0), y.max(0.0)).into(),
        )));
    }

    fn handle_mouse_button(&self, button: WinitMouseButton, state: ElementState) {
        let Some(button) = servo_mouse_button(button) else {
            return;
        };
        let action = match state {
            ElementState::Pressed => MouseButtonAction::Down,
            ElementState::Released => MouseButtonAction::Up,
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
        self.notify_input(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
            KeyState::Down,
            Key::Named(key),
        )));
        self.notify_input(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
            KeyState::Up,
            Key::Named(key),
        )));
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
            self.notify_input(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
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

    fn handle_location_key(&self, event: KeyEvent) {
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

    fn load(&self, url: Url) {
        if let Some(webview) = self.active_webview() {
            self.location_active.set(false);
            self.location_replace_on_text.set(false);
            webview.load(url);
            self.wake_after_browser_command();
            self.update_window_title();
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
    }

    fn reload(&self) {
        if let Some(webview) = self.active_webview() {
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
            webview.go_back(1);
            self.wake_after_browser_command();
        }
    }

    fn go_forward(&self) {
        if !self.can_go_forward() {
            return;
        }
        if let Some(webview) = self.active_webview() {
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

    fn close_tab(&self) {
        let mut webviews = self.webviews.borrow_mut();
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
        self.active_index
            .set((self.active_index.get() + len - 1) % len);
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

    fn run_location_smoke(&self, target: &str) -> Result<Url, String> {
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
        self.notify_input(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
            KeyState::Down,
            Key::Named(ServoNamedKey::Process),
        )));
        self.notify_input(InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
            state: CompositionState::End,
            data: text,
        })));
        self.notify_input(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
            KeyState::Up,
            Key::Named(ServoNamedKey::Process),
        )));
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
    use super::parse_direct_navigation_target;

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
