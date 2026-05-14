use chrono::Utc;
use sextant_engine::{
    BrowserKey, EngineBackend, EngineStatus, NodeType, RenderedFrame, SextantEngine, Tab,
};
use sextant_log::{CaptainsLog, LogEntry, LogStatus};
use sextant_wake::{DigitalWake, WakeEntry};
use softbuffer::{Context, Surface};
use std::env;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;
use uuid::Uuid;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowBuilder};

const BG: u32 = 0x0010161d;
const PANEL: u32 = 0x0019232c;
const PANEL_ALT: u32 = 0x00202b35;
const PANEL_DARK: u32 = 0x000b1016;
const FIELD: u32 = 0x000d1319;
const FIELD_FOCUS: u32 = 0x00172229;
const BUTTON_IDLE: u32 = 0x00345668;
const BUTTON_HOVER: u32 = 0x004a7488;
const BUTTON_BRIGHT: u32 = 0x000ec7e8;
const BUTTON_ACTIVE: u32 = 0x002fbf71;
const BUTTON_DISABLED: u32 = 0x00212a32;
const TEXT: u32 = 0x00dce7ef;
const TEXT_DIM: u32 = 0x0093a4b0;
const STATUS_OK: u32 = 0x002fbf71;
const STATUS_WARN: u32 = 0x00d9a441;
const BORDER: u32 = 0x00313d48;
const RAIL_WIDTH: u32 = 320;
const STATUS_BAR_H: u32 = 30;
const CHROME_H: u32 = 54;
const STRIP_H: u32 = 48;
const TAB_H: u32 = 46;
const METRIC_Y: u32 = CHROME_H + STRIP_H + TAB_H + 12;
const METRIC_H: u32 = 76;
const GLYPH_W: u32 = 5;
const GLYPH_GAP: u32 = 2;
const FRAME_REFRESH_IDLE: Duration = Duration::from_millis(500);
const FRAME_REFRESH_DIRTY: Duration = Duration::from_millis(180);
const FRAME_WARMUP_BUDGET: u8 = 18;
const OPERATOR_DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Rect {
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && y >= self.y as f64
            && x < (self.x + self.w) as f64
            && y < (self.y + self.h) as f64
    }
}

