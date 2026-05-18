use chrono::Utc;
#[cfg(feature = "xilem-shell")]
use sextant_airgap::SextantAirGap;
use sextant_engine::{
    BrowserKey, EngineBackend, EngineStatus, NodeType, RenderedFrame, SextantEngine, Tab,
};
#[cfg(feature = "xilem-shell")]
use sextant_firewall::{FirewallAction, SextantFirewall};
use sextant_log::{CaptainsLog, LogEntry, LogStatus};
#[cfg(feature = "xilem-shell")]
use sextant_pilot::PilotAction;
#[cfg(feature = "xilem-shell")]
use sextant_privacy::{PrivacyLevel, PrivacyMasker};
#[cfg(feature = "xilem-shell")]
use sextant_vault::CitadelVault;
use sextant_wake::{DigitalWake, WakeEntry};
use softbuffer::{Context, Surface};
use std::env;
use std::num::NonZeroU32;
#[cfg(feature = "xilem-shell")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;
use uuid::Uuid;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
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
const PAGE_TAB_H: u32 = 34;
const PAGE_TAB_PAGER_W: u32 = 26;
const MAX_VISIBLE_PAGE_TABS: usize = 6;
const METRIC_Y: u32 = CHROME_H + STRIP_H + TAB_H + PAGE_TAB_H + 12;
const METRIC_H: u32 = 76;
const GLYPH_W: u32 = 5;
const GLYPH_GAP: u32 = 2;
const FRAME_REFRESH_IDLE: Duration = Duration::from_millis(1500);
const FRAME_REFRESH_DIRTY: Duration = Duration::from_millis(250);
const FRAME_WARMUP_BUDGET: u8 = 6;
const PERF_HISTORY_LIMIT: usize = 24;
const OPERATOR_DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const WINDOW_SMOKE_DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

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

    fn intersects(self, other: Rect) -> bool {
        self.x < other.x.saturating_add(other.w)
            && self.x.saturating_add(self.w) > other.x
            && self.y < other.y.saturating_add(other.h)
            && self.y.saturating_add(self.h) > other.y
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Navigate,
    RunShowcase,
    NewTab,
    Back,
    Forward,
    Reload,
    CloseTab,
    ResetValidation,
    DistillActive,
    SearchWake,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusTarget {
    Address,
    Wake,
    Browser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrowserShortcut {
    FocusAddress,
    NewTab,
    CloseTab,
    Reload,
    Back,
    Forward,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainView {
    Browser,
    Wake,
    Log,
    Guard,
    Perception,
    Perf,
    Validation,
}

#[derive(Default)]
struct ValidationState {
    navigation_seen: bool,
    browser_focus_seen: bool,
    browser_input_seen: bool,
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

struct WindowSmokeSpec {
    target: Option<String>,
    timeout: Duration,
}

#[derive(Clone)]
struct NativeIntentPlan {
    intent: String,
    target: Url,
    should_distill: bool,
    steps: Vec<String>,
}

struct ButtonRegion {
    rect: Rect,
    label: &'static str,
    action: Action,
}

struct PageTabRegion {
    rect: Rect,
    tab_id: Uuid,
}

#[derive(Clone)]
struct PendingConsent {
    intent: String,
    message: String,
    #[cfg(feature = "xilem-shell")]
    remaining_actions: Vec<PilotAction>,
}

impl PendingConsent {
    fn payload(&self) -> String {
        #[cfg(feature = "xilem-shell")]
        {
            let mut payload = format!("intent={}; message={}", self.intent, self.message);
            if !self.remaining_actions.is_empty() {
                let steps = self
                    .remaining_actions
                    .iter()
                    .map(pilot_action_step_label)
                    .collect::<Vec<_>>()
                    .join(" -> ");
                payload.push_str(&format!("; resume={steps}"));
            }
            payload
        }
        #[cfg(not(feature = "xilem-shell"))]
        {
            format!("intent={}; message={}", self.intent, self.message)
        }
    }
}

#[derive(Clone, Default)]
struct BrowserPerf {
    navigation: Option<Duration>,
    distill: Option<Duration>,
    wake: Option<Duration>,
    resize: Option<Duration>,
    frame: Option<Duration>,
}

impl BrowserPerf {
    fn summary(&self) -> String {
        format!(
            "nav {} | distill {} | wake {} | resize {} | frame {}",
            fmt_duration(self.navigation),
            fmt_duration(self.distill),
            fmt_duration(self.wake),
            fmt_duration(self.resize),
            fmt_duration(self.frame)
        )
    }
}

#[derive(Clone)]
struct PerfEvent {
    phase: &'static str,
    label: String,
    duration: Duration,
}

#[derive(Clone, Copy, Default)]
struct DrawStats {
    total: Duration,
    frame_blit: Option<Duration>,
    present: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GuardDecision {
    action: &'static str,
    reason: String,
    allowed: bool,
    network_allowed: bool,
}

impl GuardDecision {
    fn summary(&self) -> String {
        let network = if self.network_allowed {
            "network allowed"
        } else {
            "network blocked"
        };
        format!("{} | {} | {}", self.action, network, self.reason)
    }
}

struct BrowserApp {
    engine: SextantEngine,
    wake: DigitalWake,
    log: CaptainsLog,
    #[cfg(feature = "xilem-shell")]
    consent_vault: CitadelVault,
    #[cfg(feature = "xilem-shell")]
    guard_firewall: SextantFirewall,
    #[cfg(feature = "xilem-shell")]
    guard_policy_source: String,
    persona_id: String,
    address_input: String,
    wake_query: String,
    last_status: String,
    last_ok: bool,
    defer_user_navigation: bool,
    pending_user_navigation: Option<String>,
    pending_user_action: Option<Action>,
    perf: BrowserPerf,
    perf_events: Vec<PerfEvent>,
    last_intent: String,
    pilot_status: String,
    pilot_plan: Vec<String>,
    pilot_result: String,
    pending_consent: Option<PendingConsent>,
    showcase_report: Vec<String>,
    cursor: Option<(f64, f64)>,
    modifiers: ModifiersState,
    focus: FocusTarget,
    address_rect: Rect,
    wake_rect: Rect,
    browser_tab_rect: Rect,
    wake_tab_rect: Rect,
    log_tab_rect: Rect,
    guard_tab_rect: Rect,
    perception_tab_rect: Rect,
    perf_tab_rect: Rect,
    validation_tab_rect: Rect,
    browser_viewport_rect: Rect,
    buttons: Vec<ButtonRegion>,
    page_tab_rects: Vec<PageTabRegion>,
    page_tab_prev_rect: Rect,
    page_tab_next_rect: Rect,
    consent_authorize_rect: Rect,
    consent_deny_rect: Rect,
    page_tab_window_start: usize,
    window_size: PhysicalSize<u32>,
    wake_results: Vec<WakeEntry>,
    recent_logs: Vec<LogEntry>,
    page_scroll: i32,
    main_view: MainView,
    latest_frame: Option<RenderedFrame>,
    last_frame_viewport: Option<(u32, u32)>,
    last_frame_refresh: Instant,
    frame_dirty: bool,
    frame_refresh_budget: u8,
    validation: ValidationState,
}

impl BrowserApp {
    fn new() -> Result<Self, String> {
        Self::new_with_data_dir(app_data_dir().join("browser"))
    }

    fn new_with_data_dir(data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        #[cfg(feature = "xilem-shell")]
        let (guard_firewall, guard_policy_source) = load_browser_guard_firewall(&data_dir)?;
        let mut app = Self {
            engine: SextantEngine::new(),
            wake: DigitalWake::open(data_dir.join("wake.db")).map_err(|e| e.to_string())?,
            log: CaptainsLog::new(data_dir.join("captains-log.db")).map_err(|e| e.to_string())?,
            #[cfg(feature = "xilem-shell")]
            consent_vault: init_browser_consent_vault()?,
            #[cfg(feature = "xilem-shell")]
            guard_firewall,
            #[cfg(feature = "xilem-shell")]
            guard_policy_source,
            persona_id: "browser-persona".to_string(),
            address_input: "https://example.com".to_string(),
            wake_query: "example".to_string(),
            last_status: "Ready. Type a URL or search, then press Enter.".to_string(),
            last_ok: true,
            defer_user_navigation: false,
            pending_user_navigation: None,
            pending_user_action: None,
            perf: BrowserPerf::default(),
            perf_events: Vec::new(),
            last_intent: String::new(),
            pilot_status: "IDLE".to_string(),
            pilot_plan: vec!["Awaiting URL, search, or native intent.".to_string()],
            pilot_result: "No active intent yet.".to_string(),
            pending_consent: None,
            showcase_report: Vec::new(),
            cursor: None,
            modifiers: ModifiersState::default(),
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
            guard_tab_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            perception_tab_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            perf_tab_rect: Rect {
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
            page_tab_rects: Vec::new(),
            page_tab_prev_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            page_tab_next_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            consent_authorize_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            consent_deny_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            page_tab_window_start: 0,
            window_size: PhysicalSize::new(1180, 760),
            wake_results: Vec::new(),
            recent_logs: Vec::new(),
            page_scroll: 0,
            main_view: MainView::Browser,
            latest_frame: None,
            last_frame_viewport: None,
            last_frame_refresh: Instant::now(),
            frame_dirty: false,
            frame_refresh_budget: 0,
            validation: ValidationState::default(),
        };
        app.refresh_logs();
        Ok(app)
    }

    fn update_title(&self, window: &Window) {
        window.set_title(&format!("Sextant Browser - {}", self.last_status));
    }

    fn layout(&mut self, size: PhysicalSize<u32>) {
        self.window_size = size;
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
            w: 96,
            h: 30,
        };
        self.wake_tab_rect = Rect {
            x: 130,
            y: CHROME_H + STRIP_H + 5,
            w: 70,
            h: 30,
        };
        self.log_tab_rect = Rect {
            x: 210,
            y: CHROME_H + STRIP_H + 5,
            w: 70,
            h: 30,
        };
        self.guard_tab_rect = Rect {
            x: 290,
            y: CHROME_H + STRIP_H + 5,
            w: 86,
            h: 30,
        };
        self.perception_tab_rect = Rect {
            x: 386,
            y: CHROME_H + STRIP_H + 5,
            w: 80,
            h: 30,
        };
        self.perf_tab_rect = Rect {
            x: 476,
            y: CHROME_H + STRIP_H + 5,
            w: 64,
            h: 30,
        };
        self.validation_tab_rect = Rect {
            x: 550,
            y: CHROME_H + STRIP_H + 5,
            w: 118,
            h: 30,
        };
        let main_panel = main_panel_rect(rail_x, size.height.max(620));
        self.browser_viewport_rect = browser_viewport_rect(main_panel);
        if self.latest_frame.is_some() {
            self.frame_dirty = true;
        }

        let go_x = self.address_rect.x + self.address_rect.w + gap;
        let mut buttons = vec![
            ButtonRegion {
                rect: Rect {
                    x: go_x,
                    y: 12,
                    w: 78,
                    h: button_h,
                },
                label: "RUN",
                action: Action::Navigate,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin,
                    y: 64,
                    w: 92,
                    h: button_h,
                },
                label: "SHOWCASE",
                action: Action::RunShowcase,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 92 + gap,
                    y: 64,
                    w: 82,
                    h: button_h,
                },
                label: "NEW TAB",
                action: Action::NewTab,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 174 + gap * 2,
                    y: 64,
                    w: 66,
                    h: button_h,
                },
                label: "BACK",
                action: Action::Back,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 240 + gap * 3,
                    y: 64,
                    w: 82,
                    h: button_h,
                },
                label: "FORWARD",
                action: Action::Forward,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 322 + gap * 4,
                    y: 64,
                    w: 76,
                    h: button_h,
                },
                label: "RELOAD",
                action: Action::Reload,
            },
            ButtonRegion {
                rect: Rect {
                    x: margin + 398 + gap * 5,
                    y: 64,
                    w: 88,
                    h: button_h,
                },
                label: "CLOSE TAB",
                action: Action::CloseTab,
            },
        ];
        let reset_rect = Rect {
            x: margin + 486 + gap * 6,
            y: 64,
            w: 112,
            h: button_h,
        };
        if reset_rect.x + reset_rect.w + gap <= main_right.saturating_sub(210) {
            buttons.push(ButtonRegion {
                rect: reset_rect,
                label: "RESET CHECKS",
                action: Action::ResetValidation,
            });
        }
        buttons.extend([
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
        ]);
        self.buttons = buttons;
        self.clamp_page_tab_window();
        let (prev_rect, next_rect) = self.compute_page_tab_pager_rects(main_right);
        self.page_tab_prev_rect = prev_rect;
        self.page_tab_next_rect = next_rect;
        self.consent_authorize_rect = Rect {
            x: rail_x + 20,
            y: 454,
            w: 132,
            h: button_h,
        };
        self.consent_deny_rect = Rect {
            x: rail_x + 168,
            y: 454,
            w: 112,
            h: button_h,
        };
        self.page_tab_rects = self.compute_page_tab_rects(main_right);
    }

    fn page_tab_visible_count(&self) -> usize {
        self.engine.get_tabs().len().min(MAX_VISIBLE_PAGE_TABS)
    }

    fn max_page_tab_window_start(&self) -> usize {
        let tabs_len = self.engine.get_tabs().len();
        tabs_len.saturating_sub(tabs_len.min(MAX_VISIBLE_PAGE_TABS))
    }

    fn clamp_page_tab_window(&mut self) {
        self.page_tab_window_start = self
            .page_tab_window_start
            .min(self.max_page_tab_window_start());
    }

    fn show_page_tab(&mut self, tab_id: Uuid) {
        let tabs = self.engine.get_tabs();
        let visible_count = tabs.len().min(MAX_VISIBLE_PAGE_TABS);
        if visible_count == 0 {
            self.page_tab_window_start = 0;
            return;
        }
        let Some(index) = tabs.iter().position(|tab| tab.id == tab_id) else {
            self.clamp_page_tab_window();
            return;
        };
        if index < self.page_tab_window_start {
            self.page_tab_window_start = index;
        } else if index >= self.page_tab_window_start + visible_count {
            self.page_tab_window_start = index + 1 - visible_count;
        }
        self.clamp_page_tab_window();
    }

    fn page_tab_overflowing(&self) -> bool {
        self.engine.get_tabs().len() > self.page_tab_visible_count()
    }

    fn can_page_tabs_previous(&self) -> bool {
        self.page_tab_overflowing() && self.page_tab_window_start > 0
    }

    fn can_page_tabs_next(&self) -> bool {
        self.page_tab_overflowing() && self.page_tab_window_start < self.max_page_tab_window_start()
    }

    fn compute_page_tab_pager_rects(&self, main_right: u32) -> (Rect, Rect) {
        let top = CHROME_H + STRIP_H + TAB_H + 3;
        (
            Rect {
                x: 24,
                y: top,
                w: PAGE_TAB_PAGER_W,
                h: 27,
            },
            Rect {
                x: main_right.saturating_sub(24 + PAGE_TAB_PAGER_W),
                y: top,
                w: PAGE_TAB_PAGER_W,
                h: 27,
            },
        )
    }

    fn compute_page_tab_rects(&self, main_right: u32) -> Vec<PageTabRegion> {
        let tabs = self.engine.get_tabs();
        if tabs.is_empty() {
            return Vec::new();
        }
        let overflowing = tabs.len() > MAX_VISIBLE_PAGE_TABS;
        let left = if overflowing {
            24 + PAGE_TAB_PAGER_W + 8
        } else {
            24
        };
        let top = CHROME_H + STRIP_H + TAB_H + 3;
        let gap = 8;
        let visible_count = tabs.len().min(MAX_VISIBLE_PAGE_TABS);
        let first_visible = self
            .page_tab_window_start
            .min(tabs.len().saturating_sub(visible_count));
        let pager_space = if overflowing {
            (PAGE_TAB_PAGER_W + gap) * 2
        } else {
            0
        };
        let visible_count_u32 = visible_count as u32;
        let available = main_right.saturating_sub(24 + 24 + pager_space);
        let tab_w = ((available.saturating_sub(gap * visible_count_u32.saturating_sub(1)))
            / visible_count_u32)
            .clamp(112, 210);
        tabs.into_iter()
            .skip(first_visible)
            .take(visible_count)
            .enumerate()
            .map(|(index, tab)| PageTabRegion {
                rect: Rect {
                    x: left + index as u32 * (tab_w + gap),
                    y: top,
                    w: tab_w,
                    h: 27,
                },
                tab_id: tab.id,
            })
            .collect()
    }

    fn page_tabs_previous(&mut self) {
        if !self.can_page_tabs_previous() {
            return;
        }
        self.page_tab_window_start = self.page_tab_window_start.saturating_sub(1);
        self.layout(self.window_size);
        self.last_status = self.page_tab_window_status();
        self.last_ok = true;
        self.validation.tab_control_seen = true;
    }

    fn page_tabs_next(&mut self) {
        if !self.can_page_tabs_next() {
            return;
        }
        self.page_tab_window_start =
            (self.page_tab_window_start + 1).min(self.max_page_tab_window_start());
        self.layout(self.window_size);
        self.last_status = self.page_tab_window_status();
        self.last_ok = true;
        self.validation.tab_control_seen = true;
    }

    fn page_tab_window_status(&self) -> String {
        let total = self.engine.get_tabs().len();
        if total == 0 {
            return "No tabs open.".to_string();
        }
        let visible = self.page_tab_rects.len().max(1);
        let first = self.page_tab_window_start + 1;
        let last = (self.page_tab_window_start + visible).min(total);
        format!("Showing tabs {first}-{last} of {total}.")
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
        if self.guard_tab_rect.contains(x, y) {
            self.main_view = MainView::Guard;
            return;
        }
        if self.perception_tab_rect.contains(x, y) {
            self.main_view = MainView::Perception;
            return;
        }
        if self.perf_tab_rect.contains(x, y) {
            self.main_view = MainView::Perf;
            return;
        }
        if self.validation_tab_rect.contains(x, y) {
            self.main_view = MainView::Validation;
            return;
        }
        if self.can_page_tabs_previous() && self.page_tab_prev_rect.contains(x, y) {
            self.page_tabs_previous();
            return;
        }
        if self.can_page_tabs_next() && self.page_tab_next_rect.contains(x, y) {
            self.page_tabs_next();
            return;
        }
        if self.pending_consent.is_some() && self.consent_authorize_rect.contains(x, y) {
            self.authorize_pilot_consent();
            return;
        }
        if self.pending_consent.is_some() && self.consent_deny_rect.contains(x, y) {
            self.deny_pilot_consent();
            return;
        }
        if let Some(tab_id) = self
            .page_tab_rects
            .iter()
            .find(|tab| tab.rect.contains(x, y))
            .map(|tab| tab.tab_id)
        {
            self.switch_to_page_tab(tab_id);
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

    fn switch_to_page_tab(&mut self, tab_id: Uuid) {
        match self.engine.switch_to_tab(tab_id) {
            Ok(()) => {
                self.show_page_tab(tab_id);
                self.sync_address_to_active_tab();
                self.main_view = MainView::Browser;
                self.page_scroll = 0;
                self.begin_frame_warmup();
                self.last_frame_viewport = None;
                self.refresh_frame();
                self.layout(self.window_size);
                self.last_status = format!("Switched to tab {}.", short_id(tab_id));
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log(&format!("switch tab {}", tab_id), LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Tab switch failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ =
                    self.record_log(&format!("switch tab {}", tab_id), LogStatus::Failure(error));
            }
        }
    }

    fn handle_key(&mut self, event: KeyEvent) {
        if event.state == ElementState::Pressed && self.handle_shortcut(&event.logical_key) {
            return;
        }

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

    fn handle_shortcut(&mut self, key: &Key) -> bool {
        let Some(shortcut) = browser_shortcut_for_key(self.modifiers, key) else {
            return false;
        };
        match shortcut {
            BrowserShortcut::FocusAddress => {
                self.focus = FocusTarget::Address;
                self.main_view = MainView::Browser;
                self.last_status = "Address bar focused.".to_string();
                self.last_ok = true;
            }
            BrowserShortcut::NewTab => self.run_action(Action::NewTab),
            BrowserShortcut::CloseTab => self.run_action(Action::CloseTab),
            BrowserShortcut::Reload => self.run_shortcut_action(Action::Reload),
            BrowserShortcut::Back => self.run_shortcut_action(Action::Back),
            BrowserShortcut::Forward => self.run_shortcut_action(Action::Forward),
        }
        true
    }

    fn run_shortcut_action(&mut self, action: Action) {
        if self.action_enabled(action) {
            self.run_action(action);
        } else {
            self.last_status = disabled_reason(action).to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
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
        let moved = self
            .engine
            .enqueue_mouse_move_current_viewport(local_x, local_y)
            .is_ok();
        let clicked = self
            .engine
            .enqueue_mouse_button_current_viewport(local_x, local_y, state == ElementState::Pressed)
            .is_ok();
        self.validation.browser_input_seen |= moved || clicked;
        self.frame_dirty = true;
        true
    }

    fn handle_mouse_move(&mut self, x: f64, y: f64) {
        if self.focus != FocusTarget::Browser || !self.browser_viewport_rect.contains(x, y) {
            return;
        }
        if let Some((local_x, local_y)) = self.browser_point(x, y) {
            if self
                .engine
                .enqueue_mouse_move_current_viewport(local_x, local_y)
                .is_ok()
            {
                self.validation.browser_input_seen = true;
            }
        }
    }

    fn forward_browser_key(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        match &event.logical_key {
            Key::Character(value) if value.chars().all(|c| !c.is_control()) => {
                if self
                    .engine
                    .enqueue_key_character_current_viewport(value.to_string(), pressed)
                    .is_ok()
                {
                    self.validation.browser_input_seen = true;
                }
                self.frame_dirty = true;
            }
            Key::Named(NamedKey::Space) => {
                if self
                    .engine
                    .enqueue_key_character_current_viewport(" ".to_string(), pressed)
                    .is_ok()
                {
                    self.validation.browser_input_seen = true;
                }
                self.frame_dirty = true;
            }
            Key::Named(named) => {
                if let Some(key) = browser_key_from_winit(named) {
                    if self
                        .engine
                        .enqueue_key_named_current_viewport(key, pressed)
                        .is_ok()
                    {
                        self.validation.browser_input_seen = true;
                    }
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
        let rail_x = right_rail_x(size.width.max(900));
        let tab_strip = Rect {
            x: 0,
            y: CHROME_H + STRIP_H + TAB_H,
            w: rail_x,
            h: PAGE_TAB_H,
        };
        if tab_strip.contains(x, y) {
            match delta {
                MouseScrollDelta::LineDelta(_, dy) if *dy > 0.0 => self.page_tabs_previous(),
                MouseScrollDelta::LineDelta(_, dy) if *dy < 0.0 => self.page_tabs_next(),
                MouseScrollDelta::PixelDelta(position) if position.y > 0.0 => {
                    self.page_tabs_previous()
                }
                MouseScrollDelta::PixelDelta(position) if position.y < 0.0 => self.page_tabs_next(),
                _ => {}
            }
            return;
        }
        if self.main_view != MainView::Browser {
            return;
        }
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
            self.validation.browser_input_seen = true;
            self.begin_frame_warmup();
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
        if self.defer_user_navigation && is_deferred_user_navigation_action(action) {
            self.pending_user_action = Some(action);
            self.last_status = deferred_user_navigation_status(action).to_string();
            self.last_ok = true;
            self.main_view = MainView::Browser;
            return;
        }

        match action {
            Action::Navigate => self.navigate_input(),
            Action::RunShowcase => self.run_showcase_visible(),
            Action::NewTab => self.new_tab(),
            Action::Back => self.back(),
            Action::Forward => self.forward(),
            Action::Reload => self.reload(),
            Action::CloseTab => self.close_tab(),
            Action::ResetValidation => self.reset_validation(),
            Action::DistillActive => self.distill_active(),
            Action::SearchWake => self.search_wake(),
        }
        self.refresh_logs();
    }

    fn reset_validation(&mut self) {
        self.validation = ValidationState::default();
        self.last_status = "Validation session reset. Runtime state is unchanged.".to_string();
        self.last_ok = true;
    }

    fn run_showcase_visible(&mut self) {
        self.last_status = "Running launch showcase workflow.".to_string();
        self.last_ok = true;
        match run_showcase_workflow(self) {
            Ok(report) => {
                self.last_status =
                    "Showcase complete: Intent, Wake, Log, interaction, and Servo frame are ready."
                        .to_string();
                self.pilot_status = "SHOWCASE READY".to_string();
                self.pilot_result = report
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "Sextant launch showcase run passed".to_string());
                self.pilot_plan = report
                    .iter()
                    .cloned()
                    .filter(|line| !line.starts_with("isolated data dir"))
                    .take(4)
                    .collect();
                self.showcase_report = report;
                self.last_ok = true;
                self.main_view = MainView::Validation;
                self.refresh_logs();
            }
            Err(error) => {
                self.last_status = format!("Showcase failed: {}", error);
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = error;
                self.showcase_report = vec![self.last_status.clone()];
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    fn run_real_browsing_visible(&mut self) {
        self.last_status = "Running real browsing workflow.".to_string();
        self.last_ok = true;
        match run_real_browsing_workflow(self) {
            Ok(report) => {
                self.last_status =
                    "Real browsing complete: Google, docs, history, Wake, Log, and Servo frame are ready."
                        .to_string();
                self.pilot_status = "BROWSING READY".to_string();
                self.pilot_result = report
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "real browsing smoke suite passed".to_string());
                self.pilot_plan = report
                    .iter()
                    .cloned()
                    .filter(|line| !line.starts_with("isolated data dir"))
                    .take(4)
                    .collect();
                self.showcase_report = report;
                self.last_ok = true;
                self.main_view = MainView::Validation;
                self.refresh_logs();
            }
            Err(error) => {
                self.last_status = format!("Real browsing failed: {}", error);
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = error;
                self.showcase_report = vec![self.last_status.clone()];
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    fn run_shell_interaction_visible(&mut self) {
        self.last_status = "Running shell interaction smoke.".to_string();
        self.last_ok = true;
        match run_shell_interaction_workflow(self) {
            Ok(report) => {
                self.last_status =
                    "Shell interaction complete: chrome clicks, tabs, Sense, Perf, viewport focus, Wake, Log, and frame are ready."
                        .to_string();
                self.pilot_status = "SHELL READY".to_string();
                self.pilot_result = report
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "visible shell interaction smoke passed".to_string());
                self.pilot_plan = report.iter().cloned().take(4).collect();
                self.showcase_report = report;
                self.last_ok = true;
                self.main_view = MainView::Validation;
                self.refresh_logs();
            }
            Err(error) => {
                self.last_status = format!("Shell interaction failed: {}", error);
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = error;
                self.showcase_report = vec![self.last_status.clone()];
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    fn navigate_input(&mut self) {
        let raw = self.address_input.trim().to_string();
        if raw.is_empty() {
            self.last_status = "Enter a URL, search phrase, or intent first.".to_string();
            self.last_ok = false;
            return;
        }

        if self.defer_user_navigation {
            self.pending_user_navigation = Some(raw.clone());
            self.last_status = format!("Opening {}...", truncate(&raw, 80));
            self.last_ok = true;
            self.main_view = MainView::Browser;
            return;
        }

        if native_intent_body(&raw).is_some() {
            self.run_native_intent(&raw);
            return;
        }

        match parse_navigation_target(&raw) {
            Ok(url) => self.navigate_to(url),
            Err(error) => {
                self.last_status = error;
                self.last_ok = false;
            }
        }
    }

    fn run_native_intent(&mut self, raw: &str) {
        let Some(intent) = native_intent_body(raw) else {
            self.last_status = "That input did not resolve to a native intent.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        };

        self.last_intent = intent.clone();
        self.pilot_status = "PLANNING".to_string();
        self.pilot_result = "Planning native browser work.".to_string();
        self.pilot_plan = vec![
            "Read intent".to_string(),
            "Resolve navigation target".to_string(),
        ];
        self.pending_consent = None;

        #[cfg(feature = "xilem-shell")]
        {
            self.run_pilot_action_intent(&intent);
        }

        #[cfg(not(feature = "xilem-shell"))]
        {
            if intent_needs_consent(&intent) {
                self.pilot_status = "AWAITING CONSENT".to_string();
                self.pilot_plan
                    .push("Hold before performing sensitive action".to_string());
                self.pilot_result =
                    "Sensitive intent paused. Consent UX will own this path next.".to_string();
                self.pending_consent = Some(PendingConsent {
                    intent: intent.clone(),
                    message: self.pilot_result.clone(),
                    #[cfg(feature = "xilem-shell")]
                    remaining_actions: Vec::new(),
                });
                self.last_status = format!("Intent paused for consent: {}", truncate(&intent, 56));
                self.last_ok = true;
                let _ = self.record_log(
                    &format!("native intent {}", intent),
                    LogStatus::AwaitingConsent,
                );
                return;
            }

            let plan = match plan_native_intent(&intent) {
                Ok(plan) => plan,
                Err(error) => {
                    self.pilot_status = "FAILED".to_string();
                    self.pilot_result = error.clone();
                    self.last_status = error.clone();
                    self.last_ok = false;
                    self.validation.error_seen = true;
                    let _ = self.record_log(
                        &format!("native intent {}", intent),
                        LogStatus::Failure(error),
                    );
                    return;
                }
            };

            self.pilot_plan = plan.steps.clone();
            self.pilot_status = "NAVIGATING".to_string();
            self.navigate_to(plan.target.clone());
            if !self.last_ok {
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = self.last_status.clone();
                let _ = self.record_log(
                    &format!("native intent {}", plan.intent),
                    LogStatus::Failure(self.last_status.clone()),
                );
                return;
            }

            if plan.should_distill {
                self.pilot_status = "DISTILLING".to_string();
                self.distill_active();
                if !self.last_ok {
                    self.pilot_status = "FAILED".to_string();
                    self.pilot_result = self.last_status.clone();
                    let _ = self.record_log(
                        &format!("native intent {}", plan.intent),
                        LogStatus::Failure(self.last_status.clone()),
                    );
                    return;
                }
            }

            self.pilot_status = "COMPLETE".to_string();
            self.pilot_result = if plan.should_distill {
                format!(
                    "Opened {} and stored the distilled page in Wake.",
                    short_url(&plan.target)
                )
            } else {
                format!("Opened {}.", short_url(&plan.target))
            };
            self.last_status = format!("Intent complete: {}", truncate(&plan.intent, 58));
            self.last_ok = true;
            let _ = self.record_log(
                &format!("native intent {}", plan.intent),
                LogStatus::Success,
            );
        }
    }

    #[cfg(feature = "xilem-shell")]
    fn run_pilot_action_intent(&mut self, intent: &str) {
        self.pilot_status = "PILOT PLANNING".to_string();
        self.pilot_result = "Planning explicit browser work through Pilot actions.".to_string();

        let actions = match pilot_action_plan(intent) {
            Ok(actions) => actions,
            Err(error) => {
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = error.clone();
                self.last_status = error.clone();
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(
                    &format!("pilot intent {}", intent),
                    LogStatus::Failure(error),
                );
                return;
            }
        };

        self.pilot_plan = actions.iter().map(pilot_action_step_label).collect();
        let mut analysis = Vec::new();
        if self.execute_pilot_actions(intent, actions, &mut analysis) {
            self.finish_successful_pilot_intent(intent, analysis);
        }
    }

    #[cfg(feature = "xilem-shell")]
    fn execute_pilot_actions(
        &mut self,
        intent: &str,
        actions: Vec<PilotAction>,
        analysis: &mut Vec<String>,
    ) -> bool {
        for (index, action) in actions.iter().cloned().enumerate() {
            match action {
                PilotAction::Navigate(url) => {
                    self.pilot_status = "PILOT NAVIGATING".to_string();
                    self.navigate_to(url);
                    if !self.last_ok {
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                }
                PilotAction::OpenTab(url) => {
                    self.pilot_status = "PILOT OPENING TAB".to_string();
                    self.new_tab();
                    self.navigate_to(url);
                    if !self.last_ok {
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                }
                PilotAction::Distill => {
                    self.pilot_status = "PILOT DISTILLING".to_string();
                    self.distill_active();
                    if !self.last_ok {
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                }
                PilotAction::Perceive => {
                    self.pilot_status = "PILOT PERCEIVING".to_string();
                    self.distill_active();
                    if !self.last_ok {
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                    if let Some(page) = self
                        .active_tab()
                        .and_then(|tab| tab.distilled_page.as_ref())
                    {
                        analysis.push(page_perception_summary(page));
                        self.main_view = MainView::Perception;
                    }
                }
                PilotAction::PerceiveMultiModal => {
                    self.pilot_status = "PILOT PERCEIVING".to_string();
                    analysis.push(
                        "Multimodal perception is queued for the Neural Bridge lane.".to_string(),
                    );
                }
                PilotAction::Analyze(message) => {
                    analysis.push(message);
                }
                PilotAction::RequestConsent(message) => {
                    let remaining_actions = actions[index + 1..].to_vec();
                    self.pilot_status = "AWAITING CONSENT".to_string();
                    self.pilot_result = message.clone();
                    self.pending_consent = Some(PendingConsent {
                        intent: intent.to_string(),
                        message,
                        remaining_actions,
                    });
                    self.last_status = format!("Pilot awaiting consent: {}", truncate(intent, 56));
                    self.last_ok = true;
                    let _ = self.record_log(
                        &format!("pilot intent {}", intent),
                        LogStatus::AwaitingConsent,
                    );
                    return false;
                }
                PilotAction::SwitchTab(tab_id) => match self.engine.switch_to_tab(tab_id) {
                    Ok(()) => {
                        self.show_page_tab(tab_id);
                        self.sync_address_to_active_tab();
                        self.refresh_frame();
                    }
                    Err(error) => {
                        self.last_status = format!("Pilot tab switch failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                },
                PilotAction::CloseTab(tab_id) => match self.engine.switch_to_tab(tab_id) {
                    Ok(()) => {
                        self.close_tab();
                        if !self.last_ok {
                            self.finish_failed_pilot_intent(intent);
                            return false;
                        }
                    }
                    Err(error) => {
                        self.last_status = format!("Pilot tab close failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                },
            }
        }
        true
    }

    #[cfg(feature = "xilem-shell")]
    fn finish_successful_pilot_intent(&mut self, intent: &str, analysis: Vec<String>) {
        self.pilot_status = "PILOT COMPLETE".to_string();
        self.pilot_result = analysis
            .last()
            .cloned()
            .unwrap_or_else(|| "Pilot actions completed.".to_string());
        self.last_status = format!("Pilot intent complete: {}", truncate(intent, 58));
        self.last_ok = true;
        let _ = self.record_log(&format!("pilot intent {}", intent), LogStatus::Success);
    }

    #[cfg(feature = "xilem-shell")]
    fn finish_failed_pilot_intent(&mut self, intent: &str) {
        self.pilot_status = "FAILED".to_string();
        self.pilot_result = self.last_status.clone();
        let _ = self.record_log(
            &format!("pilot intent {}", intent),
            LogStatus::Failure(self.last_status.clone()),
        );
    }

    fn authorize_pilot_consent(&mut self) {
        let Some(pending) = self.pending_consent.clone() else {
            self.last_status = "No Pilot consent request is pending.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        };

        let signature = match self.sign_pending_consent(&pending) {
            Ok(signature) => signature,
            Err(error) => {
                self.pilot_status = "CONSENT FAILED".to_string();
                self.pilot_result = error.clone();
                self.last_status = format!("Captain's Key authorization failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(
                    &format!("pilot consent authorize {}", pending.intent),
                    LogStatus::Failure(error),
                );
                return;
            }
        };

        self.pending_consent = None;
        self.pilot_status = "CONSENT AUTHORIZED".to_string();
        self.pilot_result = format!(
            "Captain's Key authorized. Signature {}.",
            truncate(&signature, 18)
        );
        self.last_status = format!(
            "Authorized Pilot consent for: {}",
            truncate(&pending.intent, 52)
        );
        self.last_ok = true;
        let _ = self.record_log_with_consent(
            &format!("pilot consent authorize {}", pending.intent),
            LogStatus::Success,
            Some(signature.clone()),
        );
        self.refresh_logs();

        #[cfg(feature = "xilem-shell")]
        {
            if !pending.remaining_actions.is_empty() {
                self.pilot_status = "CONSENT RESUMING".to_string();
                self.pilot_result =
                    "Captain's Key authorized. Resuming gated browser plan.".to_string();
                self.pilot_plan = pending
                    .remaining_actions
                    .iter()
                    .map(pilot_action_step_label)
                    .collect();
                let mut analysis = vec![format!(
                    "Captain's Key Authorized with signature {}.",
                    truncate(&signature, 18)
                )];
                if self.execute_pilot_actions(
                    &pending.intent,
                    pending.remaining_actions.clone(),
                    &mut analysis,
                ) {
                    self.finish_successful_pilot_intent(&pending.intent, analysis);
                    let _ = self.record_log_with_consent(
                        &format!("pilot consent resume {}", pending.intent),
                        LogStatus::Success,
                        Some(signature),
                    );
                }
                self.refresh_logs();
            }
        }
    }

    fn deny_pilot_consent(&mut self) {
        let Some(pending) = self.pending_consent.take() else {
            self.last_status = "No Pilot consent request is pending.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        };

        self.pilot_status = "CONSENT DENIED".to_string();
        self.pilot_result = format!("Captain's Key denied: {}", truncate(&pending.message, 72));
        self.last_status = format!(
            "Denied Pilot consent for: {}",
            truncate(&pending.intent, 52)
        );
        self.last_ok = true;
        let _ = self.record_log(
            &format!("pilot consent deny {}", pending.intent),
            LogStatus::Aborted,
        );
        self.refresh_logs();
    }

    fn sign_pending_consent(&self, pending: &PendingConsent) -> Result<String, String> {
        #[cfg(feature = "xilem-shell")]
        {
            self.consent_vault.sign_consent(&pending.payload())
        }
        #[cfg(not(feature = "xilem-shell"))]
        {
            Ok(format!(
                "reader-consent-{}-{}",
                Uuid::new_v4(),
                truncate(&pending.payload(), 12)
            ))
        }
    }

    fn navigate_to(&mut self, url: Url) {
        let started = Instant::now();
        let guard = match self.check_navigation_guard(&url, "navigate") {
            Ok(guard) => guard,
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "guard blocked");
                self.last_status = error;
                self.last_ok = false;
                self.validation.error_seen = true;
                return;
            }
        };

        match self
            .engine
            .navigate_with_fallback(url.clone(), &self.persona_id)
        {
            Ok(status) => {
                let elapsed = started.elapsed();
                self.record_perf("navigation", elapsed, "open");
                self.perf.frame = None;
                self.address_input = url.to_string();
                self.page_scroll = 0;
                self.validation.navigation_seen = true;
                self.begin_frame_warmup();
                let guard_note = if guard.action == "ALLOW" {
                    String::new()
                } else {
                    format!(" Guard {}.", guard.action)
                };
                self.last_status = format!(
                    "{} {} with {} in {}. Frame capture queued.{}",
                    render_action_label(),
                    short_url(&url),
                    backend(status),
                    format_duration(elapsed),
                    guard_note
                );
                self.last_ok = true;
                let _ = self.record_log(&format!("navigate {}", url), LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "open failed");
                self.last_status = format!("Open failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(&format!("navigate {}", url), LogStatus::Failure(error));
            }
        }
    }

    fn guard_decision(&self, url: &Url) -> GuardDecision {
        #[cfg(feature = "xilem-shell")]
        {
            let airgap = SextantAirGap::new();
            let network_required = matches!(url.scheme(), "http" | "https");
            let network_allowed = !network_required || airgap.check_network_allowed();
            if !network_allowed {
                return GuardDecision {
                    action: "BLOCK",
                    reason: format!(
                        "{:?} air-gap blocks network navigation",
                        airgap.get_status()
                    ),
                    allowed: false,
                    network_allowed,
                };
            }

            let (action, reason) = self.guard_firewall.check_access(&self.persona_id, url);
            let allowed = !matches!(action, FirewallAction::Block);
            GuardDecision {
                action: firewall_action_label(&action),
                reason,
                allowed,
                network_allowed,
            }
        }

        #[cfg(not(feature = "xilem-shell"))]
        {
            let _ = url;
            GuardDecision {
                action: "OBSERVE",
                reason: "reader lane guard crates disabled".to_string(),
                allowed: true,
                network_allowed: true,
            }
        }
    }

    fn check_navigation_guard(
        &mut self,
        url: &Url,
        activity: &str,
    ) -> Result<GuardDecision, String> {
        let guard = self.guard_decision(url);
        if guard.allowed {
            if guard.action != "ALLOW" && guard.action != "OBSERVE" {
                let _ = self.record_log(&format!("guard {activity} {}", url), LogStatus::Success);
            }
            return Ok(guard);
        }

        let message = format!(
            "Local guard blocked {activity} to {}: {}",
            short_url(url),
            guard.summary()
        );
        let _ = self.record_log(
            &format!("guard {activity} {}", url),
            LogStatus::Failure(message.clone()),
        );
        Err(message)
    }

    fn check_interaction_guard(
        &mut self,
        result: &sextant_engine::BrowserInteractionResult,
        activity: &str,
    ) -> Result<(), String> {
        let Some(url) = result.current_url.as_ref() else {
            return Ok(());
        };
        let guard = self.guard_decision(url);
        if guard.allowed {
            return Ok(());
        }

        let message = format!(
            "Local guard blocked {activity} result at {}: {}",
            short_url(url),
            guard.summary()
        );
        self.last_status = message.clone();
        self.last_ok = false;
        self.validation.error_seen = true;
        let _ = self.record_log(
            &format!("guard {activity} {}", url),
            LogStatus::Failure(message.clone()),
        );
        Err(message)
    }

    fn fill_selector_current_page(
        &mut self,
        selector: &str,
        value: &str,
    ) -> Result<sextant_engine::BrowserInteractionResult, String> {
        let result = self
            .engine
            .fill_selector_current_page(selector, value)
            .map_err(|error| {
                format!("native fill interaction failed for {}: {}", selector, error)
            })?;
        self.check_interaction_guard(&result, "fill")?;
        Ok(result)
    }

    fn click_selector_current_page(
        &mut self,
        selector: &str,
    ) -> Result<sextant_engine::BrowserInteractionResult, String> {
        let result = self
            .engine
            .click_selector_current_page(selector)
            .map_err(|error| {
                format!(
                    "native click interaction failed for {}: {}",
                    selector, error
                )
            })?;
        self.check_interaction_guard(&result, "click")?;
        Ok(result)
    }

    fn submit_selector_current_page(
        &mut self,
        selector: &str,
    ) -> Result<sextant_engine::BrowserInteractionResult, String> {
        let result = self
            .engine
            .submit_selector_current_page(selector)
            .map_err(|error| {
                format!(
                    "native submit interaction failed for {}: {}",
                    selector, error
                )
            })?;
        self.check_interaction_guard(&result, "submit")?;
        Ok(result)
    }

    fn new_tab(&mut self) {
        let id = self.engine.open_tab();
        self.show_page_tab(id);
        self.address_input = "about:blank".to_string();
        self.latest_frame = None;
        self.last_frame_viewport = None;
        self.last_status = format!("Created tab {}.", short_id(id));
        self.last_ok = true;
        self.validation.tab_control_seen = true;
        self.layout(self.window_size);
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
                if let Some(active_id) = self.active_tab().map(|tab| tab.id) {
                    self.show_page_tab(active_id);
                } else {
                    self.clamp_page_tab_window();
                }
                self.latest_frame = None;
                self.last_frame_viewport = None;
                self.last_status = format!("Closed tab {}.", short_id(tab.id));
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                self.layout(self.window_size);
                self.refresh_frame();
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
        let started = Instant::now();
        match self.engine.reload_active_tab() {
            Ok(status) => {
                let elapsed = started.elapsed();
                self.record_perf("navigation", elapsed, "reload");
                self.perf.frame = None;
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.last_status = format!(
                    "Reloaded active tab with {} in {}. Frame capture queued.",
                    backend(status),
                    format_duration(elapsed)
                );
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("reload", LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "reload failed");
                self.last_status = format!("Reload failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("reload", LogStatus::Failure(error));
            }
        }
    }

    fn back(&mut self) {
        let started = Instant::now();
        match self.engine.go_back_active_tab() {
            Ok(status) => {
                let elapsed = started.elapsed();
                self.record_perf("navigation", elapsed, "back");
                self.perf.frame = None;
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.last_status = format!(
                    "Went back with {} in {}. Frame capture queued.",
                    backend(status),
                    format_duration(elapsed)
                );
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("back", LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "back failed");
                self.last_status = format!("Back unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("back", LogStatus::Failure(error));
            }
        }
    }

    fn forward(&mut self) {
        let started = Instant::now();
        match self.engine.go_forward_active_tab() {
            Ok(status) => {
                let elapsed = started.elapsed();
                self.record_perf("navigation", elapsed, "forward");
                self.perf.frame = None;
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.last_status = format!(
                    "Went forward with {} in {}. Frame capture queued.",
                    backend(status),
                    format_duration(elapsed)
                );
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("forward", LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "forward failed");
                self.last_status = format!("Forward unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("forward", LogStatus::Failure(error));
            }
        }
    }

    fn distill_active(&mut self) {
        let started = Instant::now();
        match self.engine.distill_current_page() {
            Ok(page) => {
                let distill_elapsed = started.elapsed();
                self.record_perf("distill", distill_elapsed, "active tab");
                let wake_started = Instant::now();
                match self.wake.record(&self.persona_id, &page, None) {
                    Ok(()) => {
                        self.record_perf("wake", wake_started.elapsed(), "record page");
                        self.page_scroll = 0;
                        self.begin_frame_warmup();
                        self.wake_query = page.title.clone();
                        let title = page.title.clone();
                        self.last_status = format!(
                            "Distilled '{}' into Wake in {}.",
                            title,
                            format_duration(distill_elapsed)
                        );
                        self.last_ok = true;
                        self.validation.distill_seen = true;
                        let _ = self.record_log("distill active", LogStatus::Success);
                        self.search_wake();
                        if self.last_ok {
                            self.last_status = format!(
                                "Distilled '{}' into Wake. {}.",
                                title,
                                self.perf.summary()
                            );
                        }
                    }
                    Err(error) => {
                        self.record_perf("wake", wake_started.elapsed(), "record failed");
                        self.last_status = format!("Wake record failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                        let _ = self
                            .record_log("distill active", LogStatus::Failure(error.to_string()));
                    }
                }
            }
            Err(error) => {
                self.record_perf("distill", started.elapsed(), "active tab failed");
                self.last_status = format!("Distill failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("distill active", LogStatus::Failure(error));
            }
        }
    }

    fn search_wake(&mut self) {
        let query = self.wake_query.trim().to_string();
        let started = Instant::now();
        match self.wake.search(&self.persona_id, &query) {
            Ok(results) => {
                let elapsed = started.elapsed();
                self.record_perf("wake", elapsed, "search");
                self.last_status = format!(
                    "Wake search found {} result(s) in {}.",
                    results.len(),
                    format_duration(elapsed)
                );
                self.last_ok = true;
                self.validation.wake_seen = !results.is_empty();
                self.wake_results = results;
                let _ = self.record_log(&format!("search Wake '{}'", query), LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("wake", started.elapsed(), "search failed");
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
                status: if self.validation.browser_input_seen {
                    ValidationStatus::Pass
                } else if self.validation.browser_focus_seen {
                    ValidationStatus::Attention
                } else {
                    ValidationStatus::Waiting
                },
                detail: if self.validation.browser_input_seen {
                    "Mouse, key, or wheel input was forwarded to the viewport".to_string()
                } else if self.validation.browser_focus_seen {
                    "Viewport focused; type, click, or scroll to forward input".to_string()
                } else {
                    "Click the live viewport, then type or scroll".to_string()
                },
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

    fn record_perf(&mut self, phase: &'static str, duration: Duration, label: &str) {
        match phase {
            "navigation" => self.perf.navigation = Some(duration),
            "distill" => self.perf.distill = Some(duration),
            "wake" => self.perf.wake = Some(duration),
            "resize" => self.perf.resize = Some(duration),
            "frame" => self.perf.frame = Some(duration),
            _ => {}
        }
        self.perf_events.push(PerfEvent {
            phase,
            label: label.to_string(),
            duration,
        });
        let overflow = self.perf_events.len().saturating_sub(PERF_HISTORY_LIMIT);
        if overflow > 0 {
            self.perf_events.drain(0..overflow);
        }
    }

    fn slowest_perf_event(&self) -> Option<&PerfEvent> {
        self.perf_events
            .iter()
            .max_by_key(|event| event.duration.as_millis())
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.engine.get_active_tab()
    }

    fn action_enabled(&self, action: Action) -> bool {
        match action {
            Action::Navigate => !self.address_input.trim().is_empty(),
            Action::RunShowcase => true,
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
            Action::ResetValidation => true,
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
        let viewport_size = (viewport.w.max(1), viewport.h.max(1));
        if self.last_frame_viewport != Some(viewport_size) {
            let resize_started = Instant::now();
            match self
                .engine
                .resize_current_viewport(viewport_size.0, viewport_size.1)
            {
                Ok(()) => {
                    self.record_perf("resize", resize_started.elapsed(), "viewport");
                    self.last_frame_viewport = Some(viewport_size);
                }
                Err(error) => {
                    self.record_perf("resize", resize_started.elapsed(), "viewport failed");
                    self.last_frame_refresh = Instant::now();
                    self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                    self.frame_dirty = self.frame_refresh_budget > 0;
                    self.last_status = format!("Servo viewport resize failed: {}", error);
                    self.last_ok = false;
                    self.validation.error_seen = true;
                    return;
                }
            }
        } else {
            self.record_perf("resize", Duration::ZERO, "viewport cached");
        }

        let started = Instant::now();
        match self.engine.capture_current_frame() {
            Ok(frame) => {
                self.record_perf("frame", started.elapsed(), "capture");
                if frame.width > 0 && frame.height > 0 && !frame.pixels.is_empty() {
                    self.validation.frame_seen = true;
                }
                self.latest_frame = Some(frame);
                self.last_frame_refresh = Instant::now();
                self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                self.frame_dirty = self.frame_refresh_budget > 0;
            }
            Err(error) => {
                self.record_perf("frame", started.elapsed(), "capture failed");
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
        self.record_log_with_consent(intent, status, None)
    }

    fn record_log_with_consent(
        &mut self,
        intent: &str,
        status: LogStatus,
        consent_signature: Option<String>,
    ) -> Result<(), String> {
        match self.log.record(&LogEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            persona_id: self.persona_id.clone(),
            intent: intent.to_string(),
            plan_json: "{}".to_string(),
            signature: "native-browser-shell".to_string(),
            consent_signature,
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

#[cfg(feature = "xilem-shell")]
fn load_browser_guard_firewall(data_dir: &Path) -> Result<(SextantFirewall, String), String> {
    if let Ok(path) = env::var("SEXTANT_GUARD_POLICY") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            let firewall = SextantFirewall::load_policy_overlay(trimmed)?;
            return Ok((firewall, format!("overlay {}", trimmed)));
        }
    }

    let policy_path = data_dir.join("guard-policy.json");
    if policy_path.exists() {
        let firewall = SextantFirewall::load_policy_overlay(&policy_path)?;
        return Ok((firewall, format!("overlay {}", policy_path.display())));
    }

    let canonical_policy_path = app_data_dir().join("browser").join("guard-policy.json");
    if canonical_policy_path != policy_path && canonical_policy_path.exists() {
        let firewall = SextantFirewall::load_policy_overlay(&canonical_policy_path)?;
        return Ok((
            firewall,
            format!("overlay {}", canonical_policy_path.display()),
        ));
    }

    Ok((SextantFirewall::new(), "built-in defaults".to_string()))
}

#[cfg(feature = "xilem-shell")]
fn init_browser_consent_vault() -> Result<CitadelVault, String> {
    let mut vault = CitadelVault::new();
    let _mnemonic = vault.initialize_new("browser-captains-key")?;
    Ok(vault)
}

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .init();

    let args: Vec<String> = env::args().collect();
    let operator_timeout = match parse_operator_timeout(&args) {
        Ok(timeout) => timeout,
        Err(error) => {
            eprintln!("[operator] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    if let Some(path) = operator_arg_value(&args, "--guard-policy") {
        env::set_var("SEXTANT_GUARD_POLICY", path);
    }

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

    if let Some(target) = operator_arg_value(&args, "--perf-probe") {
        run_operator_mode("perf-probe", operator_timeout, move || {
            run_perf_probe(&target)
        });
    }

    if let Some(target) = operator_arg_value(&args, "--perception-probe") {
        run_operator_mode("perception-probe", operator_timeout, move || {
            run_perception_probe(&target)
        });
    }

    if let Some(target) = operator_arg_value(&args, "--guard-probe") {
        run_operator_mode("guard-probe", operator_timeout, move || {
            run_guard_probe(&target)
        });
    }

    if args.iter().any(|arg| arg == "--perf-baseline") {
        run_operator_mode("perf-baseline", operator_timeout, run_perf_baseline);
    }

    if args.iter().any(|arg| arg == "--operator-smoke") {
        run_operator_mode("operator-smoke", operator_timeout, run_operator_smoke);
    }

    if args.iter().any(|arg| arg == "--showcase-run") {
        run_operator_mode("showcase-run", operator_timeout, run_showcase_script);
    }

    if args.iter().any(|arg| arg == "--real-browsing-smoke") {
        run_operator_mode(
            "real-browsing-smoke",
            operator_timeout,
            run_real_browsing_smoke,
        );
    }

    if let Some(intent) = operator_arg_value(&args, "--intent-run") {
        let expect = operator_arg_value(&args, "--expect");
        run_operator_mode("intent-run", operator_timeout, move || {
            run_intent_script(&intent, expect.as_deref())
        });
    }

    if let Some(intent) = operator_arg_value(&args, "--consent-run") {
        let expect = operator_arg_value(&args, "--expect");
        run_operator_mode("consent-run", operator_timeout, move || {
            run_authorized_intent_script(&intent, expect.as_deref())
        });
    }

    let window_smoke = match parse_window_smoke(&args) {
        Ok(spec) => spec,
        Err(error) => {
            eprintln!("[window-smoke] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    let startup_input = operator_arg_value(&args, "--start");
    let start_showcase = args
        .iter()
        .any(|arg| arg == "--start-showcase" || arg == "--demo");
    let start_real_browsing = args.iter().any(|arg| arg == "--start-real-browsing");
    let start_shell_interaction = args.iter().any(|arg| arg == "--start-shell-interaction");

    if let Err(error) = run_visible_app(
        window_smoke,
        startup_input,
        start_showcase,
        start_real_browsing,
        start_shell_interaction,
    ) {
        eprintln!("[sextant-browser] failed: {error}");
        std::process::exit(1);
    }
}

fn run_operator_mode<F>(label: &'static str, timeout: Duration, run: F) -> !
where
    F: FnOnce() -> Result<Vec<String>, String> + Send + 'static,
{
    let (result_tx, result_rx) = mpsc::channel();
    if let Err(error) = thread::Builder::new()
        .name(format!("sextant-browser-{}", label))
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
                "[{label}] timed out after {}s; terminating native-browser operator run",
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

fn run_visible_app(
    window_smoke: Option<WindowSmokeSpec>,
    startup_input: Option<String>,
    start_showcase: bool,
    start_real_browsing: bool,
    start_shell_interaction: bool,
) -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|error| format!("event loop initialization failed: {error}"))?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sextant Browser")
            .with_inner_size(PhysicalSize::new(1180, 760))
            .build(&event_loop)
            .map_err(|error| format!("window creation failed: {error}"))?,
    );

    let context = Context::new(window.clone())
        .map_err(|error| format!("softbuffer context initialization failed: {error}"))?;
    let mut surface = Surface::new(&context, window.clone())
        .map_err(|error| format!("softbuffer surface initialization failed: {error}"))?;
    let mut surface_size = PhysicalSize::new(0, 0);
    let mut app =
        BrowserApp::new().map_err(|error| format!("browser app initialization failed: {error}"))?;
    app.layout(window.inner_size());
    let visible_started = Instant::now();
    let mut visible_startup_work = Duration::ZERO;
    let mut pending_start_inputs = Vec::<(&'static str, String)>::new();
    let mut active_start_input: Option<(&'static str, String)> = None;
    let mut active_start_action: Option<Action> = None;
    let mut active_start_notice_drawn = false;

    if start_showcase {
        println!("[window-start] running launch showcase");
        let started = Instant::now();
        app.run_showcase_visible();
        visible_startup_work += started.elapsed();
        if !app.last_ok {
            return Err(format!("window showcase start failed: {}", app.last_status));
        }
        println!("[window-start] {}", app.last_status);
    }

    if start_real_browsing {
        println!("[window-start] running real browsing smoke");
        let started = Instant::now();
        app.run_real_browsing_visible();
        visible_startup_work += started.elapsed();
        if !app.last_ok {
            return Err(format!(
                "window real browsing start failed: {}",
                app.last_status
            ));
        }
        println!("[window-start] {}", app.last_status);
    }

    if start_shell_interaction {
        println!("[window-start] running shell interaction smoke");
        let started = Instant::now();
        app.run_shell_interaction_visible();
        visible_startup_work += started.elapsed();
        if !app.last_ok {
            return Err(format!(
                "window shell interaction start failed: {}",
                app.last_status
            ));
        }
        println!("[window-start] {}", app.last_status);
    }

    app.defer_user_navigation = true;

    if let Some(input) = startup_input.as_ref() {
        pending_start_inputs.push(("window-start", input.clone()));
    }

    if let Some(smoke) = window_smoke.as_ref() {
        println!("[window-smoke] starting visible browser shell smoke");
        if let Some(target) = smoke.target.as_ref() {
            pending_start_inputs.push(("window-smoke", target.clone()));
        }
    }

    app.update_title(&window);
    window.request_redraw();

    let smoke_mode = window_smoke.is_some();
    let smoke_deadline = window_smoke
        .as_ref()
        .map(|spec| Instant::now() + spec.timeout);
    let smoke_requires_frame = cfg!(feature = "servo-backend")
        && (startup_input.is_some()
            || start_showcase
            || start_real_browsing
            || start_shell_interaction
            || window_smoke
                .as_ref()
                .and_then(|spec| spec.target.as_ref())
                .is_some());
    let smoke_error = Arc::new(Mutex::new(None::<String>));
    let smoke_error_for_loop = smoke_error.clone();
    let smoke_started = visible_started;
    let mut smoke_first_draw: Option<Duration> = None;
    let mut smoke_first_frame: Option<Duration> = None;
    let mut visible_first_draw_seen = false;

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
                WindowEvent::ModifiersChanged(modifiers) => {
                    app.modifiers = modifiers.state();
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
                    match draw(&window, &mut surface, &mut surface_size, &app) {
                        Ok(draw_stats) => {
                            visible_first_draw_seen = true;
                            if active_start_input.is_some() || active_start_action.is_some() {
                                active_start_notice_drawn = true;
                            }
                            if smoke_mode {
                                let draw_elapsed = *smoke_first_draw
                                    .get_or_insert_with(|| smoke_started.elapsed());
                                if smoke_requires_frame && app.latest_frame.is_none() {
                                    if app.frame_refresh_budget == 0 {
                                        app.begin_frame_warmup();
                                    }
                                    return;
                                }
                                if app.latest_frame.is_some() && smoke_first_frame.is_none() {
                                    smoke_first_frame = Some(smoke_started.elapsed());
                                }
                                println!(
                                    "[window-smoke] visible shell draw passed in {}",
                                    format_duration(draw_elapsed)
                                );
                                if visible_startup_work > Duration::ZERO {
                                    println!(
                                        "[window-smoke] startup/navigation work {}",
                                        format_duration(visible_startup_work)
                                    );
                                }
                                println!(
                                    "[window-smoke] draw cost total {} | frame blit {} | present {}",
                                    format_duration(draw_stats.total),
                                    fmt_duration(draw_stats.frame_blit),
                                    format_duration(draw_stats.present)
                                );
                                if let Some(frame) = app.latest_frame.as_ref() {
                                    println!(
                                        "[window-smoke] latest Servo frame {}x{} ({} pixels)",
                                        frame.width,
                                        frame.height,
                                        frame.pixels.len()
                                    );
                                }
                                if let Some(first_frame) = smoke_first_frame {
                                    println!(
                                        "[window-smoke] first Servo frame in {}",
                                        format_duration(first_frame)
                                    );
                                }
                                elwt.exit();
                            }
                        }
                        Err(error) => {
                            window.set_title(&format!("Sextant Browser - draw failed: {}", error));
                            if smoke_mode {
                                eprintln!("[window-smoke] draw failed: {error}");
                                if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                    *smoke_error = Some(error.to_string());
                                }
                                elwt.exit();
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                if !visible_first_draw_seen
                    && (!pending_start_inputs.is_empty()
                        || app.pending_user_navigation.is_some()
                        || app.pending_user_action.is_some())
                {
                    window.request_redraw();
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                    return;
                }

                if active_start_input.is_none() && active_start_action.is_none() {
                    if let Some(input) = app.pending_user_navigation.take() {
                        active_start_input = Some(("window-user", input));
                        active_start_notice_drawn = false;
                        app.update_title(&window);
                        window.request_redraw();
                        return;
                    }
                    if let Some(action) = app.pending_user_action.take() {
                        active_start_action = Some(action);
                        active_start_notice_drawn = false;
                        app.update_title(&window);
                        window.request_redraw();
                        return;
                    }
                    if let Some((source, input)) = pending_start_inputs.first().cloned() {
                        pending_start_inputs.remove(0);
                        app.address_input = input.clone();
                        app.last_status =
                            format!("Opening {}...", truncate(&input, 80));
                        app.last_ok = true;
                        active_start_input = Some((source, input));
                        active_start_notice_drawn = false;
                        app.update_title(&window);
                        window.request_redraw();
                        return;
                    }
                }

                if (active_start_input.is_some() || active_start_action.is_some())
                    && !active_start_notice_drawn
                {
                    window.request_redraw();
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                    return;
                }

                if let Some(action) = active_start_action.take() {
                    let label = deferred_user_navigation_label(action);
                    println!("[window-user] {label}");
                    let started = Instant::now();
                    match action {
                        Action::Back => app.back(),
                        Action::DistillActive => app.distill_active(),
                        Action::Forward => app.forward(),
                        Action::Reload => app.reload(),
                        _ => {}
                    }
                    visible_startup_work += started.elapsed();
                    app.refresh_logs();
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                    return;
                }

                if let Some((source, input)) = active_start_input.take() {
                    println!("[{source}] opening {}", input);
                    let started = Instant::now();
                    match submit_start_input(&mut app, &input) {
                        Ok(()) => {
                            visible_startup_work += started.elapsed();
                            println!("[{source}] {}", app.last_status);
                            app.update_title(&window);
                            window.request_redraw();
                        }
                        Err(error) => {
                            visible_startup_work += started.elapsed();
                            if smoke_mode {
                                let error = format!("{source} open failed: {error}");
                                eprintln!("[window-smoke] {error}");
                                if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                    *smoke_error = Some(error);
                                }
                                elwt.exit();
                            } else {
                                window.set_title(&format!(
                                    "Sextant Browser - startup open failed: {}",
                                    error
                                ));
                                window.request_redraw();
                            }
                        }
                    }
                    return;
                }

                if let Some(deadline) = smoke_deadline {
                    if Instant::now() >= deadline {
                        let error = "visible shell smoke timed out before first successful draw";
                        eprintln!("[window-smoke] {error}");
                        if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                            *smoke_error = Some(error.to_string());
                        }
                        elwt.exit();
                        return;
                    }
                }

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
                    let mut wake_at = Instant::now() + delay;
                    if let Some(deadline) = smoke_deadline {
                        if deadline < wake_at {
                            wake_at = deadline;
                        }
                    }
                    elwt.set_control_flow(ControlFlow::WaitUntil(wake_at));
                } else {
                    if let Some(deadline) = smoke_deadline {
                        let pulse = Instant::now() + Duration::from_millis(100);
                        let wake_at = if pulse < deadline { pulse } else { deadline };
                        elwt.set_control_flow(ControlFlow::WaitUntil(wake_at));
                    } else {
                        elwt.set_control_flow(ControlFlow::Wait);
                    }
                }
            }
            _ => {}
        })
        .map_err(|error| format!("event loop failed: {error}"))?;

    if let Ok(mut smoke_error) = smoke_error.lock() {
        if let Some(error) = smoke_error.take() {
            return Err(error);
        }
    }

    Ok(())
}

fn submit_start_input(app: &mut BrowserApp, input: &str) -> Result<(), String> {
    app.address_input = input.to_string();
    if native_intent_body(input).is_some() {
        app.run_native_intent(input);
    } else {
        let url = parse_navigation_target(input)?;
        app.navigate_to(url);
    }
    if !app.last_ok {
        return Err(app.last_status.clone());
    }
    Ok(())
}

fn run_operator_smoke() -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting native-browser operator bridge smoke".to_string());

    let data_dir =
        env::temp_dir().join(format!("sextant-browser-operator-smoke-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
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

    report.push("native-browser operator bridge smoke passed".to_string());
    Ok(report)
}

fn run_showcase_script() -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting Sextant launch showcase run".to_string());

    let data_dir = env::temp_dir().join(format!("sextant-browser-showcase-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));
    report.extend(run_showcase_workflow(&mut app)?);
    Ok(report)
}

fn run_real_browsing_smoke() -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting real browsing smoke suite".to_string());

    let data_dir =
        env::temp_dir().join(format!("sextant-browser-real-browsing-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    report.extend(run_real_browsing_workflow(&mut app)?);
    Ok(report)
}

fn run_shell_interaction_workflow(app: &mut BrowserApp) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting visible shell interaction smoke".to_string());

    click_rect(app, app.address_rect, "address field")?;
    if app.focus != FocusTarget::Address {
        return Err("address field click did not focus the address input".to_string());
    }
    report.push("address field click focused input".to_string());

    app.address_input = "https://example.com".to_string();
    click_action(app, Action::Navigate, "RUN")?;
    ensure_app_ok(app, "address run click")?;
    report.push(app.last_status.clone());

    click_action(app, Action::DistillActive, "DISTILL")?;
    ensure_app_ok(app, "distill click")?;
    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.clone())
        .ok_or_else(|| "distill click did not attach page data".to_string())?;
    if !operator_page_search_text(&page).contains("Example Domain") {
        return Err("distill click did not produce Example Domain content".to_string());
    }
    report.push(format!("distill click stored '{}'", page.title));

    app.wake_query = "Example Domain".to_string();
    click_action(app, Action::SearchWake, "WAKE")?;
    ensure_app_ok(app, "Wake click")?;
    if app.wake_results.is_empty() {
        return Err("Wake click returned no results".to_string());
    }
    report.push(format!(
        "Wake click returned {} result(s)",
        app.wake_results.len()
    ));

    click_rect(app, app.wake_tab_rect, "Wake tab")?;
    ensure_view(app, MainView::Wake, "Wake tab")?;
    report.push("Wake tab click switched view".to_string());

    click_rect(app, app.log_tab_rect, "Log tab")?;
    ensure_view(app, MainView::Log, "Log tab")?;
    report.push("Log tab click switched view".to_string());

    click_rect(app, app.guard_tab_rect, "Guard tab")?;
    ensure_view(app, MainView::Guard, "Guard tab")?;
    report.push("Guard tab click switched view".to_string());

    click_rect(app, app.perception_tab_rect, "Sense tab")?;
    ensure_view(app, MainView::Perception, "Sense tab")?;
    report.push("Sense tab click switched view".to_string());

    click_rect(app, app.perf_tab_rect, "Perf tab")?;
    ensure_view(app, MainView::Perf, "Perf tab")?;
    report.push("Perf tab click switched view".to_string());

    click_rect(app, app.validation_tab_rect, "Validation tab")?;
    ensure_view(app, MainView::Validation, "Validation tab")?;
    report.push("Validation tab click switched view".to_string());

    click_rect(app, app.browser_tab_rect, "Browser tab")?;
    ensure_view(app, MainView::Browser, "Browser tab")?;
    report.push("Browser tab click switched view".to_string());

    let viewport_center = rect_center(app.browser_viewport_rect);
    if !app.handle_mouse_input(viewport_center.0, viewport_center.1, ElementState::Released) {
        return Err("browser viewport click was not handled".to_string());
    }
    if app.focus != FocusTarget::Browser || !app.validation.browser_focus_seen {
        return Err("browser viewport click did not focus the viewport".to_string());
    }
    report.push("browser viewport click focused Servo viewport".to_string());

    let first_tab_id = app
        .active_tab()
        .map(|tab| tab.id)
        .ok_or_else(|| "shell interaction smoke lost the first tab".to_string())?;
    click_action(app, Action::NewTab, "NEW TAB")?;
    ensure_app_ok(app, "new tab click")?;
    let second_tab_id = app
        .active_tab()
        .map(|tab| tab.id)
        .ok_or_else(|| "new tab click did not create an active tab".to_string())?;
    report.push(app.last_status.clone());

    app.address_input = "https://www.iana.org/domains/reserved".to_string();
    click_action(app, Action::Navigate, "RUN second tab")?;
    ensure_app_ok(app, "second tab navigation")?;
    report.push("second tab navigated independently".to_string());

    click_page_tab(app, first_tab_id, "first page tab")?;
    ensure_active_tab(app, first_tab_id, "first page tab")?;
    report.push("first page tab click restored the original tab".to_string());

    click_page_tab(app, second_tab_id, "second page tab")?;
    ensure_active_tab(app, second_tab_id, "second page tab")?;
    report.push("second page tab click restored the new tab".to_string());

    click_action(app, Action::CloseTab, "CLOSE TAB")?;
    ensure_app_ok(app, "close tab click")?;
    ensure_active_tab(app, first_tab_id, "close tab fallback")?;
    report.push(app.last_status.clone());

    for _ in 0..=MAX_VISIBLE_PAGE_TABS {
        click_action(app, Action::NewTab, "overflow NEW TAB")?;
        ensure_app_ok(app, "overflow new tab click")?;
    }
    if app
        .page_tab_rects
        .iter()
        .any(|region| region.tab_id == first_tab_id)
    {
        return Err("page-tab overflow did not move the first tab out of view".to_string());
    }
    while app.can_page_tabs_previous() {
        let previous_rect = app.page_tab_prev_rect;
        click_rect(app, previous_rect, "previous page-tab pager")?;
        ensure_app_ok(app, "previous page-tab pager")?;
    }
    click_page_tab(app, first_tab_id, "overflow first page tab")?;
    ensure_active_tab(app, first_tab_id, "overflow first page tab")?;
    report.push("page-tab overflow pager restored the hidden first tab".to_string());

    app.refresh_logs();
    if app.recent_logs.len() < 5 {
        return Err(format!(
            "Captain's Log returned only {} recent entries after shell interaction smoke",
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
        .ok_or_else(|| "shell interaction smoke did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "shell interaction smoke captured an empty Servo frame: {}x{} pixels={}",
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

    report.push("visible shell interaction smoke passed".to_string());
    Ok(report)
}

fn run_real_browsing_workflow(app: &mut BrowserApp) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    navigate_distill_expect(
        app,
        "https://example.com",
        "Example Domain",
        "baseline content page",
        &mut report,
    )?;

    let query = "Sextant native browser test";
    report.push("starting Google search interaction".to_string());
    app.navigate_to(parse_navigation_target("https://www.google.com")?);
    ensure_app_ok(app, "google navigation")?;
    report.push(app.last_status.clone());

    let fill = app
        .fill_selector_current_page("textarea[name=q]", query)
        .map_err(|error| format!("google search fill failed: {}", error))?;
    if !fill.ok || fill.value != query {
        return Err(format!(
            "google search fill returned unexpected result: {:?}",
            fill
        ));
    }
    app.record_log("real browsing fill google search", LogStatus::Success)?;
    report.push(format!("filled Google search box with '{}'", query));

    let submit = app
        .submit_selector_current_page("form")
        .map_err(|error| format!("google search submit failed: {}", error))?;
    if !submit.ok {
        return Err(format!(
            "google search submit returned unexpected result: {:?}",
            submit
        ));
    }
    app.record_log("real browsing submit google search", LogStatus::Success)?;
    report.push(format!("submitted Google search form tag {}", submit.tag));

    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains(query) {
        return Err(format!(
            "google search result did not expose query '{}' in distilled page or URL",
            query
        ));
    }
    report.push(format!(
        "Google search distilled '{}' and retained query",
        page.title
    ));

    let docs_tab_id = app.engine.open_tab();
    app.show_page_tab(docs_tab_id);
    app.sync_address_to_active_tab();
    app.layout(app.window_size);
    report.push(format!(
        "opened isolated documentation tab {} after Google search",
        short_id(docs_tab_id)
    ));

    navigate_distill_expect(
        app,
        "https://developer.mozilla.org/en-US/docs/Web/HTML",
        "HTML",
        "heavier documentation page",
        &mut report,
    )?;

    app.reload();
    ensure_app_ok(app, "reload")?;
    report.push(app.last_status.clone());
    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains("HTML") {
        return Err("reload did not preserve the documentation page content".to_string());
    }
    report.push("reload preserved distillable documentation content".to_string());

    navigate_distill_expect(
        app,
        "https://www.iana.org/domains/reserved",
        "IANA",
        "second history page",
        &mut report,
    )?;

    app.back();
    ensure_app_ok(app, "back")?;
    report.push(app.last_status.clone());
    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains("HTML") {
        return Err("back navigation did not return to the documentation page".to_string());
    }
    report.push("back navigation returned to documentation page".to_string());

    app.forward();
    ensure_app_ok(app, "forward")?;
    report.push(app.last_status.clone());
    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains("IANA") {
        return Err("forward navigation did not return to the IANA page".to_string());
    }
    report.push("forward navigation returned to IANA page".to_string());

    if app.wake_results.is_empty() {
        return Err("real browsing Wake search returned no result".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    if app.recent_logs.len() < 5 {
        return Err(format!(
            "Captain's Log returned only {} recent entries after real browsing suite",
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
        .ok_or_else(|| "real browsing suite did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "real browsing suite captured an empty Servo frame: {}x{} pixels={}",
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

    report.push("real browsing smoke suite passed".to_string());
    Ok(report)
}

fn navigate_distill_expect(
    app: &mut BrowserApp,
    target: &str,
    expect: &str,
    label: &str,
    report: &mut Vec<String>,
) -> Result<(), String> {
    report.push(format!("opening {label}: {target}"));
    app.navigate_to(parse_navigation_target(target)?);
    ensure_app_ok(app, label)?;
    report.push(app.last_status.clone());

    let page = distill_operator_page(app)?;
    let haystack = operator_page_search_text(&page);
    if !haystack.contains(expect) {
        return Err(format!(
            "{label} did not include expected text '{}' in distilled page '{}' ({})",
            expect, page.title, page.url
        ));
    }
    let counts = semantic_counts(&page);
    report.push(format!(
        "{label} distilled '{}' via {} ({} chars, h={} links={} inputs={} images={} text={})",
        page.title,
        distillation_label(&page),
        page.content.chars().count(),
        counts.headings,
        counts.links,
        counts.inputs,
        counts.images,
        counts.text
    ));
    Ok(())
}

fn ensure_app_ok(app: &BrowserApp, label: &str) -> Result<(), String> {
    if app.last_ok {
        Ok(())
    } else {
        Err(format!("{label} failed: {}", app.last_status))
    }
}

fn click_action(app: &mut BrowserApp, action: Action, label: &str) -> Result<(), String> {
    let rect = app
        .buttons
        .iter()
        .find(|button| button.action == action)
        .map(|button| button.rect)
        .ok_or_else(|| format!("{label} button was not present in the visible chrome"))?;
    click_rect(app, rect, label)
}

fn click_page_tab(app: &mut BrowserApp, tab_id: Uuid, label: &str) -> Result<(), String> {
    let rect = app
        .page_tab_rects
        .iter()
        .find(|region| region.tab_id == tab_id)
        .map(|region| region.rect)
        .ok_or_else(|| format!("{label} was not visible in the page-tab strip"))?;
    click_rect(app, rect, label)
}

fn click_rect(app: &mut BrowserApp, rect: Rect, label: &str) -> Result<(), String> {
    let (x, y) = rect_center(rect);
    app.click(x, y);
    if app.last_ok {
        Ok(())
    } else {
        Err(format!("{label} click failed: {}", app.last_status))
    }
}

fn rect_center(rect: Rect) -> (f64, f64) {
    (
        rect.x as f64 + rect.w as f64 / 2.0,
        rect.y as f64 + rect.h as f64 / 2.0,
    )
}

fn ensure_view(app: &BrowserApp, view: MainView, label: &str) -> Result<(), String> {
    if app.main_view == view {
        Ok(())
    } else {
        Err(format!("{label} did not switch to the expected view"))
    }
}

fn ensure_active_tab(app: &BrowserApp, tab_id: Uuid, label: &str) -> Result<(), String> {
    match app.active_tab().map(|tab| tab.id) {
        Some(active_id) if active_id == tab_id => Ok(()),
        Some(active_id) => Err(format!(
            "{label} expected active tab {}, got {}",
            short_id(tab_id),
            short_id(active_id)
        )),
        None => Err(format!("{label} expected an active tab")),
    }
}

fn run_showcase_workflow(app: &mut BrowserApp) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    let intent = "intent: open https://example.com and distill";
    app.run_native_intent(intent);
    if !app.last_ok {
        return Err(format!("showcase intent failed: {}", app.last_status));
    }
    report.push(format!("intent status: {}", app.pilot_status));
    report.push(format!("intent result: {}", app.pilot_result));

    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.clone())
        .ok_or_else(|| "showcase intent did not attach distilled page data".to_string())?;
    if !operator_page_search_text(&page).contains("Example Domain") {
        return Err("showcase intent page did not contain Example Domain".to_string());
    }
    let counts = semantic_counts(&page);
    report.push(format!(
        "intent distilled '{}' via {} (h={} links={} text={})",
        page.title,
        distillation_label(&page),
        counts.headings,
        counts.links,
        counts.text
    ));

    app.new_tab();
    if !app.last_ok {
        return Err(format!("showcase new tab failed: {}", app.last_status));
    }
    report.push(app.last_status.clone());

    let sentinel = "native pilot showcase";
    let url = Url::parse(&format!(
        "data:text/html,{}",
        "<!DOCTYPE html>\
<title>Sextant Showcase</title>\
<main>\
<h1>Sextant Showcase</h1>\
<p>Intent Bar, Wake, and native operator bridge are online.</p>\
<input id='q' value='empty'>\
<button id='go' onclick=\"document.getElementById('out').textContent=document.getElementById('q').value\">Go</button>\
<p id='out'>waiting</p>\
</main>"
    ))
    .map_err(|error| format!("showcase data URL did not parse: {}", error))?;
    app.navigate_to(url);
    if !app.last_ok {
        return Err(format!("showcase data page failed: {}", app.last_status));
    }
    report.push(app.last_status.clone());

    let fill = app
        .fill_selector_current_page("#q", sentinel)
        .map_err(|error| format!("showcase fill failed: {}", error))?;
    if !fill.ok || fill.value != sentinel {
        return Err(format!(
            "showcase fill returned unexpected result: {:?}",
            fill
        ));
    }
    app.record_log("showcase fill #q", LogStatus::Success)?;
    report.push(format!("filled {} with {}", fill.selector, fill.value));

    let click = app
        .click_selector_current_page("#go")
        .map_err(|error| format!("showcase click failed: {}", error))?;
    if !click.ok {
        return Err(format!(
            "showcase click returned unexpected result: {:?}",
            click
        ));
    }
    app.record_log("showcase click #go", LogStatus::Success)?;
    report.push(format!("clicked {} tag {}", click.selector, click.tag));

    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains(sentinel) {
        return Err(format!(
            "showcase distilled page did not include sentinel '{}'",
            sentinel
        ));
    }
    report.push(format!(
        "interactive page distilled '{}' with sentinel",
        page.title
    ));

    if app.wake_results.is_empty() {
        return Err("showcase Wake search returned no result".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    if app.recent_logs.len() < 5 {
        return Err(format!(
            "showcase Captain's Log returned only {} recent entries",
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
        .ok_or_else(|| "showcase did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "showcase captured an empty Servo frame: {}x{} pixels={}",
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

    report.push(format!(
        "validation checks passed {}/{}",
        app.validation_pass_count(),
        app.validation_rows().len()
    ));
    report.push("Sextant launch showcase run passed".to_string());
    Ok(report)
}

fn run_intent_script(intent: &str, expect: Option<&str>) -> Result<Vec<String>, String> {
    run_intent_script_inner(intent, expect, false)
}

fn run_authorized_intent_script(intent: &str, expect: Option<&str>) -> Result<Vec<String>, String> {
    run_intent_script_inner(intent, expect, true)
}

fn run_intent_script_inner(
    intent: &str,
    expect: Option<&str>,
    authorize_consent: bool,
) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!(
        "starting native {}intent run for {}",
        if authorize_consent { "authorized " } else { "" },
        intent
    ));

    let data_dir = env::temp_dir().join(format!("sextant-browser-intent-run-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    app.address_input = intent.to_string();
    app.run_native_intent(intent);
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(format!("pilot status: {}", app.pilot_status));
    report.push(format!("pilot result: {}", app.pilot_result));
    if app.pending_consent.is_some() {
        if !authorize_consent {
            return Err(
                "intent run paused for Captain's Key consent; use --consent-run to authorize and resume"
                    .to_string(),
            );
        }
        report.push("authorizing pending Captain's Key consent".to_string());
        app.authorize_pilot_consent();
        if !app.last_ok {
            return Err(app.last_status);
        }
        report.push(format!("pilot status after consent: {}", app.pilot_status));
        report.push(format!("pilot result after consent: {}", app.pilot_result));
    }

    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.clone())
        .ok_or_else(|| "intent run did not attach distilled page data".to_string())?;
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

    if let Some(text) = expect {
        let haystack = operator_page_search_text(&page);
        if !haystack.contains(text) {
            return Err(format!(
                "expected text '{}' was not found in distilled page '{}' ({})",
                text, page.title, page.url
            ));
        }
        report.push(format!("found expected text '{}'", text));
    }

    if app.wake_results.is_empty() {
        return Err("intent run Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    if !app.recent_logs.iter().any(|entry| {
        entry.intent.starts_with("pilot intent ") || entry.intent.starts_with("native intent ")
    }) {
        return Err("Captain's Log did not include the intent audit entry".to_string());
    }
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    if cfg!(feature = "servo-backend") {
        app.refresh_frame();
        let frame = app
            .latest_frame
            .as_ref()
            .ok_or_else(|| "intent run did not capture a Servo frame".to_string())?;
        if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
            return Err(format!(
                "intent run captured an empty Servo frame: {}x{} pixels={}",
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
    } else {
        report.push("reader/fallback build completed without Servo frame capture".to_string());
    }

    report.push(format!(
        "native {}intent run passed",
        if authorize_consent { "authorized " } else { "" }
    ));
    Ok(report)
}

fn run_operator_script(spec: OperatorRunSpec) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!(
        "starting scripted native-browser run for {}",
        spec.target
    ));

    let data_dir = env::temp_dir().join(format!("sextant-browser-operator-run-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
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
                let result = app.fill_selector_current_page(&selector, &value)?;
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
                let result = app.click_selector_current_page(&selector)?;
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
                let result = app.submit_selector_current_page(&selector)?;
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

    report.push("scripted native-browser run passed".to_string());
    Ok(report)
}

fn run_operator_probe(target: &str) -> Result<Vec<String>, String> {
    run_page_probe(target, false)
}

fn run_perf_probe(target: &str) -> Result<Vec<String>, String> {
    run_page_probe(target, true)
}

fn run_perception_probe(target: &str) -> Result<Vec<String>, String> {
    let mut report = run_page_probe(target, false)?;
    let data_line = report
        .iter()
        .position(|line| line.starts_with("distilled "))
        .unwrap_or(report.len());
    report.insert(
        data_line.saturating_add(1),
        "perception probe includes page type, semantic counts, key nodes, and distillation metadata"
            .to_string(),
    );
    Ok(report)
}

fn run_guard_probe(target: &str) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!("starting browser guard probe for {}", target));

    let data_dir = env::temp_dir().join(format!("sextant-browser-guard-probe-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let url = parse_navigation_target(target)?;
    report.extend(guard_report_lines(&app, Some(&url), None));
    app.navigate_to(url);
    if !app.last_ok {
        if app.last_status.starts_with("Local guard blocked") {
            report.push(app.last_status.clone());
            report.push("browser guard probe blocked navigation".to_string());
            return Ok(report);
        }
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());

    app.distill_active();
    if app.last_ok {
        if let Some(page) = app.active_tab().and_then(|tab| tab.distilled_page.as_ref()) {
            report.extend(guard_report_lines(&app, Some(&page.url), Some(page)));
        }
    } else {
        report.push(format!("guard distillation skipped: {}", app.last_status));
    }
    report.push("browser guard probe passed".to_string());
    Ok(report)
}

fn run_page_probe(target: &str, repeat_frame_capture: bool) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!("starting native-browser probe for {}", target));

    let data_dir =
        env::temp_dir().join(format!("sextant-browser-operator-probe-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let url = parse_navigation_target(target)?;
    app.navigate_to(url.clone());
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());
    push_perf_report(&mut report, "after navigation", &app);

    app.distill_active();
    if !app.last_ok {
        return Err(app.last_status);
    }
    push_perf_report(&mut report, "after distillation", &app);

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
    push_perception_report(&mut report, page);

    if app.wake_results.is_empty() {
        return Err("probe Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    if repeat_frame_capture {
        app.distill_active();
        if !app.last_ok {
            return Err(format!("repeat distillation failed: {}", app.last_status));
        }
        push_perf_report(&mut report, "after repeat distillation", &app);
    }

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
    push_perf_report(&mut report, "after frame capture", &app);

    if repeat_frame_capture {
        app.refresh_frame();
        let frame = app
            .latest_frame
            .as_ref()
            .ok_or_else(|| "probe did not capture a warm Servo frame".to_string())?;
        if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
            return Err(format!(
                "probe captured an empty warm Servo frame: {}x{} pixels={}",
                frame.width,
                frame.height,
                frame.pixels.len()
            ));
        }
        report.push(format!(
            "captured warm Servo frame {}x{} ({} pixels)",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
        push_perf_report(&mut report, "after warm frame capture", &app);
    }

    report.push("native-browser probe passed".to_string());
    Ok(report)
}

fn run_perf_baseline() -> Result<Vec<String>, String> {
    let targets = [
        "https://example.com",
        "https://www.rust-lang.org/",
        "https://developer.mozilla.org/en-US/docs/Web/HTML",
        "https://en.wikipedia.org/wiki/Browser_engine",
    ];
    let mut report = vec![format!(
        "starting browser performance baseline across {} target(s)",
        targets.len()
    )];
    for target in targets {
        report.push(format!("baseline target: {target}"));
        match run_perf_probe(target) {
            Ok(lines) => {
                for line in lines {
                    if line.starts_with("perf ")
                        || line.starts_with("distilled ")
                        || line.starts_with("captured Servo frame ")
                    {
                        report.push(format!("{target}: {line}"));
                    } else if line.starts_with("captured warm Servo frame ") {
                        report.push(format!("{target}: {line}"));
                    }
                }
            }
            Err(error) => {
                report.push(format!("{target}: failed: {error}"));
            }
        }
    }
    report.push("browser performance baseline complete".to_string());
    Ok(report)
}

fn push_perf_report(report: &mut Vec<String>, label: &str, app: &BrowserApp) {
    report.push(format!("perf {label}: {}", app.perf.summary()));
}

fn push_perception_report(report: &mut Vec<String>, page: &sextant_engine::DistilledPage) {
    let counts = semantic_counts(page);
    report.push(format!(
        "perception summary: {}",
        page_perception_summary(page)
    ));
    report.push(format!(
        "perception counts: headings={} links={} inputs={} images={} text={} buttons={}",
        counts.headings, counts.links, counts.inputs, counts.images, counts.text, counts.buttons
    ));
    for (index, node) in key_semantic_nodes(page).into_iter().take(6).enumerate() {
        report.push(format!(
            "perception node {}: {} | {} | {}",
            index + 1,
            semantic_node_type_label(&node.node_type),
            truncate(&node.selector, 48),
            truncate(&node.text, 96)
        ));
    }
    if let Some(source) = page
        .metadata
        .get("distillation_backend")
        .or_else(|| page.metadata.get("source"))
        .or_else(|| page.metadata.get("distiller"))
    {
        report.push(format!("perception source: {source}"));
    }
}

fn distill_operator_page(app: &mut BrowserApp) -> Result<sextant_engine::DistilledPage, String> {
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
    for (key, value) in page.url.query_pairs() {
        values.push(key.into_owned());
        values.push(value.into_owned());
    }
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
            "--guard-policy" => {
                args.get(cursor + 1)
                    .ok_or_else(|| "--guard-policy requires a path".to_string())?;
                cursor += 2;
            }
            other => {
                return Err(format!(
                    "unknown operator-run argument '{}'; expected --fill, --click, --submit, --expect, --guard-policy, or --operator-timeout",
                    other
                ));
            }
        }
    }
    Ok(Some(OperatorRunSpec { target, steps }))
}

fn parse_operator_timeout(args: &[String]) -> Result<Duration, String> {
    parse_duration_arg(args, "--operator-timeout", OPERATOR_DEFAULT_TIMEOUT)
}

fn parse_window_smoke(args: &[String]) -> Result<Option<WindowSmokeSpec>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--window-smoke") else {
        return Ok(None);
    };
    let target = args
        .get(index + 1)
        .filter(|value| !value.starts_with("--"))
        .cloned();
    let timeout = parse_duration_arg(args, "--window-smoke-timeout", WINDOW_SMOKE_DEFAULT_TIMEOUT)?;
    Ok(Some(WindowSmokeSpec { target, timeout }))
}

fn parse_duration_arg(args: &[String], flag: &str, default: Duration) -> Result<Duration, String> {
    let Some(value) = operator_arg_value(args, flag) else {
        return Ok(default);
    };
    let seconds = value
        .parse::<u64>()
        .map_err(|error| format!("{flag} expects a positive whole number of seconds: {error}"))?;
    if seconds == 0 {
        return Err(format!("{flag} must be greater than zero"));
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

fn native_intent_body(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    for prefix in [
        "intent:",
        "pilot:",
        "research:",
        "summarize:",
        "summarise:",
        "remember:",
    ] {
        if trimmed.len() >= prefix.len() && trimmed[..prefix.len()].eq_ignore_ascii_case(prefix) {
            let body = trimmed[prefix.len()..].trim();
            return (!body.is_empty()).then(|| body.to_string());
        }
    }

    let lower = trimmed.to_ascii_lowercase();
    let explicit_work = lower.starts_with("research ")
        || lower.starts_with("summarize ")
        || lower.starts_with("summarise ")
        || lower.starts_with("remember ")
        || lower.starts_with("distill ");
    let open_then_memory = lower.starts_with("open ")
        && (lower.contains(" and distill")
            || lower.contains(" and remember")
            || lower.contains(" and summarize")
            || lower.contains(" and summarise"));

    if explicit_work || open_then_memory {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn intent_needs_consent(intent: &str) -> bool {
    let lower = intent.to_ascii_lowercase();
    [
        "buy",
        "purchase",
        "checkout",
        "delete",
        "remove",
        "transfer",
        "wire",
        "sign",
        "authorize",
        "submit payment",
        "send money",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

fn plan_native_intent(intent: &str) -> Result<NativeIntentPlan, String> {
    let target_input = intent_navigation_text(intent);
    let target = parse_navigation_target(&target_input)?;
    let should_distill = intent_should_distill(intent);
    let mut steps = vec![
        format!("Resolve target: {}", truncate(&target_input, 42)),
        format!(
            "Navigate with {}",
            if cfg!(feature = "servo-backend") {
                "Servo"
            } else {
                "fallback backend"
            }
        ),
    ];
    if should_distill {
        steps.push("Distill page into Wake".to_string());
        steps.push("Search Wake for the recorded page".to_string());
    }

    Ok(NativeIntentPlan {
        intent: intent.to_string(),
        target,
        should_distill,
        steps,
    })
}

#[cfg(feature = "xilem-shell")]
fn pilot_action_plan(intent: &str) -> Result<Vec<PilotAction>, String> {
    if intent_needs_consent(intent) {
        let plan = plan_native_intent(intent)?;
        return Ok(vec![
            PilotAction::Analyze(
                "Sensitive browser action detected; Captain's Key consent is required.".to_string(),
            ),
            PilotAction::RequestConsent(format!(
                "Authorize this browser action before execution: {}",
                truncate(intent, 72)
            )),
            PilotAction::Navigate(plan.target.clone()),
            PilotAction::Distill,
            PilotAction::Perceive,
            PilotAction::Analyze(format!(
                "Captain's Key authorized a bounded review of {} for {}. Destructive or purchasing clicks remain gated.",
                short_url(&plan.target),
                truncate(intent, 42)
            )),
        ]);
    }

    let plan = plan_native_intent(intent)?;
    let mut actions = vec![PilotAction::Navigate(plan.target.clone())];
    if plan.should_distill {
        actions.push(PilotAction::Distill);
        actions.push(PilotAction::Analyze(format!(
            "Opened {} and stored the distilled page in Wake for {} across {} planned steps.",
            short_url(&plan.target),
            truncate(&plan.intent, 42),
            plan.steps.len()
        )));
    } else {
        actions.push(PilotAction::Analyze(format!(
            "Opened {} through the Pilot action lane for {} across {} planned steps.",
            short_url(&plan.target),
            truncate(&plan.intent, 42),
            plan.steps.len()
        )));
    }
    Ok(actions)
}

#[cfg(feature = "xilem-shell")]
fn pilot_action_step_label(action: &PilotAction) -> String {
    match action {
        PilotAction::Navigate(url) => format!("Pilot navigate: {}", short_url(url)),
        PilotAction::Distill => "Pilot distill active page into Wake".to_string(),
        PilotAction::Perceive => "Pilot perceive active browser state".to_string(),
        PilotAction::PerceiveMultiModal => {
            "Pilot queue multimodal perception through Neural Bridge".to_string()
        }
        PilotAction::RequestConsent(message) => {
            format!(
                "Pilot request Captain's Key consent: {}",
                truncate(message, 56)
            )
        }
        PilotAction::Analyze(message) => format!("Pilot analyze: {}", truncate(message, 64)),
        PilotAction::OpenTab(url) => format!("Pilot open tab: {}", short_url(url)),
        PilotAction::SwitchTab(tab_id) => format!("Pilot switch tab: {}", short_id(*tab_id)),
        PilotAction::CloseTab(tab_id) => format!("Pilot close tab: {}", short_id(*tab_id)),
    }
}

fn intent_should_distill(intent: &str) -> bool {
    let lower = intent.to_ascii_lowercase();
    [
        "research",
        "summarize",
        "summarise",
        "distill",
        "remember",
        "wake",
        "learn",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

fn intent_navigation_text(intent: &str) -> String {
    if let Some(candidate) = first_navigation_candidate(intent) {
        return candidate;
    }

    let mut text = intent.trim().to_string();
    let lower = text.to_ascii_lowercase();
    for prefix in [
        "open ",
        "go to ",
        "visit ",
        "research ",
        "summarize ",
        "summarise ",
        "distill ",
        "remember ",
        "find ",
        "search for ",
        "look up ",
    ] {
        if lower.starts_with(prefix) {
            text = text[prefix.len()..].trim().to_string();
            break;
        }
    }

    strip_memory_suffixes(&text)
}

fn first_navigation_candidate(input: &str) -> Option<String> {
    for token in input.split_whitespace() {
        let candidate = token.trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';'
            )
        });
        let candidate = candidate.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?'));
        if candidate.is_empty() {
            continue;
        }
        if Url::parse(candidate).is_ok() {
            return Some(candidate.to_string());
        }
        if candidate.contains('.') && !candidate.contains('/') {
            let lower = candidate.to_ascii_lowercase();
            let looks_like_domain = lower
                .split('.')
                .last()
                .map(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
                .unwrap_or(false);
            if looks_like_domain {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

fn strip_memory_suffixes(input: &str) -> String {
    let mut text = input.trim().to_string();
    loop {
        let lower = text.to_ascii_lowercase();
        let Some(index) = [
            " and distill",
            " and remember",
            " and summarize",
            " and summarise",
            " then distill",
            " then remember",
            " then summarize",
            " then summarise",
        ]
        .iter()
        .filter_map(|suffix| lower.find(suffix))
        .min() else {
            break;
        };
        text.truncate(index);
        text = text
            .trim()
            .trim_end_matches(|c: char| c == ',' || c == ';')
            .to_string();
    }
    if text.is_empty() {
        input.trim().to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_explicit_native_intents() {
        assert_eq!(
            native_intent_body("intent: open example.com and distill").as_deref(),
            Some("open example.com and distill")
        );
        assert_eq!(
            native_intent_body("research Rust ownership docs").as_deref(),
            Some("research Rust ownership docs")
        );
        assert!(native_intent_body("plain web search").is_none());
    }

    #[test]
    fn plans_domain_intent_without_losing_memory_step() {
        let plan = plan_native_intent("open example.com and distill").unwrap();
        assert_eq!(plan.target.as_str(), "https://example.com/");
        assert!(plan.should_distill);
        assert!(plan.steps.iter().any(|step| step.contains("Distill")));
    }

    #[test]
    fn strips_intent_verbs_for_search_targets() {
        assert_eq!(
            intent_navigation_text("research sovereign browser architecture"),
            "sovereign browser architecture"
        );
        let target =
            parse_navigation_target(&intent_navigation_text("summarize https://example.com now"))
                .unwrap();
        assert_eq!(target.as_str(), "https://example.com/");
    }

    #[test]
    fn pauses_sensitive_intents_for_consent() {
        assert!(intent_needs_consent("buy this item and checkout"));
        assert!(!intent_needs_consent("summarize example.com"));
    }

    #[cfg(feature = "xilem-shell")]
    #[test]
    fn pilot_action_plan_pauses_sensitive_intents() {
        let actions = pilot_action_plan("buy this item and checkout").unwrap();
        assert!(matches!(actions.first(), Some(PilotAction::Analyze(_))));
        assert!(matches!(
            actions.get(1),
            Some(PilotAction::RequestConsent(message)) if message.contains("Authorize")
        ));
        assert!(
            actions
                .iter()
                .skip(2)
                .any(|action| matches!(action, PilotAction::Navigate(_))),
            "sensitive intents should carry a resumable browser continuation"
        );
        assert!(
            actions
                .iter()
                .skip(2)
                .any(|action| matches!(action, PilotAction::Perceive)),
            "resumed sensitive intents should perceive the authorized page"
        );
    }

    #[cfg(feature = "xilem-shell")]
    #[test]
    fn pilot_action_plan_maps_safe_intents_to_browser_actions() {
        let actions = pilot_action_plan("open example.com and distill").unwrap();
        assert!(matches!(
            actions.first(),
            Some(PilotAction::Navigate(url)) if url.as_str() == "https://example.com/"
        ));
        assert!(actions
            .iter()
            .any(|action| matches!(action, PilotAction::Distill)));
        assert!(actions
            .iter()
            .any(|action| matches!(action, PilotAction::Analyze(_))));
    }

    #[test]
    fn browser_shell_can_deny_pending_pilot_consent() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-deny-consent-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

        app.run_native_intent("intent: buy example.com and checkout");
        assert_eq!(app.pilot_status, "AWAITING CONSENT");
        assert!(app.pending_consent.is_some());

        app.deny_pilot_consent();
        assert_eq!(app.pilot_status, "CONSENT DENIED");
        assert!(app.pending_consent.is_none());
        app.refresh_logs();
        assert!(app
            .recent_logs
            .iter()
            .any(|entry| entry.intent.starts_with("pilot consent deny ")));

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn browser_shell_can_authorize_pending_pilot_consent() -> Result<(), String> {
        let data_dir = env::temp_dir().join(format!(
            "sextant-browser-authorize-consent-{}",
            Uuid::new_v4()
        ));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

        app.pending_consent = Some(PendingConsent {
            intent: "buy example.com and checkout".to_string(),
            message: "Authorize this browser action before execution.".to_string(),
            #[cfg(feature = "xilem-shell")]
            remaining_actions: Vec::new(),
        });
        app.pilot_status = "AWAITING CONSENT".to_string();
        assert_eq!(app.pilot_status, "AWAITING CONSENT");

        app.authorize_pilot_consent();
        assert_eq!(app.pilot_status, "CONSENT AUTHORIZED");
        assert!(app.pending_consent.is_none());
        app.refresh_logs();
        assert!(app.recent_logs.iter().any(|entry| {
            entry.intent.starts_with("pilot consent authorize ")
                && entry.consent_signature.is_some()
        }));

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn visible_consent_controls_authorize_pending_request() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-click-consent-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.layout(PhysicalSize::new(1180, 760));

        app.pending_consent = Some(PendingConsent {
            intent: "buy example.com and checkout".to_string(),
            message: "Authorize this browser action before execution.".to_string(),
            #[cfg(feature = "xilem-shell")]
            remaining_actions: Vec::new(),
        });
        app.pilot_status = "AWAITING CONSENT".to_string();
        let authorize_rect = app.consent_authorize_rect;
        click_rect(&mut app, authorize_rect, "authorize consent")?;

        assert_eq!(app.pilot_status, "CONSENT AUTHORIZED");
        assert!(app.pending_consent.is_none());

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[cfg(feature = "xilem-shell")]
    #[test]
    fn authorizing_consent_resumes_pending_pilot_actions() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-resume-consent-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.pending_consent = Some(PendingConsent {
            intent: "buy example.com and checkout".to_string(),
            message: "Authorize this browser action before execution.".to_string(),
            remaining_actions: vec![PilotAction::Analyze(
                "Resumed gated browser plan.".to_string(),
            )],
        });
        app.pilot_status = "AWAITING CONSENT".to_string();

        app.authorize_pilot_consent();

        assert_eq!(app.pilot_status, "PILOT COMPLETE");
        assert!(app.pending_consent.is_none());
        assert_eq!(app.pilot_result, "Resumed gated browser plan.");
        app.refresh_logs();
        assert!(app
            .recent_logs
            .iter()
            .any(|entry| entry.intent.starts_with("pilot consent resume ")
                && entry.consent_signature.is_some()));

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn maps_browser_chrome_shortcuts() {
        let control = ModifiersState::CONTROL;
        let alt = ModifiersState::ALT;

        assert_eq!(
            browser_shortcut_for_key(control, &Key::Character("l".into())),
            Some(BrowserShortcut::FocusAddress)
        );
        assert_eq!(
            browser_shortcut_for_key(control, &Key::Character("T".into())),
            Some(BrowserShortcut::NewTab)
        );
        assert_eq!(
            browser_shortcut_for_key(control, &Key::Character("w".into())),
            Some(BrowserShortcut::CloseTab)
        );
        assert_eq!(
            browser_shortcut_for_key(control, &Key::Character("r".into())),
            Some(BrowserShortcut::Reload)
        );
        assert_eq!(
            browser_shortcut_for_key(alt, &Key::Named(NamedKey::ArrowLeft)),
            Some(BrowserShortcut::Back)
        );
        assert_eq!(
            browser_shortcut_for_key(alt, &Key::Named(NamedKey::ArrowRight)),
            Some(BrowserShortcut::Forward)
        );
        assert_eq!(
            browser_shortcut_for_key(control | alt, &Key::Character("l".into())),
            None
        );
    }

    #[test]
    fn page_tab_strip_keeps_active_overflow_tab_visible() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-tab-overflow-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        let first_tab = app.active_tab().expect("initial tab").id;

        for _ in 0..MAX_VISIBLE_PAGE_TABS {
            app.new_tab();
        }
        app.layout(PhysicalSize::new(1180, 760));

        let active_tab = app.active_tab().expect("active tab").id;
        assert!(
            app.page_tab_rects
                .iter()
                .any(|region| region.tab_id == active_tab),
            "new active tab should stay visible when the strip overflows"
        );
        assert!(
            !app.page_tab_rects
                .iter()
                .any(|region| region.tab_id == first_tab),
            "overflowed strip should window around the active tab"
        );

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn page_tab_strip_pager_reaches_hidden_tabs() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-tab-pager-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        let first_tab = app.active_tab().expect("initial tab").id;

        for _ in 0..(MAX_VISIBLE_PAGE_TABS + 2) {
            app.new_tab();
        }
        app.layout(PhysicalSize::new(1180, 760));
        assert!(
            !app.page_tab_rects
                .iter()
                .any(|region| region.tab_id == first_tab),
            "first tab should start outside the active overflow window"
        );

        while app.can_page_tabs_previous() {
            let previous_rect = app.page_tab_prev_rect;
            click_rect(&mut app, previous_rect, "previous tab pager")?;
        }

        assert!(
            app.page_tab_rects
                .iter()
                .any(|region| region.tab_id == first_tab),
            "previous pager should make the hidden first tab visible"
        );

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn page_tab_range_label_does_not_overlap_controls() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-tab-label-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

        for _ in 0..(MAX_VISIBLE_PAGE_TABS + 2) {
            app.new_tab();
        }
        app.layout(PhysicalSize::new(2400, 900));
        let (label_rect, _) = page_tab_range_label_rect(&app)
            .ok_or_else(|| "wide overflow strip should have room for a range label".to_string())?;

        assert!(
            !label_rect.intersects(app.page_tab_next_rect),
            "range label should not overlap the next pager"
        );
        for region in &app.page_tab_rects {
            assert!(
                !label_rect.intersects(region.rect),
                "range label should not overlap visible tab labels"
            );
        }

        app.layout(PhysicalSize::new(900, 620));
        if let Some((narrow_label_rect, _)) = page_tab_range_label_rect(&app) {
            assert!(
                !narrow_label_rect.intersects(app.page_tab_next_rect),
                "narrow range label should not overlap the next pager"
            );
            for region in &app.page_tab_rects {
                assert!(
                    !narrow_label_rect.intersects(region.rect),
                    "narrow range label should not overlap visible tabs"
                );
            }
        }

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn records_perf_events_and_caps_history() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-perf-history-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

        for millis in 0..(PERF_HISTORY_LIMIT + 2) {
            app.record_perf("frame", Duration::from_millis(millis as u64), "capture");
        }

        assert_eq!(app.perf_events.len(), PERF_HISTORY_LIMIT);
        assert_eq!(
            app.perf_events.first().map(|event| event.duration),
            Some(Duration::from_millis(2))
        );
        assert_eq!(
            app.perf.frame,
            Some(Duration::from_millis((PERF_HISTORY_LIMIT + 1) as u64))
        );

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn slowest_perf_event_tracks_largest_duration() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-perf-slowest-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

        app.record_perf("navigation", Duration::from_millis(100), "open");
        app.record_perf("frame", Duration::from_millis(20), "capture");
        app.record_perf("distill", Duration::from_millis(300), "active tab");

        let slowest = app
            .slowest_perf_event()
            .ok_or_else(|| "missing slowest perf event".to_string())?;
        assert_eq!(slowest.phase, "distill");
        assert_eq!(slowest.duration, Duration::from_millis(300));

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn perf_tab_switches_to_performance_view() -> Result<(), String> {
        let data_dir = env::temp_dir().join(format!("sextant-browser-perf-tab-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.layout(PhysicalSize::new(1180, 760));

        let perf_rect = app.perf_tab_rect;
        click_rect(&mut app, perf_rect, "Perf tab")?;
        assert!(app.main_view == MainView::Perf);

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn visible_navigation_can_defer_user_open() -> Result<(), String> {
        let data_dir = env::temp_dir().join(format!(
            "sextant-browser-deferred-navigation-{}",
            Uuid::new_v4()
        ));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.defer_user_navigation = true;
        app.address_input = "https://example.com".to_string();

        app.navigate_input();

        assert_eq!(
            app.pending_user_navigation.as_deref(),
            Some("https://example.com")
        );
        assert!(app.last_status.contains("Opening https://example.com"));
        assert!(app.last_ok);

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn visible_navigation_can_defer_user_reload() -> Result<(), String> {
        let data_dir = env::temp_dir().join(format!(
            "sextant-browser-deferred-reload-{}",
            Uuid::new_v4()
        ));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.defer_user_navigation = true;

        app.run_action(Action::Reload);

        assert!(matches!(app.pending_user_action, Some(Action::Reload)));
        assert_eq!(app.last_status, "Reloading active tab...");
        assert!(app.last_ok);

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn visible_navigation_can_defer_user_distill() -> Result<(), String> {
        let data_dir = env::temp_dir().join(format!(
            "sextant-browser-deferred-distill-{}",
            Uuid::new_v4()
        ));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.defer_user_navigation = true;

        app.run_action(Action::DistillActive);

        assert!(matches!(
            app.pending_user_action,
            Some(Action::DistillActive)
        ));
        assert_eq!(app.last_status, "Distilling active page...");
        assert!(app.last_ok);

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn viewport_wheel_queues_frame_warmup() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-wheel-warmup-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.layout(PhysicalSize::new(1180, 760));
        app.navigate_to(
            Url::parse("data:text/html,<body style='height:2000px'>wheel</body>")
                .map_err(|error| error.to_string())?,
        );
        assert!(app.last_ok);
        app.latest_frame = Some(RenderedFrame {
            width: 2,
            height: 2,
            pixels: vec![0; 4],
        });
        app.frame_refresh_budget = 0;
        app.frame_dirty = false;
        let (x, y) = rect_center(app.browser_viewport_rect);

        app.scroll_at(
            x,
            y,
            &MouseScrollDelta::LineDelta(0.0, -1.0),
            app.window_size,
        );

        assert!(app.validation.browser_input_seen);
        assert!(app.frame_dirty);
        assert!(app.frame_refresh_budget > 0);

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn perception_tab_switches_to_perception_view() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-sense-tab-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.layout(PhysicalSize::new(1180, 760));

        let sense_rect = app.perception_tab_rect;
        click_rect(&mut app, sense_rect, "Sense tab")?;
        assert!(app.main_view == MainView::Perception);

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn guard_tab_switches_to_guard_view() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-guard-tab-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        app.layout(PhysicalSize::new(1180, 760));

        let guard_rect = app.guard_tab_rect;
        click_rect(&mut app, guard_rect, "Guard tab")?;
        assert!(app.main_view == MainView::Guard);

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn guard_report_includes_trust_boundary_lines() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-guard-report-{}", Uuid::new_v4()));
        let app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        let url = Url::parse("https://example.com").unwrap();

        let lines = guard_report_lines(&app, Some(&url), None);

        assert!(lines.iter().any(|line| line.starts_with("guard persona:")));
        assert!(lines.iter().any(|line| line.starts_with("airgap:")));
        assert!(lines.iter().any(|line| line.starts_with("firewall:")));
        assert!(lines.iter().any(|line| line.starts_with("privacy:")));

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn guard_decision_blocks_blacklisted_navigation() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-guard-block-{}", Uuid::new_v4()));
        let app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        let url = Url::parse("https://malicious-site.net/path").unwrap();

        let decision = app.guard_decision(&url);

        assert!(!decision.allowed);
        assert_eq!(decision.action, "BLOCK");
        assert!(decision.reason.contains("Global blacklist"));

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[cfg(feature = "xilem-shell")]
    #[test]
    fn guard_decision_uses_data_dir_policy_overlay() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-guard-policy-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
        std::fs::write(
            data_dir.join("guard-policy.json"),
            r#"{
                "persona_rules": {
                    "browser-persona": [
                        {
                            "domain_pattern": "example.com",
                            "action": "Block",
                            "reason": "operator QA policy"
                        }
                    ]
                }
            }"#,
        )
        .map_err(|error| error.to_string())?;
        let app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        let url = Url::parse("https://example.com").unwrap();

        let decision = app.guard_decision(&url);
        let lines = guard_report_lines(&app, Some(&url), None);

        assert!(!decision.allowed);
        assert_eq!(decision.action, "BLOCK");
        assert_eq!(decision.reason, "operator QA policy");
        assert!(lines
            .iter()
            .any(|line| line.contains("guard policy: overlay")));
        assert!(lines
            .iter()
            .any(|line| line.contains("guard rules: persona=1")));

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn navigate_to_stops_at_local_guard_block() -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-guard-navigate-{}", Uuid::new_v4()));
        let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
        let url = Url::parse("https://malicious-site.net/path").unwrap();

        app.navigate_to(url);

        assert!(!app.last_ok);
        assert!(app.last_status.contains("Local guard blocked navigate"));
        assert!(app.latest_frame.is_none());

        let _ = std::fs::remove_dir_all(data_dir);
        Ok(())
    }

    #[test]
    fn guard_probe_reports_policy_block_as_probe_result() -> Result<(), String> {
        let report = run_guard_probe("https://malicious-site.net/path")?;

        assert!(report.iter().any(|line| line.contains("firewall: BLOCK")));
        assert!(report
            .iter()
            .any(|line| line.contains("browser guard probe blocked navigation")));

        Ok(())
    }

    #[test]
    fn perception_summary_classifies_interactive_pages() {
        let page = sextant_engine::DistilledPage {
            title: "Form".to_string(),
            url: Url::parse("https://example.com/form").unwrap(),
            content: "Form page".to_string(),
            semantic_map: vec![
                sextant_engine::SemanticNode {
                    id: "h1".to_string(),
                    node_type: NodeType::Heading,
                    text: "Form".to_string(),
                    selector: "h1".to_string(),
                    attributes: Default::default(),
                },
                sextant_engine::SemanticNode {
                    id: "input-q".to_string(),
                    node_type: NodeType::Input,
                    text: "Search".to_string(),
                    selector: "input[name=q]".to_string(),
                    attributes: Default::default(),
                },
            ],
            metadata: Default::default(),
        };

        assert!(page_perception_summary(&page).contains("interactive form page"));
        assert_eq!(semantic_counts(&page).inputs, 1);
    }
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

fn browser_shortcut_for_key(modifiers: ModifiersState, key: &Key) -> Option<BrowserShortcut> {
    if modifiers.control_key() && !modifiers.alt_key() {
        let Key::Character(value) = key else {
            return None;
        };
        return match value.to_ascii_lowercase().as_str() {
            "l" => Some(BrowserShortcut::FocusAddress),
            "t" => Some(BrowserShortcut::NewTab),
            "w" => Some(BrowserShortcut::CloseTab),
            "r" => Some(BrowserShortcut::Reload),
            _ => None,
        };
    }
    if modifiers.alt_key() && !modifiers.control_key() {
        return match key {
            Key::Named(NamedKey::ArrowLeft) => Some(BrowserShortcut::Back),
            Key::Named(NamedKey::ArrowRight) => Some(BrowserShortcut::Forward),
            _ => None,
        };
    }
    None
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

fn fmt_duration(duration: Option<Duration>) -> String {
    duration
        .map(format_duration)
        .unwrap_or_else(|| "pending".to_string())
}

fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis >= 1000 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        format!("{millis}ms")
    }
}

fn page_tab_label(tab: &Tab) -> String {
    if let Some(page) = tab.distilled_page.as_ref() {
        if !page.title.trim().is_empty() {
            return page.title.trim().to_string();
        }
    }
    if let Some(url) = tab.url.as_ref() {
        if url.scheme() == "data" {
            return "data page".to_string();
        }
        if let Some(domain) = url.domain() {
            return domain.trim_start_matches("www.").to_string();
        }
        return short_url(url);
    }
    format!("New tab {}", short_id(tab.id))
}

fn backend(status: EngineStatus) -> &'static str {
    match status.active_backend {
        EngineBackend::Servo => "Servo",
        EngineBackend::Gecko => "Gecko",
        EngineBackend::Chromium => "Chromium",
    }
}

fn display_backend(app: &BrowserApp, tab: &Tab) -> &'static str {
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

fn is_deferred_user_navigation_action(action: Action) -> bool {
    matches!(
        action,
        Action::Back | Action::Forward | Action::Reload | Action::DistillActive
    )
}

fn deferred_user_navigation_status(action: Action) -> &'static str {
    match action {
        Action::Back => "Going back...",
        Action::DistillActive => "Distilling active page...",
        Action::Forward => "Going forward...",
        Action::Reload => "Reloading active tab...",
        _ => "Opening...",
    }
}

fn deferred_user_navigation_label(action: Action) -> &'static str {
    match action {
        Action::Back => "back",
        Action::DistillActive => "distill",
        Action::Forward => "forward",
        Action::Reload => "reload",
        _ => "navigation",
    }
}

fn draw(
    window: &Window,
    surface: &mut Surface<Arc<Window>, Arc<Window>>,
    surface_size: &mut PhysicalSize<u32>,
    app: &BrowserApp,
) -> Result<DrawStats, String> {
    let draw_started = Instant::now();
    let size = window.inner_size();
    let width = size.width.max(1);
    let height = size.height.max(1);
    if surface_size.width != width || surface_size.height != height {
        surface
            .resize(
                NonZeroU32::new(width).ok_or("invalid width")?,
                NonZeroU32::new(height).ok_or("invalid height")?,
            )
            .map_err(|e| e.to_string())?;
        *surface_size = PhysicalSize::new(width, height);
    }

    let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
    for pixel in buffer.iter_mut() {
        *pixel = BG;
    }

    draw_top_bar(&mut buffer, width, height, app);
    draw_controls(&mut buffer, width, height, app);
    draw_page_tab_strip(&mut buffer, width, height, app);
    draw_metric_cards(&mut buffer, width, height, app);
    let frame_blit = match app.main_view {
        MainView::Browser => draw_page_panel(&mut buffer, width, height, app),
        MainView::Wake => {
            draw_wake_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Log => {
            draw_log_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Guard => {
            draw_guard_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Perception => {
            draw_perception_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Perf => {
            draw_perf_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Validation => {
            draw_validation_panel(&mut buffer, width, height, app);
            None
        }
    };
    draw_ai_rail(&mut buffer, width, height, app);
    draw_status_bar(&mut buffer, width, height, app);

    let present_started = Instant::now();
    buffer.present().map_err(|e| e.to_string())?;
    Ok(DrawStats {
        total: draw_started.elapsed(),
        frame_blit,
        present: present_started.elapsed(),
    })
}

fn draw_top_bar(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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
    draw_text(buffer, width, height, 24, 34, "SERVO", TEXT_DIM, 1);
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

fn draw_controls(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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
        "LOG",
        app.main_view == MainView::Log,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.guard_tab_rect,
        "GUARD",
        app.main_view == MainView::Guard,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.perception_tab_rect,
        "SENSE",
        app.main_view == MainView::Perception,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.perf_tab_rect,
        "PERF",
        app.main_view == MainView::Perf,
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

fn draw_page_tab_strip(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let strip = Rect {
        x: 0,
        y: CHROME_H + STRIP_H + TAB_H,
        w: rail_x,
        h: PAGE_TAB_H,
    };
    fill_rect(buffer, width, height, strip, PANEL_DARK);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: strip.y + strip.h.saturating_sub(1),
            w: rail_x,
            h: 1,
        },
        BORDER,
    );

    let tabs = app.engine.get_tabs();
    let active_id = app.active_tab().map(|tab| tab.id);
    if tabs.len() > app.page_tab_rects.len() {
        draw_page_tab_pager(
            buffer,
            width,
            height,
            app.page_tab_prev_rect,
            "<",
            app.can_page_tabs_previous(),
            app.cursor
                .map(|(x, y)| app.page_tab_prev_rect.contains(x, y))
                .unwrap_or(false),
        );
        draw_page_tab_pager(
            buffer,
            width,
            height,
            app.page_tab_next_rect,
            ">",
            app.can_page_tabs_next(),
            app.cursor
                .map(|(x, y)| app.page_tab_next_rect.contains(x, y))
                .unwrap_or(false),
        );
    }
    for region in &app.page_tab_rects {
        let Some(tab) = tabs.iter().find(|tab| tab.id == region.tab_id) else {
            continue;
        };
        let active = Some(tab.id) == active_id;
        let hovered = app
            .cursor
            .map(|(x, y)| region.rect.contains(x, y))
            .unwrap_or(false);
        draw_page_tab(buffer, width, height, region.rect, tab, active, hovered);
    }

    if tabs.len() > app.page_tab_rects.len() {
        if let Some((rect, label)) = page_tab_range_label_rect(app) {
            draw_text(buffer, width, height, rect.x, rect.y, &label, TEXT_DIM, 1);
        }
    }
}

fn page_tab_range_label_rect(app: &BrowserApp) -> Option<(Rect, String)> {
    let tabs_len = app.engine.get_tabs().len();
    if tabs_len <= app.page_tab_rects.len() || app.page_tab_rects.is_empty() {
        return None;
    }
    let first = app.page_tab_window_start + 1;
    let last = (app.page_tab_window_start + app.page_tab_rects.len()).min(tabs_len);
    let label = format!("{first}-{last}/{tabs_len}");
    let w = text_width(&label, 1);
    let h = char_advance(1);
    let x = app.page_tab_next_rect.x.saturating_sub(w + 8);
    let y = app.page_tab_next_rect.y + 9;
    let rect = Rect { x, y, w, h };
    let minimum_x = app
        .page_tab_rects
        .last()
        .map(|region| {
            region
                .rect
                .x
                .saturating_add(region.rect.w)
                .saturating_add(8)
        })
        .unwrap_or(
            app.page_tab_prev_rect
                .x
                .saturating_add(app.page_tab_prev_rect.w),
        );
    if rect.x < minimum_x || rect.intersects(app.page_tab_next_rect) {
        return None;
    }
    Some((rect, label))
}

fn draw_page_tab_pager(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    enabled: bool,
    hovered: bool,
) {
    let fill = if enabled && hovered {
        FIELD_FOCUS
    } else {
        FIELD
    };
    fill_rect(buffer, width, height, rect, fill);
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if enabled { BORDER } else { BUTTON_DISABLED },
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 9,
        rect.y + 9,
        label,
        if enabled { TEXT } else { TEXT_DIM },
        1,
    );
}

fn draw_page_tab(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    tab: &Tab,
    active: bool,
    hovered: bool,
) {
    let fill = if active {
        PANEL_ALT
    } else if hovered {
        FIELD_FOCUS
    } else {
        FIELD
    };
    fill_rect(buffer, width, height, rect, fill);
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if active { BUTTON_ACTIVE } else { BORDER },
    );
    let marker = if active { STATUS_OK } else { TEXT_DIM };
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x + 8,
            y: rect.y + 8,
            w: 6,
            h: 6,
        },
        marker,
    );
    let label = page_tab_label(tab);
    let max_chars = ((rect.w.saturating_sub(28)) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        rect.x + 20,
        rect.y + 9,
        &truncate(&label, max_chars),
        if active { TEXT } else { TEXT_DIM },
        1,
    );
}

fn draw_metric_cards(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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

fn draw_page_panel(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    app: &BrowserApp,
) -> Option<Duration> {
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
            return Some(draw_rendered_frame(buffer, width, height, panel, frame));
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
    None
}

fn draw_wake_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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
) -> Duration {
    let started = Instant::now();
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
        return started.elapsed();
    }
    let expected_pixels = frame.width as usize * frame.height as usize;
    if frame.pixels.len() < expected_pixels {
        draw_text(
            buffer,
            width,
            height,
            viewport.x + 12,
            viewport.y + 12,
            "SERVO FRAME IS TRUNCATED",
            STATUS_WARN,
            1,
        );
        return started.elapsed();
    }

    let scale_x = viewport.w as f32 / frame.width as f32;
    let scale_y = viewport.h as f32 / frame.height as f32;
    let scale = scale_x.min(scale_y).max(0.01);
    let draw_w = (frame.width as f32 * scale).max(1.0) as u32;
    let draw_h = (frame.height as f32 * scale).max(1.0) as u32;
    let offset_x = viewport.x + viewport.w.saturating_sub(draw_w) / 2;
    let offset_y = viewport.y + viewport.h.saturating_sub(draw_h) / 2;

    let copy_w = draw_w.min(viewport.w).min(width.saturating_sub(offset_x));
    let copy_h = draw_h.min(viewport.h).min(height.saturating_sub(offset_y));
    if copy_w > 0 && copy_h > 0 {
        let x_step = ((frame.width as u64) << 32) / draw_w.max(1) as u64;
        let y_step = ((frame.height as u64) << 32) / draw_h.max(1) as u64;
        for dy in 0..copy_h {
            let src_y = (((dy as u64 * y_step) >> 32) as u32).min(frame.height - 1);
            let src_row_start = src_y as usize * frame.width as usize;
            let dest_y = offset_y + dy;
            let dest_start = dest_y as usize * width as usize + offset_x as usize;
            let dest_end = dest_start + copy_w as usize;
            if let Some(dest_row) = buffer.get_mut(dest_start..dest_end) {
                for dx in 0..copy_w {
                    let src_x = (((dx as u64 * x_step) >> 32) as u32).min(frame.width - 1);
                    dest_row[dx as usize] = frame.pixels[src_row_start + src_x as usize];
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
    started.elapsed()
}

fn draw_log_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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

fn draw_guard_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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
        "LOCAL GUARD",
        TEXT,
        1,
    );

    let active_url = app
        .active_tab()
        .and_then(|tab| tab.url.as_ref())
        .or_else(|| {
            app.active_tab()
                .and_then(|tab| tab.distilled_page.as_ref())
                .map(|page| &page.url)
        });
    let active_page = app.active_tab().and_then(|tab| tab.distilled_page.as_ref());
    let lines = guard_report_lines(app, active_url, active_page);
    let max_chars = ((panel.w.saturating_sub(40)) / char_advance(1)) as usize;
    let max_rows = panel.h.saturating_sub(58) / 26;
    for (index, line) in lines.iter().take(max_rows as usize).enumerate() {
        let y = panel.y + 54 + index as u32 * 26;
        let color = if line.contains("BLOCK") || line.contains("HARDENED") {
            STATUS_WARN
        } else if line.contains("ALLOW") || line.contains("ONLINE") {
            STATUS_OK
        } else {
            TEXT_DIM
        };
        draw_text(
            buffer,
            width,
            height,
            44,
            y,
            &truncate(line, max_chars),
            color,
            1,
        );
    }
}

fn draw_perception_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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
        "PAGE PERCEPTION",
        TEXT,
        1,
    );

    let Some(page) = app.active_tab().and_then(|tab| tab.distilled_page.as_ref()) else {
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 54,
            "NO DISTILLED PAGE YET. DISTILL THE ACTIVE TAB TO BUILD A SEMANTIC MAP.",
            TEXT_DIM,
            1,
        );
        return;
    };

    let max_chars = ((panel.w.saturating_sub(40)) / char_advance(1)) as usize;
    let counts = semantic_counts(page);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 46,
        &truncate(&page.title, max_chars),
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 70,
        &truncate(page.url.as_str(), max_chars),
        TEXT_DIM,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 98,
        &truncate(&page_perception_summary(page), max_chars),
        STATUS_OK,
        1,
    );
    let counts_line = format!(
        "SEMANTICS H={} LINKS={} BUTTONS={} INPUTS={} IMAGES={} TEXT={}",
        counts.headings, counts.links, counts.buttons, counts.inputs, counts.images, counts.text
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 126,
        &truncate(&counts_line, max_chars),
        TEXT_DIM,
        1,
    );
    let source = page
        .metadata
        .get("distillation_backend")
        .or_else(|| page.metadata.get("source"))
        .or_else(|| page.metadata.get("distiller"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 150,
        &truncate(&format!("SOURCE {}", source), max_chars),
        TEXT_DIM,
        1,
    );

    let header_y = panel.y + 190;
    draw_text(buffer, width, height, 44, header_y, "TYPE", TEXT, 1);
    draw_text(buffer, width, height, 132, header_y, "SELECTOR", TEXT, 1);
    draw_text(buffer, width, height, 362, header_y, "TEXT", TEXT, 1);

    let row_start = header_y + 28;
    let max_rows = panel.y.saturating_add(panel.h).saturating_sub(row_start) / 28;
    let selector_chars = 28;
    let text_chars = ((panel.w.saturating_sub(370)) / char_advance(1)) as usize;
    for (index, node) in key_semantic_nodes(page)
        .into_iter()
        .take(max_rows as usize)
        .enumerate()
    {
        let y = row_start + index as u32 * 28;
        let color = match node.node_type {
            NodeType::Heading => TEXT,
            NodeType::Input | NodeType::Button => STATUS_WARN,
            NodeType::Link => BUTTON_BRIGHT,
            NodeType::Image | NodeType::Text => TEXT_DIM,
        };
        draw_text(
            buffer,
            width,
            height,
            44,
            y,
            semantic_node_type_label(&node.node_type),
            color,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            132,
            y,
            &truncate(&node.selector, selector_chars),
            TEXT_DIM,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            362,
            y,
            &truncate(&node.text, text_chars),
            color,
            1,
        );
    }
}

fn draw_perf_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    fill_rect(buffer, width, height, panel, PANEL_ALT);
    stroke_rect(buffer, width, height, panel, BORDER);

    let max_chars = ((panel.w.saturating_sub(40)) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "BROWSER PERFORMANCE",
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 46,
        &truncate(&format!("LATEST {}", app.perf.summary()), max_chars),
        TEXT_DIM,
        1,
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
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 72,
        &truncate(&slowest, max_chars),
        STATUS_WARN,
        1,
    );

    let header_y = panel.y + 112;
    draw_text(buffer, width, height, 44, header_y, "PHASE", TEXT, 1);
    draw_text(buffer, width, height, 176, header_y, "TIME", TEXT, 1);
    draw_text(buffer, width, height, 276, header_y, "DETAIL", TEXT, 1);

    if app.perf_events.is_empty() {
        draw_text(
            buffer,
            width,
            height,
            44,
            header_y + 34,
            "NO PERFORMANCE EVENTS YET. OPEN, DISTILL, OR CAPTURE A FRAME.",
            TEXT_DIM,
            1,
        );
        return;
    }

    let row_start = header_y + 34;
    let max_rows = panel.y.saturating_add(panel.h).saturating_sub(row_start) / 24;
    let detail_chars = ((panel.w.saturating_sub(320)) / char_advance(1)) as usize;
    let slowest_event = app.slowest_perf_event();
    for (index, event) in app
        .perf_events
        .iter()
        .rev()
        .take(max_rows as usize)
        .enumerate()
    {
        let y = row_start + index as u32 * 24;
        let color = if slowest_event
            .map(|slow| slow.phase == event.phase && slow.duration == event.duration)
            .unwrap_or(false)
        {
            STATUS_WARN
        } else {
            TEXT_DIM
        };
        draw_text(buffer, width, height, 44, y, event.phase, color, 1);
        draw_text(
            buffer,
            width,
            height,
            176,
            y,
            &format_duration(event.duration),
            color,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            276,
            y,
            &truncate(&event.label, detail_chars),
            color,
            1,
        );
    }
}

fn draw_validation_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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
        "NATIVE BROWSER VALIDATION",
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
        if app.showcase_report.is_empty() {
            "THIS VIEW TRACKS THE CURRENT WINDOW SESSION, NOT ONLY OPERATOR AUTOMATION."
        } else {
            "BROWSER PROOF IS ACTIVE. RECENT PASSED STEPS ARE LISTED BELOW."
        },
        TEXT_DIM,
        1,
    );

    let mut row_start = panel.y + 112;
    if !app.showcase_report.is_empty() {
        let report_rect = Rect {
            x: 44,
            y: panel.y + 98,
            w: panel.w.saturating_sub(88),
            h: 138,
        };
        fill_rect(buffer, width, height, report_rect, PANEL_DARK);
        stroke_rect(buffer, width, height, report_rect, BORDER);
        draw_text(
            buffer,
            width,
            height,
            report_rect.x + 14,
            report_rect.y + 12,
            proof_report_title(&app.showcase_report),
            TEXT,
            1,
        );
        for (index, line) in app
            .showcase_report
            .iter()
            .filter(|line| !line.starts_with("isolated data dir"))
            .take(6)
            .enumerate()
        {
            let y = report_rect.y + 36 + index as u32 * 16;
            fill_rect(
                buffer,
                width,
                height,
                Rect {
                    x: report_rect.x + 14,
                    y: y + 2,
                    w: 6,
                    h: 6,
                },
                STATUS_OK,
            );
            draw_text(
                buffer,
                width,
                height,
                report_rect.x + 28,
                y,
                &truncate(
                    line,
                    ((report_rect.w.saturating_sub(44)) / char_advance(1)) as usize,
                ),
                TEXT_DIM,
                1,
            );
        }
        row_start = report_rect.y + report_rect.h + 24;
    }

    let max_detail_chars = ((panel.w.saturating_sub(260)) / char_advance(1)) as usize;
    let max_rows = panel.y.saturating_add(panel.h).saturating_sub(row_start) / 38;
    for (index, row) in rows.iter().take(max_rows as usize).enumerate() {
        let y = row_start + index as u32 * 38;
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

fn proof_report_title(report: &[String]) -> &'static str {
    if report
        .iter()
        .any(|line| line.to_ascii_lowercase().contains("real browsing"))
    {
        "REAL BROWSING"
    } else if report
        .iter()
        .any(|line| line.to_ascii_lowercase().contains("shell interaction"))
    {
        "SHELL SMOKE"
    } else {
        "LAUNCH SHOWCASE"
    }
}

fn draw_ai_rail(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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
        "CONTEXT VAULT",
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        118,
        &format!("PILOT {}", truncate(&app.pilot_status, 18)),
        if app.pilot_status == "FAILED" {
            STATUS_WARN
        } else {
            STATUS_OK
        },
        1,
    );
    let active_url = app
        .active_tab()
        .and_then(|tab| tab.url.as_ref())
        .map(short_url)
        .unwrap_or_else(|| "NO ACTIVE PAGE".to_string());
    let page_state = if app.last_intent.is_empty() {
        app.active_tab()
            .and_then(|tab| tab.distilled_page.as_ref())
            .map(|page| format!("I HAVE DISTILLED '{}'.", page.title))
            .unwrap_or_else(|| format!("TRACKING {}. DISTILL RUNS REAL HTTP FETCH.", active_url))
    } else {
        format!("INTENT: {}", truncate(&app.last_intent, 70))
    };
    let plan_summary = if app.pilot_plan.is_empty() {
        "NEXT: OPEN A PAGE OR ENTER A NATIVE INTENT.".to_string()
    } else {
        format!("PLAN: {}", truncate(&app.pilot_plan.join(" -> "), 72))
    };
    let next_step = if app.last_intent.is_empty() && app.active_tab().is_none() {
        "OPEN A PAGE TO START THE BROWSER LOOP.".to_string()
    } else if app.last_intent.is_empty()
        && app
            .active_tab()
            .and_then(|tab| tab.distilled_page.as_ref())
            .is_none()
    {
        "NEXT: DISTILL THE PAGE INTO WAKE.".to_string()
    } else if app.wake_results.is_empty() {
        plan_summary
    } else {
        format!("RESULT: {}", truncate(&app.pilot_result, 72))
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
        &next_step,
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

    if let Some(pending) = app.pending_consent.as_ref() {
        draw_text(
            buffer,
            width,
            height,
            rail_x + 20,
            444,
            "CAPTAIN'S KEY",
            TEXT_DIM,
            1,
        );
        let authorize_hovered = app
            .cursor
            .map(|(x, y)| app.consent_authorize_rect.contains(x, y))
            .unwrap_or(false);
        let deny_hovered = app
            .cursor
            .map(|(x, y)| app.consent_deny_rect.contains(x, y))
            .unwrap_or(false);
        draw_button(
            buffer,
            width,
            height,
            app.consent_authorize_rect,
            "AUTHORIZE",
            authorize_hovered,
            true,
        );
        draw_button(
            buffer,
            width,
            height,
            app.consent_deny_rect,
            "DENY",
            deny_hovered,
            true,
        );
        draw_text(
            buffer,
            width,
            height,
            rail_x + 20,
            496,
            &truncate(&pending.message, 42),
            TEXT_DIM,
            1,
        );
    }

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

fn draw_status_bar(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
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
        "RENDER {} {}    {}    WAKE RESULTS {}    LOG ENTRIES {}    VALIDATION {}/{}    PRIVACY LOCAL",
        backend,
        render_mode_label(),
        app.perf.summary(),
        app.wake_results.len(),
        app.recent_logs.len(),
        app.validation_pass_count(),
        validation_total
    );
    let max_status_chars = (width.saturating_sub(172) / char_advance(1)).max(1) as usize;
    draw_text(
        buffer,
        width,
        height,
        24,
        y + 14,
        &truncate(&status, max_status_chars),
        TEXT_DIM,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        width.saturating_sub(124),
        y + 14,
        "SERVO EDGE",
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
        Action::Navigate => "Enter an address, search phrase, or intent first.",
        Action::RunShowcase => "Showcase is always available.",
        Action::NewTab => "New tab is always available.",
        Action::Back => "Back is unavailable for this tab/backend.",
        Action::Forward => "Forward is unavailable for this tab/backend.",
        Action::Reload => "Active tab has no URL to reload.",
        Action::CloseTab => "No active tab to close.",
        Action::ResetValidation => "Validation reset is always available.",
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
    buttons: usize,
    inputs: usize,
    images: usize,
    text: usize,
}

fn semantic_counts(page: &sextant_engine::DistilledPage) -> SemanticCounts {
    let mut counts = SemanticCounts {
        headings: 0,
        links: 0,
        buttons: 0,
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
            NodeType::Button => counts.buttons += 1,
        }
    }
    counts
}

fn page_perception_summary(page: &sextant_engine::DistilledPage) -> String {
    let counts = semantic_counts(page);
    let kind = if counts.inputs > 0 {
        "interactive form page"
    } else if counts.links >= 20 {
        "navigation-rich reference page"
    } else if counts.headings >= 3 && counts.text >= 8 {
        "structured article page"
    } else if counts.images > counts.text && counts.images > 0 {
        "media-heavy page"
    } else if counts.links > 0 {
        "simple linked document"
    } else {
        "simple document"
    };
    format!(
        "PERCEPTION {} with {} semantic node(s), {} link(s), {} input(s), {} heading(s)",
        kind,
        page.semantic_map.len(),
        counts.links,
        counts.inputs,
        counts.headings
    )
}

fn key_semantic_nodes(page: &sextant_engine::DistilledPage) -> Vec<&sextant_engine::SemanticNode> {
    let mut nodes = page
        .semantic_map
        .iter()
        .filter(|node| !node.text.trim().is_empty() || !node.selector.trim().is_empty())
        .collect::<Vec<_>>();
    nodes.sort_by_key(|node| match node.node_type {
        NodeType::Heading => 0,
        NodeType::Input => 1,
        NodeType::Button => 2,
        NodeType::Link => 3,
        NodeType::Image => 4,
        NodeType::Text => 5,
    });
    nodes
}

fn semantic_node_type_label(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::Heading => "HEAD",
        NodeType::Link => "LINK",
        NodeType::Button => "BUTTON",
        NodeType::Input => "INPUT",
        NodeType::Image => "IMAGE",
        NodeType::Text => "TEXT",
    }
}

fn guard_report_lines(
    app: &BrowserApp,
    target_url: Option<&Url>,
    _page: Option<&sextant_engine::DistilledPage>,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("guard persona: {}", app.persona_id));
    #[cfg(feature = "xilem-shell")]
    {
        lines.push(format!("guard policy: {}", app.guard_policy_source));
        lines.push(format!(
            "guard rules: persona={} global_blacklist={}",
            app.guard_firewall.persona_rules(&app.persona_id).len(),
            app.guard_firewall.global_blacklist().len()
        ));
        let airgap = SextantAirGap::new();
        let network = if airgap.check_network_allowed() {
            "ONLINE network allowed"
        } else {
            "ISOLATED network blocked"
        };
        lines.push(format!("airgap: {:?} | {}", airgap.get_status(), network));

        if let Some(url) = target_url {
            let (action, reason) = app.guard_firewall.check_access(&app.persona_id, url);
            lines.push(format!(
                "firewall: {} {} | {}",
                firewall_action_label(&action),
                short_url(url),
                reason
            ));
        } else {
            lines.push("firewall: waiting for an active target URL".to_string());
        }

        let masker = PrivacyMasker::new();
        let sample = _page
            .map(|page| page.content.as_str())
            .unwrap_or("Contact Paul at paul@example.com or 555-123-4567.");
        let redacted = masker.redact_text(sample, &PrivacyLevel::Standard);
        let redacted_changed = redacted != sample;
        lines.push(format!(
            "privacy: STANDARD redaction {}",
            if redacted_changed {
                "active"
            } else {
                "no pii found"
            }
        ));
        lines.push(format!("privacy sample: {}", truncate(&redacted, 96)));
    }
    #[cfg(not(feature = "xilem-shell"))]
    {
        if let Some(url) = target_url {
            lines.push(format!("firewall: reader lane observes {}", short_url(url)));
        } else {
            lines.push("firewall: reader lane waiting for target URL".to_string());
        }
        lines.push("airgap: reader lane, guard crates disabled".to_string());
        lines.push("privacy: reader lane, redaction unavailable".to_string());
    }
    lines
}

#[cfg(feature = "xilem-shell")]
fn firewall_action_label(action: &FirewallAction) -> &'static str {
    match action {
        FirewallAction::Allow => "ALLOW",
        FirewallAction::Block => "BLOCK",
        FirewallAction::Audit => "AUDIT",
        FirewallAction::Isolate => "ISOLATE",
    }
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