#[derive(Clone, Copy)]
enum Action {
    Navigate,
    NewTab,
    Back,
    Forward,
    Reload,
    CloseTab,
    DistillActive,
    SearchWake,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusTarget {
    Address,
    Wake,
    Browser,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainView {
    Browser,
    Wake,
    Log,
    Validation,
}

#[derive(Default)]
struct ValidationState {
    navigation_seen: bool,
    browser_focus_seen: bool,
    frame_seen: bool,
    distill_seen: bool,
    wake_seen: bool,
    log_seen: bool,
    tab_control_seen: bool,
    error_seen: bool,
}

#[derive(Clone, Copy)]
enum ValidationStatus {
    Pass,
    Waiting,
    Attention,
}

struct ValidationRow {
    label: &'static str,
    status: ValidationStatus,
    detail: String,
}

enum OperatorStep {
    Fill { selector: String, value: String },
    Click { selector: String },
    Submit { selector: String },
    Expect { text: String },
}

struct OperatorRunSpec {
    target: String,
    steps: Vec<OperatorStep>,
}

struct ButtonRegion {
    rect: Rect,
    label: &'static str,
    action: Action,
}

struct LiteApp {
    engine: SextantEngine,
    wake: DigitalWake,
    log: CaptainsLog,
    persona_id: String,
    address_input: String,
    wake_query: String,
    last_status: String,
    last_ok: bool,
    cursor: Option<(f64, f64)>,
    focus: FocusTarget,
    address_rect: Rect,
    wake_rect: Rect,
    browser_tab_rect: Rect,
    wake_tab_rect: Rect,
    log_tab_rect: Rect,
    validation_tab_rect: Rect,
    browser_viewport_rect: Rect,
    buttons: Vec<ButtonRegion>,
    wake_results: Vec<WakeEntry>,
    recent_logs: Vec<LogEntry>,
    page_scroll: i32,
    main_view: MainView,
    latest_frame: Option<RenderedFrame>,
    last_frame_refresh: Instant,
    frame_dirty: bool,
    frame_refresh_budget: u8,
    validation: ValidationState,
}

impl LiteApp {
    fn new() -> Result<Self, String> {
        Self::new_with_data_dir(app_data_dir().join("lite"))
    }

    fn new_with_data_dir(data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        let mut app = Self {
            engine: SextantEngine::new(),
            wake: DigitalWake::open(data_dir.join("wake.db")).map_err(|e| e.to_string())?,
            log: CaptainsLog::new(data_dir.join("captains-log.db")).map_err(|e| e.to_string())?,
            persona_id: "lite-persona".to_string(),
            address_input: "https://example.com".to_string(),
            wake_query: "example".to_string(),
            last_status: "Ready. Type a URL or search, then press Enter.".to_string(),
            last_ok: true,
            cursor: None,
            focus: FocusTarget::Address,
            address_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            wake_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            browser_tab_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            wake_tab_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            log_tab_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            validation_tab_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            browser_viewport_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            buttons: Vec::new(),
            wake_results: Vec::new(),
            recent_logs: Vec::new(),
            page_scroll: 0,
            main_view: MainView::Browser,
            latest_frame: None,
            last_frame_refresh: Instant::now(),
            frame_dirty: false,
            frame_refresh_budget: 0,
            validation: ValidationState::default(),
        };
        app.refresh_logs();
        Ok(app)
    }

    fn update_title(&self, window: &Window) {
        window.set_title(&format!("Sextant Lite - {}", self.last_status));
    }

    fn layout(&mut self, size: PhysicalSize<u32>) {
        let width = size.width.max(900);
        let rail_x = right_rail_x(width);
        let main_right = rail_x.saturating_sub(18);
        let margin = 24;
        let gap = 8;
        let button_h = 30;
        self.address_rect = Rect {
            x: 112,
            y: 12,
            w: main_right.saturating_sub(224),
            h: button_h,
        };
        self.wake_rect = Rect {
            x: rail_x + 20,
            y: size.height.max(620).saturating_sub(STATUS_BAR_H + 66),
            w: RAIL_WIDTH.saturating_sub(40),
            h: 36,
        };
        self.browser_tab_rect = Rect {
            x: 24,
            y: CHROME_H + STRIP_H + 5,
            w: 138,
            h: 30,
        };
        self.wake_tab_rect = Rect {
            x: 172,
            y: CHROME_H + STRIP_H + 5,
            w: 92,
            h: 30,
        };
        self.log_tab_rect = Rect {
            x: 274,
            y: CHROME_H + STRIP_H + 5,
            w: 134,
            h: 30,
        };
        self.validation_tab_rect = Rect {
            x: 418,
            y: CHROME_H + STRIP_H + 5,
            w: 126,
            h: 30,
        };
        let main_panel = main_panel_rect(rail_x, size.height.max(620));
        self.browser_viewport_rect = browser_viewport_rect(main_panel);
        if self.latest_frame.is_some() {
            self.frame_dirty = true;
        }

        let go_x = self.address_rect.x + self.address_rect.w + gap;
        self.buttons = vec![
            ButtonRegion {
                rect: Rect {
                    x: go_x,
                    y: 12,
                    w: 78,
                    h: button_h,
                },
                label: "GO",
                action: Action::Navigate,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin,
                    y: 64,
                    w: 82,
                    h: button_h,
                },
                label: "NEW TAB",
                action: Action::NewTab,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 82 + gap,
                    y: 64,
                    w: 66,
                    h: button_h,
                },
                label: "BACK",
                action: Action::Back,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 148 + gap * 2,
                    y: 64,
                    w: 82,
                    h: button_h,
                },
                label: "FORWARD",
                action: Action::Forward,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 230 + gap * 3,
                    y: 64,
                    w: 76,
                    h: button_h,
                },
                label: "RELOAD",
                action: Action::Reload,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 306 + gap * 4,
                    y: 64,
                    w: 88,
                    h: button_h,
                },
                label: "CLOSE TAB",
                action: Action::CloseTab,
            },
            ButtonRegion {
                rect: Rect {
                    x: main_right.saturating_sub(210),
                    y: 64,
                    w: 90,
                    h: button_h,
                },
                label: "DISTILL",
                action: Action::DistillActive,
            },
            ButtonRegion {
                rect: Rect {
                    x: main_right.saturating_sub(106),
                    y: 64,
                    w: 98,
                    h: button_h,
                },
                label: "WAKE",
                action: Action::SearchWake,
            },
        ];
    }

    fn click(&mut self, x: f64, y: f64) {
        if self.address_rect.contains(x, y) {
            self.focus = FocusTarget::Address;
            return;
        }
        if self.wake_rect.contains(x, y) {
            self.focus = FocusTarget::Wake;
            return;
        }
        if self.browser_tab_rect.contains(x, y) {
            self.main_view = MainView::Browser;
            return;
        }
        if self.wake_tab_rect.contains(x, y) {
            self.main_view = MainView::Wake;
            return;
        }
        if self.log_tab_rect.contains(x, y) {
            self.main_view = MainView::Log;
            return;
        }
        if self.validation_tab_rect.contains(x, y) {
            self.main_view = MainView::Validation;
            return;
        }

        let Some(action) = self
            .buttons
            .iter()
            .find(|button| button.rect.contains(x, y))
            .map(|button| button.action)
        else {
            self.last_status = "Click missed a control.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        };

        if !self.action_enabled(action) {
            self.last_status = disabled_reason(action).to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        }

        self.run_action(action);
    }

    fn handle_key(&mut self, event: KeyEvent) {
        if self.focus == FocusTarget::Browser && self.latest_frame.is_some() {
            self.forward_browser_key(&event);
            return;
        }

        if event.state != ElementState::Pressed {
            return;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => match self.focus {
                FocusTarget::Address => self.navigate_input(),
                FocusTarget::Wake => self.search_wake(),
                FocusTarget::Browser => {}
            },
            Key::Named(NamedKey::Tab) => {
                self.focus = match self.focus {
                    FocusTarget::Address => FocusTarget::Wake,
                    FocusTarget::Wake => FocusTarget::Address,
                    FocusTarget::Browser => FocusTarget::Address,
                };
            }
            Key::Named(NamedKey::Backspace) => {
                self.focused_text_mut().pop();
            }
            Key::Named(NamedKey::Space) => {
                self.focused_text_mut().push(' ');
            }
            Key::Character(value) => {
                if value.chars().all(|c| !c.is_control()) {
                    self.focused_text_mut().push_str(value);
                }
            }
            _ => {}
        }
    }

    fn handle_mouse_input(&mut self, x: f64, y: f64, state: ElementState) -> bool {
        if self.main_view != MainView::Browser || !self.browser_viewport_rect.contains(x, y) {
            return false;
        }

        self.focus = FocusTarget::Browser;
        self.last_status = "Browser viewport focused.".to_string();
        self.last_ok = true;
        self.validation.browser_focus_seen = true;
        let Some((local_x, local_y)) = self.browser_point(x, y) else {
            return true;
        };
        let _ = self
            .engine
            .enqueue_mouse_move_current_viewport(local_x, local_y);
        let _ = self.engine.enqueue_mouse_button_current_viewport(
            local_x,
            local_y,
            state == ElementState::Pressed,
        );
        self.frame_dirty = true;
        true
    }

    fn handle_mouse_move(&mut self, x: f64, y: f64) {
        if self.focus != FocusTarget::Browser || !self.browser_viewport_rect.contains(x, y) {
            return;
        }
        if let Some((local_x, local_y)) = self.browser_point(x, y) {
            let _ = self
                .engine
                .enqueue_mouse_move_current_viewport(local_x, local_y);
        }
    }

    fn forward_browser_key(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        match &event.logical_key {
            Key::Character(value) if value.chars().all(|c| !c.is_control()) => {
                let _ = self
                    .engine
                    .enqueue_key_character_current_viewport(value.to_string(), pressed);
                self.frame_dirty = true;
            }
            Key::Named(NamedKey::Space) => {
                let _ = self
                    .engine
                    .enqueue_key_character_current_viewport(" ".to_string(), pressed);
                self.frame_dirty = true;
            }
            Key::Named(named) => {
                if let Some(key) = browser_key_from_winit(named) {
                    let _ = self.engine.enqueue_key_named_current_viewport(key, pressed);
                    self.frame_dirty = true;
                }
            }
            _ => {}
        }
    }

    fn browser_point(&self, x: f64, y: f64) -> Option<(f32, f32)> {
        if !self.browser_viewport_rect.contains(x, y) {
            return None;
        }
        Some((
            (x - self.browser_viewport_rect.x as f64).max(0.0) as f32,
            (y - self.browser_viewport_rect.y as f64).max(0.0) as f32,
        ))
    }

    fn scroll_at(&mut self, x: f64, y: f64, delta: &MouseScrollDelta, size: PhysicalSize<u32>) {
        if self.main_view != MainView::Browser {
            return;
        }
        let rail_x = right_rail_x(size.width.max(900));
        let panel = main_panel_rect(rail_x, size.height.max(620));
        let viewport = browser_viewport_rect(panel);
        if !viewport.contains(x, y) {
            return;
        }
        let (delta_x, delta_y, pixel_mode) = match delta {
            MouseScrollDelta::LineDelta(dx, dy) => {
                ((*dx * 76.0) as f64, (*dy * 76.0) as f64, false)
            }
            MouseScrollDelta::PixelDelta(position) => (position.x, position.y, true),
        };
        if self.latest_frame.is_some()
            && self
                .engine
                .wheel_current_viewport(delta_x, delta_y, pixel_mode)
                .is_ok()
        {
            self.refresh_frame();
            return;
        }
        self.page_scroll = (self.page_scroll - delta_y as i32).clamp(0, 4000);
    }

    fn focused_text_mut(&mut self) -> &mut String {
        match self.focus {
            FocusTarget::Address => &mut self.address_input,
            FocusTarget::Wake => &mut self.wake_query,
            FocusTarget::Browser => unreachable!("browser focus does not edit shell text"),
        }
    }

    fn run_action(&mut self, action: Action) {
        match action {
            Action::Navigate => self.navigate_input(),
            Action::NewTab => self.new_tab(),
            Action::Back => self.back(),
            Action::Forward => self.forward(),
            Action::Reload => self.reload(),
            Action::CloseTab => self.close_tab(),
            Action::DistillActive => self.distill_active(),
            Action::SearchWake => self.search_wake(),
        }
        self.refresh_logs();
    }

    fn navigate_input(&mut self) {
        let raw = self.address_input.trim();
        if raw.is_empty() {
            self.last_status = "Enter a URL or search phrase first.".to_string();
            self.last_ok = false;
            return;
        }
        match parse_navigation_target(raw) {
            Ok(url) => self.navigate_to(url),
            Err(error) => {
                self.last_status = error;
                self.last_ok = false;
            }
        }
    }

    fn navigate_to(&mut self, url: Url) {
        match self
            .engine
            .navigate_with_fallback(url.clone(), &self.persona_id)
        {
            Ok(status) => {
                self.address_input = url.to_string();
                self.page_scroll = 0;
                self.validation.navigation_seen = true;
                self.begin_frame_warmup();
                self.refresh_frame();
                self.last_status = format!(
                    "{} {} with {}. Distill fetches real page content.",
                    render_action_label(),
                    short_url(&url),
                    backend(status)
                );
                self.last_ok = true;
                let _ = self.record_log(&format!("navigate {}", url), LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Open failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(&format!("navigate {}", url), LogStatus::Failure(error));
            }
        }
    }

    fn new_tab(&mut self) {
        let id = self.engine.open_tab();
        self.address_input = "about:blank".to_string();
        self.latest_frame = None;
        self.last_status = format!("Created tab {}.", short_id(id));
        self.last_ok = true;
        self.validation.tab_control_seen = true;
        let _ = self.record_log("new tab", LogStatus::Success);
    }

    fn close_tab(&mut self) {
        let Some(tab) = self.active_tab().cloned() else {
            self.last_status = "No active tab to close.".to_string();
            self.last_ok = false;
            return;
        };

        match self.engine.close_tab(&tab.id) {
            Ok(()) => {
                self.sync_address_to_active_tab();
                self.latest_frame = None;
                self.last_status = format!("Closed tab {}.", short_id(tab.id));
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("close tab", LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Close failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("close tab", LogStatus::Failure(error));
            }
        }
    }

    fn reload(&mut self) {
        match self.engine.reload_active_tab() {
            Ok(status) => {
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.refresh_frame();
                self.last_status = format!("Reloaded active tab with {}.", backend(status));
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("reload", LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Reload failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("reload", LogStatus::Failure(error));
            }
        }
    }

    fn back(&mut self) {
        match self.engine.go_back_active_tab() {
            Ok(status) => {
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.refresh_frame();
                self.last_status = format!("Went back with {}.", backend(status));
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("back", LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Back unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("back", LogStatus::Failure(error));
            }
        }
    }

    fn forward(&mut self) {
        match self.engine.go_forward_active_tab() {
            Ok(status) => {
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.refresh_frame();
                self.last_status = format!("Went forward with {}.", backend(status));
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("forward", LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Forward unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("forward", LogStatus::Failure(error));
            }
        }
    }

    fn distill_active(&mut self) {
        match self.engine.distill_current_page() {
            Ok(page) => match self.wake.record(&self.persona_id, &page, None) {
                Ok(()) => {
                    self.page_scroll = 0;
                    self.begin_frame_warmup();
                    self.refresh_frame();
                    self.wake_query = page.title.clone();
                    self.last_status = format!("Distilled '{}' into Wake.", page.title);
                    self.last_ok = true;
                    self.validation.distill_seen = true;
                    let _ = self.record_log("distill active", LogStatus::Success);
                    self.search_wake();
                }
                Err(error) => {
                    self.last_status = format!("Wake record failed: {}", error);
                    self.last_ok = false;
                    self.validation.error_seen = true;
                    let _ =
                        self.record_log("distill active", LogStatus::Failure(error.to_string()));
                }
            },
            Err(error) => {
                self.last_status = format!("Distill failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("distill active", LogStatus::Failure(error));
            }
        }
    }

    fn search_wake(&mut self) {
        let query = self.wake_query.trim().to_string();
        match self.wake.search(&self.persona_id, &query) {
            Ok(results) => {
                self.last_status = format!("Wake search found {} result(s).", results.len());
                self.last_ok = true;
                self.validation.wake_seen = !results.is_empty();
                self.wake_results = results;
                let _ = self.record_log(&format!("search Wake '{}'", query), LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Wake search failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("search Wake", LogStatus::Failure(error.to_string()));
            }
        }
    }

    fn validation_rows(&self) -> Vec<ValidationRow> {
        let active_url = self
            .active_tab()
            .and_then(|tab| tab.url.as_ref())
            .map(short_url)
            .unwrap_or_else(|| "no URL yet".to_string());
        let distilled_title = self
            .active_tab()
            .and_then(|tab| tab.distilled_page.as_ref())
            .map(|page| page.title.clone());
        let frame_ready = self
            .latest_frame
            .as_ref()
            .map(|frame| frame.width > 0 && frame.height > 0 && !frame.pixels.is_empty())
            .unwrap_or(false);
        let has_page = distilled_title.is_some();

        vec![
            ValidationRow {
                label: "ACTIVE TAB",
                status: if self.active_tab().is_some() {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: format!("{} tab(s) in engine state", self.engine.get_tabs().len()),
            },
            ValidationRow {
                label: "NAVIGATION",
                status: if self.validation.navigation_seen {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: active_url,
            },
            ValidationRow {
                label: "VIEWPORT",
                status: if frame_ready || self.validation.frame_seen {
                    ValidationStatus::Pass
                } else if has_page {
                    ValidationStatus::Attention
                } else {
                    ValidationStatus::Waiting
                },
                detail: if frame_ready {
                    "Servo frame captured in-window".to_string()
                } else if has_page {
                    "Reader fallback visible; Servo frame not captured yet".to_string()
                } else {
                    "Press GO, then wait for frame warm-up".to_string()
                },
            },
            ValidationRow {
                label: "BROWSER INPUT",
                status: if self.validation.browser_focus_seen {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: "Click the live viewport and type or scroll".to_string(),
            },
            ValidationRow {
                label: "DISTILL",
                status: if self.validation.distill_seen || has_page {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: distilled_title
                    .unwrap_or_else(|| "No distilled page attached yet".to_string()),
            },
            ValidationRow {
                label: "WAKE",
                status: if self.validation.wake_seen || !self.wake_results.is_empty() {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: format!("{} visible result(s)", self.wake_results.len()),
            },
            ValidationRow {
                label: "CAPTAIN LOG",
                status: if self.validation.log_seen || !self.recent_logs.is_empty() {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: format!("{} recent entry row(s)", self.recent_logs.len()),
            },
            ValidationRow {
                label: "TAB CONTROLS",
                status: if self.validation.tab_control_seen {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: "Use new tab, close, reload, back, or forward".to_string(),
            },
            ValidationRow {
                label: "ERROR SURFACE",
                status: if self.validation.error_seen {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: if self.last_ok {
                    "Trigger a disabled action or failed load to verify status text".to_string()
                } else {
                    truncate(&self.last_status, 72)
                },
            },
        ]
    }

    fn validation_pass_count(&self) -> usize {
        self.validation_rows()
            .iter()
            .filter(|row| matches!(row.status, ValidationStatus::Pass))
            .count()
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.engine.get_active_tab()
    }

    fn action_enabled(&self, action: Action) -> bool {
        match action {
            Action::Navigate => !self.address_input.trim().is_empty(),
            Action::NewTab => true,
            Action::Back => self
                .active_tab()
                .map(|tab| tab.can_go_back)
                .unwrap_or(false),
            Action::Forward => self
                .active_tab()
                .map(|tab| tab.can_go_forward)
                .unwrap_or(false),
            Action::Reload => self.active_tab().and_then(|tab| tab.url.as_ref()).is_some(),
            Action::CloseTab | Action::DistillActive => self.active_tab().is_some(),
            Action::SearchWake => !self.wake_query.trim().is_empty(),
        }
    }

    fn sync_address_to_active_tab(&mut self) {
        if let Some(url) = self.active_tab().and_then(|tab| tab.url.clone()) {
            self.address_input = url.to_string();
        } else {
            self.address_input = "about:blank".to_string();
        }
    }

    fn refresh_logs(&mut self) {
        self.recent_logs = self
            .log
            .get_entries(&self.persona_id, 5)
            .unwrap_or_default();
    }

    fn refresh_frame(&mut self) {
        let viewport = self.browser_viewport_rect;
        let _ = self
            .engine
            .resize_current_viewport(viewport.w.max(1), viewport.h.max(1));
        match self.engine.capture_current_frame() {
            Ok(frame) => {
                if frame.width > 0 && frame.height > 0 && !frame.pixels.is_empty() {
                    self.validation.frame_seen = true;
                }
                self.latest_frame = Some(frame);
                self.last_frame_refresh = Instant::now();
                self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                self.frame_dirty = self.frame_refresh_budget > 0;
            }
            Err(error) => {
                self.last_frame_refresh = Instant::now();
                self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                self.frame_dirty = self.frame_refresh_budget > 0;
                self.last_status = format!("Servo frame unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    fn maybe_refresh_frame(&mut self) -> bool {
        if self.main_view != MainView::Browser
            || (self.latest_frame.is_none() && self.frame_refresh_budget == 0)
        {
            return false;
        }

        let elapsed = self.last_frame_refresh.elapsed();
        let due = if self.frame_dirty || self.frame_refresh_budget > 0 {
            elapsed >= FRAME_REFRESH_DIRTY
        } else {
            self.focus == FocusTarget::Browser && elapsed >= FRAME_REFRESH_IDLE
        };

        if due {
            self.refresh_frame();
            return true;
        }
        false
    }

    fn begin_frame_warmup(&mut self) {
        self.frame_refresh_budget = FRAME_WARMUP_BUDGET;
        self.frame_dirty = true;
    }

    fn record_log(&mut self, intent: &str, status: LogStatus) -> Result<(), String> {
        match self.log.record(&LogEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            persona_id: self.persona_id.clone(),
            intent: intent.to_string(),
            plan_json: "{}".to_string(),
            signature: "lite-shell".to_string(),
            consent_signature: None,
            status,
        }) {
            Ok(()) => {
                self.validation.log_seen = true;
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sextant")
}

fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().collect();
    let operator_timeout = match parse_operator_timeout(&args) {
        Ok(timeout) => timeout,
        Err(error) => {
            eprintln!("[operator] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };

    match parse_operator_run(&args) {
        Ok(Some(spec)) => run_operator_mode("operator-run", operator_timeout, move || {
            run_operator_script(spec)
        }),
        Ok(None) => {}
        Err(error) => {
            eprintln!("[operator-run] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    }

    if let Some(target) = operator_arg_value(&args, "--operator-probe") {
        run_operator_mode("operator-probe", operator_timeout, move || {
            run_operator_probe(&target)
        });
    }

    if args.iter().any(|arg| arg == "--operator-smoke") {
        run_operator_mode("operator-smoke", operator_timeout, run_operator_smoke);
    }

    if let Err(error) = run_visible_app() {
        eprintln!("[sextant-lite] failed: {error}");
        std::process::exit(1);
    }
}

fn run_operator_mode<F>(label: &'static str, timeout: Duration, run: F) -> !
where
    F: FnOnce() -> Result<Vec<String>, String> + Send + 'static,
{
    let (result_tx, result_rx) = mpsc::channel();
    if let Err(error) = thread::Builder::new()
        .name(format!("sextant-lite-{}", label))
        .spawn(move || {
            let _ = result_tx.send(run());
        })
    {
        eprintln!("[{label}] failed to start worker: {error}");
        std::process::exit(1);
    }

    match result_rx.recv_timeout(timeout) {
        Ok(Ok(report)) => {
            for line in report {
                println!("[{label}] {line}");
            }
            std::process::exit(0);
        }
        Ok(Err(error)) => {
            eprintln!("[{label}] failed: {error}");
            std::process::exit(1);
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            eprintln!(
                "[{label}] timed out after {}s; terminating native-lite operator run",
                timeout.as_secs()
            );
            std::process::exit(124);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            eprintln!("[{label}] worker exited before reporting a result");
            std::process::exit(1);
        }
    }
}

fn run_visible_app() -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|error| format!("event loop initialization failed: {error}"))?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sextant Lite")
            .with_inner_size(PhysicalSize::new(1180, 760))
            .build(&event_loop)
            .map_err(|error| format!("window creation failed: {error}"))?,
    );

    let context = Context::new(window.clone())
        .map_err(|error| format!("softbuffer context initialization failed: {error}"))?;
    let mut surface = Surface::new(&context, window.clone())
        .map_err(|error| format!("softbuffer surface initialization failed: {error}"))?;
    let mut app =
        LiteApp::new().map_err(|error| format!("lite app initialization failed: {error}"))?;
    app.layout(window.inner_size());
    app.update_title(&window);

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(size) => {
                    app.layout(size);
                    window.request_redraw();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    app.cursor = Some((position.x, position.y));
                    app.handle_mouse_move(position.x, position.y);
                    window.request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    app.handle_key(event);
                    app.update_title(&window);
                    window.request_redraw();
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some((x, y)) = app.cursor {
                        if !app.handle_mouse_input(x, y, state) && state == ElementState::Released {
                            app.click(x, y);
                        }
                        app.update_title(&window);
                        window.request_redraw();
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if let Some((x, y)) = app.cursor {
                        app.scroll_at(x, y, &delta, window.inner_size());
                        window.request_redraw();
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Err(error) = draw(&window, &mut surface, &app) {
                        window.set_title(&format!("Sextant Lite - draw failed: {}", error));
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                if app.maybe_refresh_frame() {
                    window.request_redraw();
                }
                if app.main_view == MainView::Browser
                    && (app.latest_frame.is_some() || app.frame_refresh_budget > 0)
                {
                    let delay = if app.frame_dirty {
                        FRAME_REFRESH_DIRTY
                    } else {
                        FRAME_REFRESH_IDLE
                    };
                    elwt.set_control_flow(ControlFlow::WaitUntil(Instant::now() + delay));
                } else {
                    elwt.set_control_flow(ControlFlow::Wait);
                }
            }
            _ => {}
        })
        .map_err(|error| format!("event loop failed: {error}"))
}

fn run_operator_smoke() -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting native-lite operator bridge smoke".to_string());

    let data_dir = env::temp_dir().join(format!("sextant-lite-operator-smoke-{}", Uuid::new_v4()));
    let mut app = LiteApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let sentinel = "native-agent-bridge-online";
    let url = Url::parse(&format!(
        "data:text/html,{}",
        "<!DOCTYPE html>\
<title>Native Operator Smoke</title>\
<main>\
<h1>Native Operator Smoke</h1>\
<input id='q' value='empty'>\
<button id='go' onclick=\"document.getElementById('out').textContent=document.getElementById('q').value\">Go</button>\
<p id='out'>waiting</p>\
</main>"
    ))
    .map_err(|error| format!("operator smoke data URL did not parse: {}", error))?;

    app.navigate_to(url);
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());

    let fill = app
        .engine
        .fill_selector_current_page("#q", sentinel)
        .map_err(|error| format!("native fill interaction failed: {}", error))?;
    if !fill.ok || fill.value != sentinel {
        return Err(format!(
            "native fill interaction returned unexpected result: {:?}",
            fill
        ));
    }
    app.record_log("operator smoke fill #q", LogStatus::Success)?;
    report.push(format!("filled {} with {}", fill.selector, fill.value));

    let click = app
        .engine
        .click_selector_current_page("#go")
        .map_err(|error| format!("native click interaction failed: {}", error))?;
    if !click.ok {
        return Err(format!(
            "native click interaction returned unexpected result: {:?}",
            click
        ));
    }
    app.record_log("operator smoke click #go", LogStatus::Success)?;
    report.push(format!("clicked {} tag {}", click.selector, click.tag));

    app.distill_active();
    if !app.last_ok {
        return Err(app.last_status);
    }
    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.as_ref())
        .ok_or_else(|| "distill did not attach a page to the active tab".to_string())?;
    if !page.content.contains(sentinel) {
        return Err(format!(
            "distilled content did not include sentinel '{}': {}",
            sentinel, page.content
        ));
    }
    report.push(format!("distilled '{}' and found sentinel", page.title));

    if app.wake_results.is_empty() {
        return Err("Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    if app.recent_logs.len() < 3 {
        return Err(format!(
            "Captain's Log returned only {} recent entries after smoke workflow",
            app.recent_logs.len()
        ));
    }
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "Servo frame capture did not return a frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "Servo frame capture returned an empty frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));

    report.push("native-lite operator bridge smoke passed".to_string());
    Ok(report)
}

fn run_operator_script(spec: OperatorRunSpec) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!(
        "starting scripted native-lite run for {}",
        spec.target
    ));

    let data_dir = env::temp_dir().join(format!("sextant-lite-operator-run-{}", Uuid::new_v4()));
    let mut app = LiteApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let url = parse_navigation_target(&spec.target)?;
    app.navigate_to(url);
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());

    let mut distilled = false;
    for step in spec.steps {
        match step {
            OperatorStep::Fill { selector, value } => {
                let result = app
                    .engine
                    .fill_selector_current_page(&selector, &value)
                    .map_err(|error| {
                        format!("native fill interaction failed for {}: {}", selector, error)
                    })?;
                if !result.ok || result.value != value {
                    return Err(format!(
                        "native fill interaction returned unexpected result: {:?}",
                        result
                    ));
                }
                app.record_log(
                    &format!("operator run fill {}", selector),
                    LogStatus::Success,
                )?;
                report.push(format!("filled {} with {}", selector, value));
                distilled = false;
            }
            OperatorStep::Click { selector } => {
                let result =
                    app.engine
                        .click_selector_current_page(&selector)
                        .map_err(|error| {
                            format!(
                                "native click interaction failed for {}: {}",
                                selector, error
                            )
                        })?;
                if !result.ok {
                    return Err(format!(
                        "native click interaction returned unexpected result: {:?}",
                        result
                    ));
                }
                app.record_log(
                    &format!("operator run click {}", selector),
                    LogStatus::Success,
                )?;
                report.push(format!("clicked {} tag {}", selector, result.tag));
                distilled = false;
            }
            OperatorStep::Submit { selector } => {
                let result =
                    app.engine
                        .submit_selector_current_page(&selector)
                        .map_err(|error| {
                            format!(
                                "native submit interaction failed for {}: {}",
                                selector, error
                            )
                        })?;
                if !result.ok {
                    return Err(format!(
                        "native submit interaction returned unexpected result: {:?}",
                        result
                    ));
                }
                app.record_log(
                    &format!("operator run submit {}", selector),
                    LogStatus::Success,
                )?;
                report.push(format!("submitted {} tag {}", selector, result.tag));
                distilled = false;
            }
            OperatorStep::Expect { text } => {
                let page = distill_operator_page(&mut app)?;
                distilled = true;
                let haystack = operator_page_search_text(&page);
                if !haystack.contains(&text) {
                    return Err(format!(
                        "expected text '{}' was not found in distilled page '{}' ({})",
                        text, page.title, page.url
                    ));
                }
                report.push(format!("found expected text '{}'", text));
            }
        }
    }

    if !distilled {
        let page = distill_operator_page(&mut app)?;
        let counts = semantic_counts(&page);
        report.push(format!(
            "distilled '{}' from {} via {} ({} chars, h={} links={} inputs={} images={} text={})",
            page.title,
            page.url,
            distillation_label(&page),
            page.content.chars().count(),
            counts.headings,
            counts.links,
            counts.inputs,
            counts.images,
            counts.text
        ));
    }

    if app.wake_results.is_empty() {
        return Err("scripted run Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "scripted run did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "scripted run captured an empty Servo frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));

    report.push("scripted native-lite run passed".to_string());
    Ok(report)
}

fn run_operator_probe(target: &str) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!("starting native-lite probe for {}", target));

    let data_dir = env::temp_dir().join(format!("sextant-lite-operator-probe-{}", Uuid::new_v4()));
    let mut app = LiteApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let url = parse_navigation_target(target)?;
    app.navigate_to(url.clone());
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());

    app.distill_active();
    if !app.last_ok {
        return Err(app.last_status);
    }

    let tab = app
        .active_tab()
        .ok_or_else(|| "probe finished without an active tab".to_string())?;
    let page = tab
        .distilled_page
        .as_ref()
        .ok_or_else(|| "probe did not attach distilled page data".to_string())?;
    let counts = semantic_counts(page);
    report.push(format!(
        "distilled '{}' from {} via {} ({} chars, h={} links={} inputs={} images={} text={})",
        page.title,
        page.url,
        distillation_label(page),
        page.content.chars().count(),
        counts.headings,
        counts.links,
        counts.inputs,
        counts.images,
        counts.text
    ));

    if app.wake_results.is_empty() {
        return Err("probe Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "probe did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "probe captured an empty Servo frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));

    report.push("native-lite probe passed".to_string());
    Ok(report)
}

fn distill_operator_page(app: &mut LiteApp) -> Result<sextant_engine::DistilledPage, String> {
    app.distill_active();
    if !app.last_ok {
        return Err(app.last_status.clone());
    }
    app.active_tab()
        .and_then(|tab| tab.distilled_page.clone())
        .ok_or_else(|| "operator distill did not attach distilled page data".to_string())
}

fn operator_page_search_text(page: &sextant_engine::DistilledPage) -> String {
    let mut values = vec![
        page.title.clone(),
        page.url.to_string(),
        page.content.clone(),
    ];
    for node in &page.semantic_map {
        values.push(node.text.clone());
        values.push(node.selector.clone());
        for (key, value) in &node.attributes {
            values.push(key.clone());
            values.push(value.clone());
        }
    }
    values.join("\n")
}

fn parse_operator_run(args: &[String]) -> Result<Option<OperatorRunSpec>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--operator-run") else {
        return Ok(None);
    };
    let target = args
        .get(index + 1)
        .ok_or_else(|| "--operator-run requires a target URL or search phrase".to_string())?
        .clone();
    let mut steps = Vec::new();
    let mut cursor = index + 2;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "--fill" => {
                let selector = args
                    .get(cursor + 1)
                    .ok_or_else(|| "--fill requires a selector and value".to_string())?
                    .clone();
                let value = args
                    .get(cursor + 2)
                    .ok_or_else(|| "--fill requires a selector and value".to_string())?
                    .clone();
                steps.push(OperatorStep::Fill { selector, value });
                cursor += 3;
            }
            "--click" => {
                let selector = args
                    .get(cursor + 1)
                    .ok_or_else(|| "--click requires a selector".to_string())?
                    .clone();
                steps.push(OperatorStep::Click { selector });
                cursor += 2;
            }
            "--submit" => {
                let selector = args
                    .get(cursor + 1)
                    .ok_or_else(|| "--submit requires a selector".to_string())?
                    .clone();
                steps.push(OperatorStep::Submit { selector });
                cursor += 2;
            }
            "--expect" => {
                let text = args
                    .get(cursor + 1)
                    .ok_or_else(|| "--expect requires text".to_string())?
                    .clone();
                steps.push(OperatorStep::Expect { text });
                cursor += 2;
            }
            "--operator-timeout" => {
                args.get(cursor + 1)
                    .ok_or_else(|| "--operator-timeout requires seconds".to_string())?;
                cursor += 2;
            }
            other => {
                return Err(format!(
                    "unknown operator-run argument '{}'; expected --fill, --click, --submit, --expect, or --operator-timeout",
                    other
                ));
            }
        }
    }
    Ok(Some(OperatorRunSpec { target, steps }))
}

fn parse_operator_timeout(args: &[String]) -> Result<Duration, String> {
    let Some(value) = operator_arg_value(args, "--operator-timeout") else {
        return Ok(OPERATOR_DEFAULT_TIMEOUT);
    };
    let seconds = value.parse::<u64>().map_err(|error| {
        format!("--operator-timeout expects a positive whole number of seconds: {error}")
    })?;
    if seconds == 0 {
        return Err("--operator-timeout must be greater than zero".to_string());
    }
    Ok(Duration::from_secs(seconds))
}

fn operator_arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn parse_navigation_target(input: &str) -> Result<Url, String> {
    if let Ok(url) = Url::parse(input) {
        return Ok(url);
    }
    if input.contains('.') && !input.contains(' ') {
        return Url::parse(&format!("https://{}", input))
            .map_err(|error| format!("URL parse failed: {}", error));
    }
    let query = url::form_urlencoded::byte_serialize(input.as_bytes()).collect::<String>();
    Url::parse(&format!("https://duckduckgo.com/?q={}", query))
        .map_err(|error| format!("Search URL parse failed: {}", error))
}

fn browser_key_from_winit(key: &NamedKey) -> Option<BrowserKey> {
    match key {
        NamedKey::Enter => Some(BrowserKey::Enter),
        NamedKey::Backspace => Some(BrowserKey::Backspace),
        NamedKey::Tab => Some(BrowserKey::Tab),
        NamedKey::Escape => Some(BrowserKey::Escape),
        NamedKey::ArrowLeft => Some(BrowserKey::ArrowLeft),
        NamedKey::ArrowRight => Some(BrowserKey::ArrowRight),
        NamedKey::ArrowUp => Some(BrowserKey::ArrowUp),
        NamedKey::ArrowDown => Some(BrowserKey::ArrowDown),
        NamedKey::Delete => Some(BrowserKey::Delete),
        _ => None,
    }
}

fn short_url(url: &Url) -> String {
    let value = url.to_string();
    if value.len() > 58 {
        format!("{}...", &value[..55])
    } else {
        value
    }
}

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

fn backend(status: EngineStatus) -> &'static str {
    match status.active_backend {
        EngineBackend::Servo => "Servo",
        EngineBackend::Gecko => "Gecko",
        EngineBackend::Chromium => "Chromium",
    }
}

fn display_backend(app: &LiteApp, tab: &Tab) -> &'static str {
    if cfg!(feature = "servo-backend") && app.latest_frame.is_some() {
        "Servo"
    } else {
        match tab.status.active_backend {
            EngineBackend::Servo => "Servo",
            EngineBackend::Gecko => "Fallback",
            EngineBackend::Chromium => "Legacy",
        }
    }
}

fn render_mode_label() -> &'static str {
    if cfg!(feature = "servo-backend") {
        "LIVE"
    } else {
        "SIM"
    }
}

fn render_action_label() -> &'static str {
    if cfg!(feature = "servo-backend") {
        "Opened"
    } else {
        "Tracked"
    }
}

fn draw(
    window: &Window,
    surface: &mut Surface<Arc<Window>, Arc<Window>>,
    app: &LiteApp,
) -> Result<(), String> {
    let size = window.inner_size();
    let width = size.width.max(1);
    let height = size.height.max(1);
    surface
        .resize(
            NonZeroU32::new(width).ok_or("invalid width")?,
            NonZeroU32::new(height).ok_or("invalid height")?,
        )
        .map_err(|e| e.to_string())?;

    let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
    for pixel in buffer.iter_mut() {
        *pixel = BG;
    }

    draw_top_bar(&mut buffer, width, height, app);
    draw_controls(&mut buffer, width, height, app);
    draw_metric_cards(&mut buffer, width, height, app);
    match app.main_view {
        MainView::Browser => draw_page_panel(&mut buffer, width, height, app),
        MainView::Wake => draw_wake_panel(&mut buffer, width, height, app),
        MainView::Log => draw_log_panel(&mut buffer, width, height, app),
        MainView::Validation => draw_validation_panel(&mut buffer, width, height, app),
    }
    draw_ai_rail(&mut buffer, width, height, app);
    draw_status_bar(&mut buffer, width, height, app);

    buffer.present().map_err(|e| e.to_string())
}

fn draw_top_bar(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let rail_x = right_rail_x(width);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: 0,
            w: width,
            h: CHROME_H,
        },
        PANEL_DARK,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: CHROME_H.saturating_sub(1),
            w: width,
            h: 1,
        },
        BORDER,
    );
    draw_text(buffer, width, height, 24, 14, "SEXTANT", TEXT, 1);
    draw_text(buffer, width, height, 24, 34, "LITE", TEXT_DIM, 1);
    let tabs = app.engine.get_tabs();
    let tab_label = format!("TABS {}", tabs.len());
    draw_text(
        buffer,
        width,
        height,
        rail_x.saturating_sub(94),
        24,
        &tab_label,
        TEXT_DIM,
        1,
    );
    draw_text(buffer, width, height, rail_x + 26, 18, "MAYA", TEXT, 1);
    draw_text(
        buffer,
        width,
        height,
        rail_x + 26,
        36,
        "LOCAL ONLINE",
        STATUS_OK,
        1,
    );
}

fn draw_controls(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let rail_x = right_rail_x(width);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: CHROME_H,
            w: rail_x,
            h: STRIP_H,
        },
        PANEL,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: CHROME_H + STRIP_H - 1,
            w: rail_x,
            h: 1,
        },
        BORDER,
    );
    draw_field(
        buffer,
        width,
        height,
        app.address_rect,
        &app.address_input,
        app.focus == FocusTarget::Address,
    );

    for button in &app.buttons {
        let hovered = app
            .cursor
            .map(|(x, y)| button.rect.contains(x, y))
            .unwrap_or(false);
        draw_button(
            buffer,
            width,
            height,
            button.rect,
            button.label,
            hovered,
            app.action_enabled(button.action),
        );
    }

    draw_pill(
        buffer,
        width,
        height,
        app.browser_tab_rect,
        "BROWSER",
        app.main_view == MainView::Browser,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.wake_tab_rect,
        "WAKE",
        app.main_view == MainView::Wake,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.log_tab_rect,
        "CAPTAIN LOG",
        app.main_view == MainView::Log,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.validation_tab_rect,
        "VALIDATION",
        app.main_view == MainView::Validation,
    );
    draw_text(
        buffer,
        width,
        height,
        rail_x.saturating_sub(184),
        CHROME_H + STRIP_H + 14,
        "AI READY",
        STATUS_OK,
        1,
    );
}

fn draw_metric_cards(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let rail_x = right_rail_x(width);
    let left = 24;
    let gap = 12;
    let card_w = ((rail_x.saturating_sub(left * 2 + gap * 3)) / 4).max(120);
    let tabs = app.engine.get_tabs();
    let wake_count = app.wake_results.len();
    let backend_label = app
        .active_tab()
        .map(|tab| display_backend(app, tab).to_string())
        .unwrap_or_else(|| "NONE".to_string());
    let page_label = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.as_ref())
        .map(|_| "READY".to_string())
        .unwrap_or_else(|| "PENDING".to_string());

    draw_metric_card(
        buffer,
        width,
        height,
        Rect {
            x: left,
            y: METRIC_Y,
            w: card_w,
            h: METRIC_H,
        },
        "TABS",
        &tabs.len().to_string(),
        "ACTIVE SESSION",
        STATUS_OK,
    );
    draw_metric_card(
        buffer,
        width,
        height,
        Rect {
            x: left + (card_w + gap),
            y: METRIC_Y,
            w: card_w,
            h: METRIC_H,
        },
        "ENGINE",
        &backend_label,
        "LOCAL BUILD",
        BUTTON_BRIGHT,
    );
    draw_metric_card(
        buffer,
        width,
        height,
        Rect {
            x: left + (card_w + gap) * 2,
            y: METRIC_Y,
            w: card_w,
            h: METRIC_H,
        },
        "WAKE",
        &wake_count.to_string(),
        "SEARCH HITS",
        if wake_count > 0 { STATUS_OK } else { TEXT_DIM },
    );
    draw_metric_card(
        buffer,
        width,
        height,
        Rect {
            x: left + (card_w + gap) * 3,
            y: METRIC_Y,
            w: card_w,
            h: METRIC_H,
        },
        "PAGE",
        &page_label,
        "DISTILL STATUS",
        if page_label == "READY" {
            STATUS_OK
        } else {
            STATUS_WARN
        },
    );
}

fn draw_page_panel(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let rail_x = right_rail_x(width);
    let page = main_panel_rect(rail_x, height);
    let panel = Rect {
        x: 24,
        y: page.y,
        w: rail_x.saturating_sub(48),
        h: page.h,
    };
    fill_rect(buffer, width, height, panel, PANEL_ALT);
    stroke_rect(buffer, width, height, panel, BORDER);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 20,
        "ACTIVE PAGE",
        TEXT,
        1,
    );

    let status_color = if app.last_ok { STATUS_OK } else { STATUS_WARN };
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 44,
            y: panel.y + 46,
            w: 10,
            h: 10,
        },
        status_color,
    );
    draw_text(
        buffer,
        width,
        height,
        62,
        panel.y + 44,
        &truncate(
            &app.last_status,
            ((panel.w.saturating_sub(62)) / 8) as usize,
        ),
        TEXT,
        1,
    );

    if let Some(tab) = app.active_tab() {
        let url = tab
            .url
            .as_ref()
            .map(short_url)
            .unwrap_or_else(|| "about:blank".to_string());
        let status = format!(
            "TAB {} | BACK {} | FORWARD {} | RENDER {} {}",
            short_id(tab.id),
            tab.can_go_back,
            tab.can_go_forward,
            display_backend(app, tab),
            render_mode_label()
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 76,
            &truncate(&url, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 100,
            &truncate(&status, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
        if let Some(frame) = app.latest_frame.as_ref() {
            draw_rendered_frame(buffer, width, height, panel, frame);
        } else if let Some(page) = tab.distilled_page.as_ref() {
            draw_reader_page(buffer, width, height, panel, page, app.page_scroll);
        } else {
            draw_text(
                buffer,
                width,
                height,
                44,
                panel.y + 134,
                if cfg!(feature = "servo-backend") {
                    "NO SERVO FRAME YET"
                } else {
                    "NO DISTILLED PAGE YET"
                },
                TEXT,
                1,
            );
            draw_text(
                buffer,
                width,
                height,
                44,
                panel.y + 160,
                if cfg!(feature = "servo-backend") {
                    "PRESS GO OR DISTILL TO CAPTURE THE LIVE VIEWPORT."
                } else {
                    "PRESS DISTILL TO FETCH AND PARSE THE CURRENT URL."
                },
                TEXT_DIM,
                1,
            );
        }
    } else {
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 86,
            "NO ACTIVE TAB",
            TEXT_DIM,
            1,
        );
    }
}

fn draw_wake_panel(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    fill_rect(buffer, width, height, panel, PANEL_ALT);
    stroke_rect(buffer, width, height, panel, BORDER);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "DIGITAL WAKE",
        TEXT,
        1,
    );

    if app.wake_results.is_empty() {
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 54,
            "NO WAKE RESULTS YET. DISTILL A PAGE OR ENTER A QUERY IN THE AI RAIL.",
            TEXT_DIM,
            1,
        );
        return;
    }

    let max_rows = panel.h.saturating_sub(56) / 30;
    for (index, entry) in app.wake_results.iter().take(max_rows as usize).enumerate() {
        let y = panel.y + 52 + index as u32 * 30;
        let line = format!("{} | {}", entry.title, short_url(&entry.url));
        draw_text(
            buffer,
            width,
            height,
            44,
            y,
            &truncate(&line, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
        let detail = format!(
            "IMPORTANCE {:.2} | USED {}",
            entry.importance, entry.usage_count
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            y + 14,
            &truncate(&detail, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
    }
}

fn draw_reader_page(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    panel: Rect,
    page: &sextant_engine::DistilledPage,
    scroll: i32,
) {
    let clip = Rect {
        x: panel.x + 18,
        y: panel.y + 126,
        w: panel.w.saturating_sub(36),
        h: panel.h.saturating_sub(142),
    };
    fill_rect(buffer, width, height, clip, PANEL_ALT);
    let max_chars = ((clip.w.saturating_sub(8)) / char_advance(1)) as usize;
    let max_title_chars = ((clip.w.saturating_sub(8)) / char_advance(2)) as usize;
    let mut y = clip.y as i32 - scroll;

    for line in wrap_text(&page.title, max_title_chars).into_iter().take(3) {
        draw_text_clipped(buffer, width, height, clip, clip.x, y, &line, TEXT, 2);
        y += 20;
    }
    y += 8;
    draw_text_clipped(
        buffer,
        width,
        height,
        clip,
        clip.x,
        y,
        &truncate(page.url.as_str(), max_chars),
        TEXT_DIM,
        1,
    );
    y += 28;

    for line in wrap_text(&page.content, max_chars).into_iter().take(24) {
        draw_text_clipped(buffer, width, height, clip, clip.x, y, &line, TEXT_DIM, 1);
        y += 18;
    }

    y += 14;
    draw_text_clipped(
        buffer,
        width,
        height,
        clip,
        clip.x,
        y,
        "SEMANTIC MAP",
        TEXT,
        1,
    );
    y += 24;
    let counts = semantic_counts(page);
    let semantic_line = format!(
        "HEADINGS {} | LINKS {} | INPUTS {} | IMAGES {} | TEXT NODES {}",
        counts.headings, counts.links, counts.inputs, counts.images, counts.text
    );
    draw_text_clipped(
        buffer,
        width,
        height,
        clip,
        clip.x,
        y,
        &truncate(&semantic_line, max_chars),
        TEXT_DIM,
        1,
    );
    y += 24;

    let key_nodes = page
        .semantic_map
        .iter()
        .filter(|node| matches!(node.node_type, NodeType::Heading | NodeType::Link))
        .take(2)
        .map(|node| node.text.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    if !key_nodes.is_empty() {
        draw_text_clipped(
            buffer,
            width,
            height,
            clip,
            clip.x,
            y,
            &truncate(&key_nodes, max_chars),
            TEXT_DIM,
            1,
        );
    }

    if scroll > 0 {
        draw_text(
            buffer,
            width,
            height,
            panel.x + panel.w - 108,
            panel.y + 20,
            "SCROLLED",
            TEXT_DIM,
            1,
        );
    } else {
        draw_text(
            buffer,
            width,
            height,
            panel.x + panel.w - 112,
            panel.y + 20,
            "WHEEL TO READ",
            TEXT_DIM,
            1,
        );
    }
}

fn draw_rendered_frame(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    panel: Rect,
    frame: &RenderedFrame,
) {
    let viewport = browser_viewport_rect(panel);
    fill_rect(buffer, width, height, viewport, FIELD);
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        draw_text(
            buffer,
            width,
            height,
            viewport.x + 12,
            viewport.y + 12,
            "SERVO FRAME IS EMPTY",
            STATUS_WARN,
            1,
        );
        return;
    }

    let scale_x = viewport.w as f32 / frame.width as f32;
    let scale_y = viewport.h as f32 / frame.height as f32;
    let scale = scale_x.min(scale_y).max(0.01);
    let draw_w = (frame.width as f32 * scale).max(1.0) as u32;
    let draw_h = (frame.height as f32 * scale).max(1.0) as u32;
    let offset_x = viewport.x + viewport.w.saturating_sub(draw_w) / 2;
    let offset_y = viewport.y + viewport.h.saturating_sub(draw_h) / 2;

    for dy in 0..draw_h.min(viewport.h) {
        let src_y = ((dy as f32 / scale) as u32).min(frame.height - 1);
        for dx in 0..draw_w.min(viewport.w) {
            let src_x = ((dx as f32 / scale) as u32).min(frame.width - 1);
            let src_idx = src_y as usize * frame.width as usize + src_x as usize;
            if let Some(pixel) = frame.pixels.get(src_idx) {
                let dest_x = offset_x + dx;
                let dest_y = offset_y + dy;
                if dest_x < width && dest_y < height {
                    buffer[dest_y as usize * width as usize + dest_x as usize] = *pixel;
                }
            }
        }
    }

    stroke_rect(buffer, width, height, viewport, BUTTON_ACTIVE);
    let label = format!("SERVO FRAME {}x{}", frame.width, frame.height);
    draw_text(
        buffer,
        width,
        height,
        panel.x + panel.w.saturating_sub(174),
        panel.y + 20,
        &label,
        TEXT_DIM,
        1,
    );
}

fn draw_log_panel(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    fill_rect(buffer, width, height, panel, PANEL_ALT);
    stroke_rect(buffer, width, height, panel, BORDER);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "CAPTAIN'S LOG",
        TEXT,
        1,
    );

    if app.recent_logs.is_empty() {
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 54,
            "NO LOG ENTRIES YET",
            TEXT_DIM,
            1,
        );
        return;
    }

    let max_rows = panel.h.saturating_sub(50) / 22;
    for (index, entry) in app.recent_logs.iter().take(max_rows as usize).enumerate() {
        let y = panel.y + 52 + index as u32 * 22;
        let line = format!("{} | {}", status_label(&entry.status), entry.intent);
        draw_text(
            buffer,
            width,
            height,
            44,
            y,
            &truncate(&line, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
    }
}

fn draw_validation_panel(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    fill_rect(buffer, width, height, panel, PANEL_ALT);
    stroke_rect(buffer, width, height, panel, BORDER);

    let rows = app.validation_rows();
    let pass_count = app.validation_pass_count();
    let summary = format!("{} OF {} CHECKS COMPLETE", pass_count, rows.len());
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "NATIVE-LITE VALIDATION",
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 44,
        &summary,
        if pass_count == rows.len() {
            STATUS_OK
        } else {
            STATUS_WARN
        },
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 70,
        "THIS VIEW TRACKS THE CURRENT WINDOW SESSION, NOT ONLY OPERATOR AUTOMATION.",
        TEXT_DIM,
        1,
    );

    let max_detail_chars = ((panel.w.saturating_sub(260)) / char_advance(1)) as usize;
    let max_rows = panel.h.saturating_sub(116) / 38;
    for (index, row) in rows.iter().take(max_rows as usize).enumerate() {
        let y = panel.y + 112 + index as u32 * 38;
        let color = validation_status_color(row.status);
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: 44,
                y,
                w: 10,
                h: 10,
            },
            color,
        );
        draw_text(
            buffer,
            width,
            height,
            62,
            y - 2,
            validation_status_label(row.status),
            color,
            1,
        );
        draw_text(buffer, width, height, 132, y - 2, row.label, TEXT, 1);
        draw_text(
            buffer,
            width,
            height,
            268,
            y - 2,
            &truncate(&row.detail, max_detail_chars),
            TEXT_DIM,
            1,
        );
    }
}

fn draw_field(buffer: &mut [u32], width: u32, height: u32, rect: Rect, value: &str, focused: bool) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if focused { FIELD_FOCUS } else { FIELD },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if focused { BUTTON_ACTIVE } else { BORDER },
    );
    let visible = truncate(value, ((rect.w.saturating_sub(24)) / 8) as usize);
    draw_text(
        buffer,
        width,
        height,
        rect.x + 12,
        rect.y + 9,
        &visible,
        TEXT,
        1,
    );
    if focused {
        let caret_x = rect.x + 12 + text_width(&visible, 1) + 2;
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: caret_x.min(rect.x + rect.w.saturating_sub(8)),
                y: rect.y + 7,
                w: 2,
                h: 16,
            },
            BUTTON_ACTIVE,
        );
    }
}

fn draw_ai_rail(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let rail_x = right_rail_x(width);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 64,
            w: RAIL_WIDTH,
            h: height.saturating_sub(64 + STATUS_BAR_H),
        },
        PANEL_DARK,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 64,
            w: 1,
            h: height.saturating_sub(64),
        },
        BORDER,
    );
    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        92,
        "SEXTANT AI",
        TEXT,
        2,
    );
    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        118,
        "LOCAL ASSISTANT ONLINE",
        STATUS_OK,
        1,
    );
    let active_url = app
        .active_tab()
        .and_then(|tab| tab.url.as_ref())
        .map(short_url)
        .unwrap_or_else(|| "NO ACTIVE PAGE".to_string());
    let page_state = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.as_ref())
        .map(|page| format!("I HAVE DISTILLED '{}'.", page.title))
        .unwrap_or_else(|| format!("TRACKING {}. DISTILL RUNS REAL HTTP FETCH.", active_url));
    let next_step = if app.active_tab().is_none() {
        "OPEN A PAGE TO START THE BROWSER LOOP."
    } else if app
        .active_tab()
        .and_then(|tab| tab.distilled_page.as_ref())
        .is_none()
    {
        "NEXT: DISTILL THE PAGE INTO WAKE."
    } else if app.wake_results.is_empty() {
        "NEXT: SEARCH WAKE FOR THE DISTILLED PAGE."
    } else {
        "WAKE HAS RESULTS. ASK A QUESTION OR OPEN ANOTHER PAGE."
    };

    draw_chat_bubble(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 20,
            y: 156,
            w: RAIL_WIDTH.saturating_sub(40),
            h: 92,
        },
        &page_state,
        false,
    );
    draw_chat_bubble(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 56,
            y: 270,
            w: RAIL_WIDTH.saturating_sub(76),
            h: 66,
        },
        next_step,
        true,
    );
    draw_chat_bubble(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 20,
            y: 358,
            w: RAIL_WIDTH.saturating_sub(40),
            h: 82,
        },
        &truncate(&app.last_status, 72),
        false,
    );

    draw_text(
        buffer,
        width,
        height,
        app.wake_rect.x,
        app.wake_rect.y.saturating_sub(18),
        "ASK OR SEARCH WAKE",
        TEXT_DIM,
        1,
    );
    draw_field(
        buffer,
        width,
        height,
        app.wake_rect,
        &app.wake_query,
        app.focus == FocusTarget::Wake,
    );
}

fn draw_status_bar(buffer: &mut [u32], width: u32, height: u32, app: &LiteApp) {
    let y = height.saturating_sub(STATUS_BAR_H);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y,
            w: width,
            h: STATUS_BAR_H,
        },
        PANEL_DARK,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y,
            w: width,
            h: 1,
        },
        BORDER,
    );
    let backend = app
        .active_tab()
        .map(|tab| display_backend(app, tab).to_string())
        .unwrap_or_else(|| "NO TAB".to_string());
    let validation_total = app.validation_rows().len();
    let status = format!(
        "RENDER {} {}    DISTILL REAL FETCH    WAKE RESULTS {}    LOG ENTRIES {}    VALIDATION {}/{}    PRIVACY LOCAL",
        backend,
        render_mode_label(),
        app.wake_results.len(),
        app.recent_logs.len(),
        app.validation_pass_count(),
        validation_total
    );
    draw_text(buffer, width, height, 24, y + 14, &status, TEXT_DIM, 1);
    draw_text(
        buffer,
        width,
        height,
        width.saturating_sub(124),
        y + 14,
        "LITE EDGE",
        TEXT_DIM,
        1,
    );
}

fn draw_metric_card(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    value: &str,
    hint: &str,
    accent: u32,
) {
    fill_rect(buffer, width, height, rect, PANEL_ALT);
    stroke_rect(buffer, width, height, rect, BORDER);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: 2,
        },
        accent,
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 18,
        rect.y + 14,
        label,
        TEXT_DIM,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 18,
        rect.y + 34,
        &truncate(value, ((rect.w.saturating_sub(36)) / 13) as usize),
        accent,
        2,
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 18,
        rect.y + 62,
        hint,
        TEXT_DIM,
        1,
    );
}

fn draw_pill(buffer: &mut [u32], width: u32, height: u32, rect: Rect, label: &str, active: bool) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if active { BUTTON_IDLE } else { PANEL_ALT },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if active { BUTTON_BRIGHT } else { BORDER },
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 16,
        rect.y + 10,
        label,
        if active { TEXT } else { TEXT_DIM },
        1,
    );
}

fn draw_chat_bubble(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    text: &str,
    user: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if user { BUTTON_BRIGHT } else { PANEL_ALT },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if user { BUTTON_BRIGHT } else { BORDER },
    );
    let color = if user { FIELD } else { TEXT };
    let max = ((rect.w.saturating_sub(24)) / 8) as usize;
    for (index, line) in wrap_text(text, max).iter().take(4).enumerate() {
        draw_text(
            buffer,
            width,
            height,
            rect.x + 12,
            rect.y + 14 + index as u32 * 18,
            line,
            color,
            1,
        );
    }
}

fn draw_button(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    hovered: bool,
    enabled: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if !enabled {
            BUTTON_DISABLED
        } else if hovered {
            BUTTON_HOVER
        } else {
            BUTTON_IDLE
        },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if enabled { BORDER } else { PANEL },
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 12,
        rect.y + 10,
        label,
        if enabled { TEXT } else { TEXT_DIM },
        1,
    );
}

fn disabled_reason(action: Action) -> &'static str {
    match action {
        Action::Navigate => "Enter an address or search phrase first.",
        Action::NewTab => "New tab is always available.",
        Action::Back => "Back is unavailable for this tab/backend.",
        Action::Forward => "Forward is unavailable for this tab/backend.",
        Action::Reload => "Active tab has no URL to reload.",
        Action::CloseTab => "No active tab to close.",
        Action::DistillActive => "Open a page before distilling.",
        Action::SearchWake => "Enter a Wake query first.",
    }
}

fn right_rail_x(width: u32) -> u32 {
    width.saturating_sub(RAIL_WIDTH)
}

fn main_panel_rect(rail_x: u32, height: u32) -> Rect {
    let top = METRIC_Y + METRIC_H + 14;
    let bottom_limit = height.saturating_sub(STATUS_BAR_H + 12);
    Rect {
        x: 24,
        y: top,
        w: rail_x.saturating_sub(48),
        h: bottom_limit.saturating_sub(top).max(180),
    }
}

fn browser_viewport_rect(panel: Rect) -> Rect {
    Rect {
        x: panel.x + 18,
        y: panel.y + 126,
        w: panel.w.saturating_sub(36),
        h: panel.h.saturating_sub(142),
    }
}

struct SemanticCounts {
    headings: usize,
    links: usize,
    inputs: usize,
    images: usize,
    text: usize,
}

fn semantic_counts(page: &sextant_engine::DistilledPage) -> SemanticCounts {
    let mut counts = SemanticCounts {
        headings: 0,
        links: 0,
        inputs: 0,
        images: 0,
        text: 0,
    };
    for node in &page.semantic_map {
        match node.node_type {
            NodeType::Heading => counts.headings += 1,
            NodeType::Link => counts.links += 1,
            NodeType::Input => counts.inputs += 1,
            NodeType::Image => counts.images += 1,
            NodeType::Text => counts.text += 1,
            NodeType::Button => {}
        }
    }
    counts
}

fn distillation_label(page: &sextant_engine::DistilledPage) -> String {
    page.metadata
        .get("distillation_backend")
        .or_else(|| page.metadata.get("source"))
        .or_else(|| page.metadata.get("distiller"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

fn status_label(status: &LogStatus) -> &'static str {
    match status {
        LogStatus::Success => "OK",
        LogStatus::Failure(_) => "FAIL",
        LogStatus::Aborted => "ABORT",
        LogStatus::AwaitingConsent => "CONSENT",
    }
}

fn validation_status_label(status: ValidationStatus) -> &'static str {
    match status {
        ValidationStatus::Pass => "PASS",
        ValidationStatus::Waiting => "WAIT",
        ValidationStatus::Attention => "CHECK",
    }
}

fn validation_status_color(status: ValidationStatus) -> u32 {
    match status {
        ValidationStatus::Pass => STATUS_OK,
        ValidationStatus::Waiting => TEXT_DIM,
        ValidationStatus::Attention => STATUS_WARN,
    }
}

fn wrap_text(value: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(12);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        if !current.is_empty() && current.len() + word.len() + 1 > max_chars {
            lines.push(current);
            current = String::new();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

fn fill_rect(buffer: &mut [u32], width: u32, height: u32, rect: Rect, color: u32) {
    let max_x = rect.x.saturating_add(rect.w).min(width);
    let max_y = rect.y.saturating_add(rect.h).min(height);
    for y in rect.y.min(height)..max_y {
        let row = y as usize * width as usize;
        for x in rect.x.min(width)..max_x {
            buffer[row + x as usize] = color;
        }
    }
}

fn stroke_rect(buffer: &mut [u32], width: u32, height: u32, rect: Rect, color: u32) {
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
        color,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y.saturating_add(rect.h.saturating_sub(1)),
            w: rect.w,
            h: 1,
        },
        color,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: 1,
            h: rect.h,
        },
        color,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x.saturating_add(rect.w.saturating_sub(1)),
            y: rect.y,
            w: 1,
            h: rect.h,
        },
        color,
    );
}

fn draw_text(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    mut x: u32,
    y: u32,
    text: &str,
    color: u32,
    scale: u32,
) {
    let scale = scale.max(1);
    for ch in text.chars() {
        if ch == '\n' {
            x = 0;
            continue;
        }
        draw_char(buffer, width, height, x, y, ch, color, scale);
        x += char_advance(scale);
    }
}

fn draw_text_clipped(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    clip: Rect,
    mut x: u32,
    y: i32,
    text: &str,
    color: u32,
    scale: u32,
) {
    let scale = scale.max(1);
    for ch in text.chars() {
        draw_char_clipped(buffer, width, height, clip, x, y, ch, color, scale);
        x += char_advance(scale);
        if x >= clip.x.saturating_add(clip.w) {
            break;
        }
    }
}

fn text_width(text: &str, scale: u32) -> u32 {
    let chars = text.chars().count() as u32;
    if chars == 0 {
        0
    } else {
        chars * char_advance(scale)
    }
}

fn char_advance(scale: u32) -> u32 {
    (GLYPH_W + GLYPH_GAP) * scale.max(1)
}

fn draw_char(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    ch: char,
    color: u32,
    scale: u32,
) {
    let glyph = glyph(ch);
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                fill_rect(
                    buffer,
                    width,
                    height,
                    Rect {
                        x: x + col * scale,
                        y: y + row as u32 * scale,
                        w: scale,
                        h: scale,
                    },
                    color,
                );
            }
        }
    }
}

fn draw_char_clipped(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    clip: Rect,
    x: u32,
    y: i32,
    ch: char,
    color: u32,
    scale: u32,
) {
    let glyph = glyph(ch);
    for (row, bits) in glyph.iter().enumerate() {
        let pixel_y = y + row as i32 * scale as i32;
        if pixel_y < clip.y as i32 || pixel_y >= (clip.y + clip.h) as i32 {
            continue;
        }
        for col in 0..5 {
            let pixel_x = x + col * scale;
            if pixel_x < clip.x || pixel_x >= clip.x.saturating_add(clip.w) {
                continue;
            }
            if bits & (1 << (4 - col)) != 0 {
                fill_rect(
                    buffer,
                    width,
                    height,
                    Rect {
                        x: pixel_x,
                        y: pixel_y as u32,
                        w: scale,
                        h: scale,
                    },
                    color,
                );
            }
        }
    }
}

fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0f, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0f],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0f, 0x10, 0x10, 0x13, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0e],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
        ':' => [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x00],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1f],
        '?' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        '&' => [0x0c, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0d],
        '=' => [0x00, 0x00, 0x1f, 0x00, 0x1f, 0x00, 0x00],
        '\'' => [0x0c, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00],
        '"' => [0x0a, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00],
        '|' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        _ => [0x1f, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
    }
}
