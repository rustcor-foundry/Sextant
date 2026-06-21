use chrono::Utc;
#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
use serde::{Deserialize, Serialize};
#[cfg(feature = "xilem-shell")]
use sextant_airgap::SextantAirGap;
use sextant_engine::{
    AsyncDistillResult, AsyncFrameCapture, AsyncNavigationResult, AsyncNavigationTimings,
    BrowserEvalProbe, BrowserKey, DistilledPage, EngineBackend, EngineStatus, NodeType,
    RenderedFrame, SextantEngine, Tab,
};
#[cfg(feature = "xilem-shell")]
use sextant_firewall::{FirewallAction, SextantFirewall};
#[cfg(feature = "xilem-shell")]
use sextant_inference::InferenceBackend;
use sextant_log::{CaptainsLog, LogEntry, LogStatus};
#[cfg(feature = "xilem-shell")]
use sextant_pilot::PilotAction;
#[cfg(feature = "xilem-shell")]
use sextant_pilot::{LocalBrain, PilotBrain};
#[cfg(feature = "xilem-shell")]
use sextant_privacy::{PrivacyLevel, PrivacyMasker};
#[cfg(feature = "xilem-shell")]
use sextant_vault::CitadelVault;
use sextant_wake::{DigitalWake, WakeEntry};
use softbuffer::{Context, Surface};
use std::collections::VecDeque;
use std::env;
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(any(feature = "xilem-shell", feature = "servo-backend", test))]
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use url::Url;
use uuid::Uuid;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Icon, Window, WindowBuilder};

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::{
        Input::KeyboardAndMouse::{SetActiveWindow, SetFocus},
        WindowsAndMessaging::{
            BringWindowToTop, EnumChildWindows, GetWindowThreadProcessId, SetForegroundWindow,
            SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        },
    },
};

#[cfg(feature = "servo-backend")]
mod direct_servo;

// Palette, layout, and timing constants live in the theme submodule (first cut
// of the browser.rs split). Glob-imported so existing unqualified references
// (BG, PANEL, CHROME_H, ...) keep resolving unchanged.
#[path = "browser/theme.rs"]
mod theme;
use theme::*;
#[path = "browser/draw.rs"]
mod draw;
use draw::*;
#[path = "browser/harness.rs"]
mod harness;
use harness::*;
#[path = "browser/model.rs"]
mod model;
use model::*;

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
#[path = "browser/egui_retro.rs"]
mod egui_retro;
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
use egui_retro::{apply_retro_egui_theme, run_retro_egui_proof};

#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
#[path = "browser/egui_bridge.rs"]
mod egui_bridge;
#[cfg(all(
    target_os = "windows",
    feature = "servo-backend",
    feature = "xilem-shell"
))]
use egui_bridge::run_visible_app_egui;

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
#[path = "browser/egui_hosted.rs"]
mod egui_hosted;
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
use egui_hosted::run_hosted_direct_app_egui;
#[path = "browser/hosted_direct.rs"]
mod hosted_direct;
use hosted_direct::*;

#[path = "browser/lanes.rs"]
mod lanes;
use lanes::PersistenceLane;
#[cfg(feature = "xilem-shell")]
use lanes::PilotBrainLane;

// BrowserApp god-impl, split into cohesive `impl BrowserApp` partitions.
#[path = "browser/appliance_cert.rs"]
mod appliance_cert;
#[path = "browser/frame_refresh.rs"]
mod frame_refresh;
#[path = "browser/guard.rs"]
mod guard;
#[path = "browser/input.rs"]
mod input;
#[path = "browser/intent_pilot.rs"]
mod intent_pilot;
#[path = "browser/navigation.rs"]
mod navigation;
#[path = "browser/page_ops.rs"]
mod page_ops;
#[path = "browser/page_tabs.rs"]
mod page_tabs;
#[path = "browser/persistence_ops.rs"]
mod persistence_ops;
#[path = "browser/validation_perf.rs"]
mod validation_perf;

struct BrowserApp {
    engine: SextantEngine,
    wake: DigitalWake,
    log: CaptainsLog,
    persistence_lane: PersistenceLane,
    data_dir: PathBuf,
    profile_data_dir: PathBuf,
    incognito_data_dir: Option<PathBuf>,
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
    observation_warmup_enabled: bool,
    pre_size_visible_navigation: bool,
    pending_user_navigation: Option<String>,
    pending_user_action: Option<Action>,
    pending_navigation: Option<PendingNavigation>,
    scheduled_observation_warmup: Option<ScheduledObservationWarmup>,
    pending_observation_warmup: Option<PendingObservationWarmup>,
    pending_distillation: Option<PendingDistillation>,
    pending_persistence: Option<PendingPersistence>,
    pending_wake_search: Option<PendingWakeSearch>,
    pending_log_writes: Vec<PendingLogWrite>,
    pending_log_refresh: Option<PendingLogRefresh>,
    browser_mode: BrowserMode,
    user_render_path: UserRenderPath,
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
    mode_rects: Vec<ModeRegion>,
    page_tab_rects: Vec<PageTabRegion>,
    page_tab_prev_rect: Rect,
    page_tab_next_rect: Rect,
    consent_authorize_rect: Rect,
    consent_deny_rect: Rect,
    page_tab_window_start: usize,
    window_size: PhysicalSize<u32>,
    wake_results: Vec<WakeEntry>,
    recent_logs: Vec<LogEntry>,
    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    appliance_cert_entries: Vec<ApplianceCertTrustEntry>,
    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    selected_appliance_cert: Option<usize>,
    appliance_cert_status: String,
    page_scroll: i32,
    main_view: MainView,
    latest_frame: Option<RenderedFrame>,
    pending_frame_capture: Option<PendingFrameCapture>,
    last_frame_viewport: Option<(u32, u32)>,
    last_frame_refresh: Instant,
    last_viewport_mouse_move_forward: Option<Instant>,
    last_viewport_mouse_move_point: Option<(f32, f32)>,
    pending_browser_text_tab: Option<Uuid>,
    pending_browser_text: String,
    last_browser_text_input: Option<Instant>,
    pending_viewport_input_tab: Option<Uuid>,
    pending_viewport_input: VecDeque<ViewportInputEvent>,
    last_viewport_input_flush: Option<Instant>,
    frame_dirty: bool,
    frame_refresh_budget: u8,
    validation: ValidationState,
    #[cfg(feature = "xilem-shell")]
    pilot_brain_lane: PilotBrainLane,
    #[cfg(feature = "xilem-shell")]
    pending_pilot_plan: Option<PendingPilotPlan>,
    // Latest structured pilot run, retained as the hook a future visual dashboard /
    // MCP reads; the human-readable view already renders via pilot_plan/result.
    #[cfg(feature = "xilem-shell")]
    #[allow(dead_code)]
    last_pilot_run: Option<PilotRunArtifact>,
    #[cfg(feature = "xilem-shell")]
    ai_config: AiLocalConfig,
    #[cfg(feature = "xilem-shell")]
    settings_tab_rect: Rect,
}

impl BrowserApp {
    fn new() -> Result<Self, String> {
        Self::new_with_data_dir(app_data_dir().join("browser"))
    }

    fn new_with_data_dir(data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        let profile_data_dir = data_dir.clone();
        #[cfg(feature = "xilem-shell")]
        let (guard_firewall, guard_policy_source) = load_browser_guard_firewall(&data_dir)?;
        let persistence_lane = PersistenceLane::new(&data_dir)?;
        #[cfg(feature = "xilem-shell")]
        let ai_config = AiLocalConfig::load(&profile_data_dir);
        #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
        let appliance_cert_entries = ApplianceCertTrustStore::load(&profile_data_dir)?.entries;
        let mut app = Self {
            engine: SextantEngine::new(),
            wake: DigitalWake::open(data_dir.join("wake.db")).map_err(|e| e.to_string())?,
            log: CaptainsLog::new(data_dir.join("captains-log.db")).map_err(|e| e.to_string())?,
            persistence_lane,
            data_dir,
            profile_data_dir,
            incognito_data_dir: None,
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
            observation_warmup_enabled: false,
            pre_size_visible_navigation: false,
            pending_user_navigation: None,
            pending_user_action: None,
            pending_navigation: None,
            scheduled_observation_warmup: None,
            pending_observation_warmup: None,
            pending_distillation: None,
            pending_persistence: None,
            pending_wake_search: None,
            pending_log_writes: Vec::new(),
            pending_log_refresh: None,
            browser_mode: BrowserMode::Assisted,
            user_render_path: UserRenderPath::FrameBridge,
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
            mode_rects: Vec::new(),
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
            #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
            appliance_cert_entries,
            #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
            selected_appliance_cert: None,
            appliance_cert_status: "Local appliance trust store ready.".to_string(),
            page_scroll: 0,
            main_view: MainView::Browser,
            latest_frame: None,
            pending_frame_capture: None,
            last_frame_viewport: None,
            last_frame_refresh: Instant::now(),
            last_viewport_mouse_move_forward: None,
            last_viewport_mouse_move_point: None,
            pending_browser_text_tab: None,
            pending_browser_text: String::new(),
            last_browser_text_input: None,
            pending_viewport_input_tab: None,
            pending_viewport_input: VecDeque::new(),
            last_viewport_input_flush: None,
            frame_dirty: false,
            frame_refresh_budget: 0,
            validation: ValidationState::default(),
            #[cfg(feature = "xilem-shell")]
            pilot_brain_lane: PilotBrainLane::new(&ai_config),
            #[cfg(feature = "xilem-shell")]
            pending_pilot_plan: None,
            #[cfg(feature = "xilem-shell")]
            last_pilot_run: None,
            #[cfg(feature = "xilem-shell")]
            ai_config,
            #[cfg(feature = "xilem-shell")]
            settings_tab_rect: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
        };
        app.refresh_logs();
        Ok(app)
    }

    fn update_title(&self, window: &Window) {
        window.set_title(&format!("Sextant Browser - {}", self.last_status));
    }

    fn capabilities(&self) -> BrowserCapabilities {
        self.browser_mode.capabilities()
    }

    fn ai_status_label(&self) -> &'static str {
        match self.browser_mode {
            BrowserMode::Agent => "CONTROL",
            BrowserMode::Assisted => "ASSIST",
            BrowserMode::Observe => "OBSERVE",
            BrowserMode::Direct | BrowserMode::Incognito => "OFF",
        }
    }

    fn storage_status_label(&self) -> &'static str {
        if self.incognito_data_dir.is_some() {
            "EPHEMERAL"
        } else {
            "PROFILE"
        }
    }

    fn render_path_label(&self) -> &'static str {
        self.user_render_path.label()
    }

    fn render_path_status(&self) -> &'static str {
        self.user_render_path.status()
    }

    fn direct_render_gap_label(&self) -> Option<&'static str> {
        if self.capabilities().direct_render_required
            && self.user_render_path == UserRenderPath::FrameBridge
        {
            Some("direct compositor pending")
        } else {
            None
        }
    }

    fn set_user_render_path(&mut self, path: UserRenderPath) -> Result<(), String> {
        if !path.integrated() {
            self.last_status = format!(
                "{} render path is proven by sextant-servo-direct, but production shell integration is not complete.",
                path.label()
            );
            self.last_ok = false;
            self.validation.error_seen = true;
            return Err(self.last_status.clone());
        }
        self.user_render_path = path;
        self.last_status = format!("User render path set to {}.", path.status());
        self.last_ok = true;
        Ok(())
    }

    fn native_intent_allowed(&self) -> bool {
        matches!(
            self.browser_mode,
            BrowserMode::Agent | BrowserMode::Assisted
        )
    }

    fn proof_workflow_allowed(&self) -> bool {
        self.native_intent_allowed()
    }

    fn block_mode_control_work(&mut self, label: &str) {
        self.last_status = format!(
            "{} mode blocks {}. Switch to Agent or Assisted mode to run this workflow.",
            self.browser_mode.label(),
            label
        );
        self.last_ok = false;
        self.pilot_status = "BLOCKED".to_string();
        self.pilot_result = self.last_status.clone();
        self.validation.error_seen = true;
    }

    fn set_browser_mode(&mut self, mode: BrowserMode) {
        if self.browser_mode == mode {
            return;
        }
        self.flush_pending_browser_text();
        let previous_mode = self.browser_mode;
        if mode == BrowserMode::Incognito && previous_mode != BrowserMode::Incognito {
            if let Err(error) = self.enter_incognito_storage() {
                self.last_status = format!("Incognito storage setup failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                return;
            }
        } else if previous_mode == BrowserMode::Incognito && mode != BrowserMode::Incognito {
            if let Err(error) = self.leave_incognito_storage() {
                self.last_status = format!("Profile storage restore failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                return;
            }
        }
        self.browser_mode = mode;
        if matches!(mode, BrowserMode::Direct | BrowserMode::Incognito) {
            self.pending_consent = None;
            self.wake_results.clear();
            self.recent_logs.clear();
            self.pending_user_action = None;
            self.scheduled_observation_warmup = None;
            self.pending_observation_warmup = None;
            self.pending_distillation = None;
            self.pending_persistence = None;
            self.pending_wake_search = None;
            self.pending_log_writes.clear();
            self.pending_log_refresh = None;
        } else {
            self.refresh_logs();
        }
        self.last_status = mode.status().to_string();
        self.last_ok = true;
    }

    fn enter_incognito_storage(&mut self) -> Result<(), String> {
        let data_dir =
            env::temp_dir().join(format!("sextant-browser-incognito-{}", Uuid::new_v4()));
        self.reopen_persistent_stores(&data_dir)?;
        self.cleanup_incognito_storage();
        self.data_dir = data_dir.clone();
        self.incognito_data_dir = Some(data_dir);
        self.wake_results.clear();
        self.recent_logs.clear();
        Ok(())
    }

    fn leave_incognito_storage(&mut self) -> Result<(), String> {
        let profile_data_dir = self.profile_data_dir.clone();
        self.reopen_persistent_stores(&profile_data_dir)?;
        let old_incognito_dir = self.incognito_data_dir.take();
        self.data_dir = profile_data_dir;
        self.refresh_logs();
        if let Some(data_dir) = old_incognito_dir {
            let _ = std::fs::remove_dir_all(data_dir);
        }
        Ok(())
    }

    fn reopen_persistent_stores(&mut self, data_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
        self.wake =
            DigitalWake::open(data_dir.join("wake.db")).map_err(|error| error.to_string())?;
        self.log = CaptainsLog::new(data_dir.join("captains-log.db"))
            .map_err(|error| error.to_string())?;
        self.pending_log_writes.clear();
        self.pending_log_refresh = None;
        self.persistence_lane = PersistenceLane::new(data_dir)?;
        Ok(())
    }

    fn cleanup_incognito_storage(&mut self) {
        if let Some(data_dir) = self.incognito_data_dir.take() {
            if self.data_dir == data_dir {
                let profile_data_dir = self.profile_data_dir.clone();
                if self.reopen_persistent_stores(&profile_data_dir).is_ok() {
                    self.data_dir = profile_data_dir;
                }
            }
            let _ = std::fs::remove_dir_all(data_dir);
        }
    }

    fn layout(&mut self, size: PhysicalSize<u32>) {
        self.window_size = size;
        let width = size.width.max(1);
        let rail_x = right_rail_x(width);
        let main_right = rail_x.saturating_sub(18);
        let margin = 24;
        let gap = 8;
        let button_h = 30;
        self.wake_rect = Rect {
            x: rail_x + 20,
            y: size.height.max(1).saturating_sub(STATUS_BAR_H + 66),
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
        #[cfg(feature = "xilem-shell")]
        {
            self.settings_tab_rect = Rect {
                x: 674,
                y: CHROME_H + STRIP_H + 5,
                w: 96,
                h: 30,
            };
        }
        self.mode_rects.clear();
        let mut mode_x: u32 = 146;
        let mode_y = 12;
        for mode in BrowserMode::ALL {
            let label_w = (mode.label().len() as u32)
                .saturating_mul(char_advance(1))
                .saturating_add(22)
                .max(58);
            if mode_x.saturating_add(label_w) > main_right.saturating_sub(8) {
                break;
            }
            self.mode_rects.push(ModeRegion {
                rect: Rect {
                    x: mode_x,
                    y: mode_y,
                    w: label_w,
                    h: 30,
                },
                mode,
            });
            mode_x = mode_x.saturating_add(label_w + 6);
        }
        let address_right = main_right.saturating_sub(86);
        let min_address_w = 180;
        let max_address_x = address_right.saturating_sub(min_address_w).max(112);
        let address_x = mode_x.saturating_add(8).max(112).min(max_address_x);
        self.address_rect = Rect {
            x: address_x,
            y: 12,
            w: address_right.saturating_sub(address_x).max(min_address_w),
            h: button_h,
        };
        let main_panel = main_panel_rect(rail_x, size.height.max(1));
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

    fn reset_validation(&mut self) {
        self.validation = ValidationState::default();
        self.last_status = "Validation session reset. Runtime state is unchanged.".to_string();
        self.last_ok = true;
    }
}

fn app_data_dir() -> PathBuf {
    if let Ok(path) = env::var("SEXTANT_BROWSER_DATA_DIR") {
        let path = path.trim();
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sextant")
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ApplianceCertTrustEntry {
    origin: String,
    fingerprint_sha256: String,
    label: Option<String>,
    created_at: String,
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplianceCertTrustStore {
    version: u32,
    entries: Vec<ApplianceCertTrustEntry>,
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
impl Default for ApplianceCertTrustStore {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
impl ApplianceCertTrustStore {
    fn load(data_dir: &Path) -> Result<Self, String> {
        let path = appliance_cert_trust_path(data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {}", path.display(), error))?;
        let mut store: Self = serde_json::from_str(&contents)
            .map_err(|error| format!("failed to parse {}: {}", path.display(), error))?;
        store.entries.retain(|entry| {
            normalize_certificate_fingerprint(&entry.fingerprint_sha256).is_ok()
                && !entry.origin.trim().is_empty()
        });
        Ok(store)
    }

    fn save(&self, data_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
        let path = appliance_cert_trust_path(data_dir);
        let contents = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to encode {}: {}", path.display(), error))?;
        std::fs::write(&path, contents)
            .map_err(|error| format!("failed to write {}: {}", path.display(), error))
    }

    fn remember(
        &mut self,
        target: &Url,
        fingerprint_sha256: &str,
        label: Option<String>,
    ) -> Result<(), String> {
        let origin = appliance_origin(target)?;
        let fingerprint_sha256 = normalize_certificate_fingerprint(fingerprint_sha256)?;
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.origin == origin) {
            entry.fingerprint_sha256 = fingerprint_sha256;
            entry.label = label;
            entry.created_at = Utc::now().to_rfc3339();
            return Ok(());
        }
        self.entries.push(ApplianceCertTrustEntry {
            origin,
            fingerprint_sha256,
            label,
            created_at: Utc::now().to_rfc3339(),
        });
        Ok(())
    }

    fn is_trusted(&self, target: &Url, fingerprint_sha256: &str) -> Result<bool, String> {
        let origin = appliance_origin(target)?;
        let fingerprint_sha256 = normalize_certificate_fingerprint(fingerprint_sha256)?;
        Ok(self
            .entries
            .iter()
            .any(|entry| entry.origin == origin && entry.fingerprint_sha256 == fingerprint_sha256))
    }

    fn forget(&mut self, target: &Url) -> Result<bool, String> {
        let origin = appliance_origin(target)?;
        let before = self.entries.len();
        self.entries.retain(|entry| entry.origin != origin);
        Ok(self.entries.len() != before)
    }

    fn clear(&mut self) -> bool {
        let had_entries = !self.entries.is_empty();
        self.entries.clear();
        had_entries
    }
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
fn appliance_cert_trust_path(data_dir: &Path) -> PathBuf {
    data_dir.join(APPLIANCE_CERT_TRUST_FILE)
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
fn normalize_certificate_fingerprint(value: &str) -> Result<String, String> {
    let compact: String = value
        .chars()
        .filter(|ch| !matches!(ch, ':' | ' ' | '-' | '\n' | '\r' | '\t'))
        .collect::<String>()
        .to_ascii_lowercase();
    if compact.len() != 64 || !compact.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("certificate fingerprint must be a 64-character SHA-256 hex value".to_string());
    }
    Ok(compact)
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
fn appliance_origin(target: &Url) -> Result<String, String> {
    if target.scheme() != "https" {
        return Err("appliance certificate trust only applies to https origins".to_string());
    }
    if !is_local_appliance_target(target) {
        return Err(format!(
            "appliance certificate trust is limited to localhost, .local, private, and link-local targets; refused {}",
            target
        ));
    }
    let host = target
        .host_str()
        .ok_or_else(|| "appliance URL must include a host".to_string())?
        .to_ascii_lowercase();
    let mut origin = format!("https://{host}");
    if let Some(port) = target.port() {
        origin.push(':');
        origin.push_str(&port.to_string());
    }
    Ok(origin)
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
fn handle_list_local_appliance_certs_arg(args: &[String]) -> Result<bool, String> {
    if !args.iter().any(|arg| arg == "--list-local-appliance-certs") {
        return Ok(false);
    }
    let profile_data_dir = app_data_dir().join("browser");
    let path = appliance_cert_trust_path(&profile_data_dir);
    let trust_store = ApplianceCertTrustStore::load(&profile_data_dir)?;
    let payload = serde_json::json!({
        "path": path,
        "count": trust_store.entries.len(),
        "entries": trust_store.entries,
    });
    let encoded = serde_json::to_string(&payload)
        .map_err(|error| format!("failed to encode appliance cert trust list: {error}"))?;
    println!("[sextant-browser] local appliance certificate trust list {encoded}");
    Ok(true)
}

#[cfg(not(any(feature = "xilem-shell", feature = "servo-backend")))]
fn handle_list_local_appliance_certs_arg(args: &[String]) -> Result<bool, String> {
    if args.iter().any(|arg| arg == "--list-local-appliance-certs") {
        return Err(
            "local appliance certificate trust storage is unavailable in this build".to_string(),
        );
    }
    Ok(false)
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
fn handle_forget_local_appliance_cert_arg(args: &[String]) -> Result<bool, String> {
    let Some(target) = operator_arg_value(args, "--forget-local-appliance-cert") else {
        return Ok(false);
    };
    let profile_data_dir = app_data_dir().join("browser");
    let mut trust_store = ApplianceCertTrustStore::load(&profile_data_dir)?;
    let changed = if target.eq_ignore_ascii_case("all") {
        trust_store.clear()
    } else {
        let target = parse_navigation_target(&target)?;
        trust_store.forget(&target)?
    };
    trust_store.save(&profile_data_dir)?;
    println!(
        "[sextant-browser] local appliance certificate trust forget {} changed {}",
        target, changed
    );
    Ok(true)
}

#[cfg(not(any(feature = "xilem-shell", feature = "servo-backend")))]
fn handle_forget_local_appliance_cert_arg(args: &[String]) -> Result<bool, String> {
    if operator_arg_value(args, "--forget-local-appliance-cert").is_some() {
        return Err(
            "local appliance certificate trust storage is unavailable in this build".to_string(),
        );
    }
    Ok(false)
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
    match handle_list_local_appliance_certs_arg(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("[sextant-browser] failed: {error}");
            std::process::exit(2);
        }
    }
    match handle_forget_local_appliance_cert_arg(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("[sextant-browser] failed: {error}");
            std::process::exit(2);
        }
    }
    if let Some(path) = operator_arg_value(&args, DIRECT_INCOGNITO_CLEANUP_ARG) {
        if let Err(error) = cleanup_direct_incognito_data_dir(Path::new(&path)) {
            eprintln!("[sextant-browser] direct incognito cleanup failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|arg| arg == "--retro-egui-proof") {
        #[cfg(all(target_os = "windows", feature = "servo-backend"))]
        {
            if let Err(error) = run_retro_egui_proof() {
                eprintln!("[retro-egui-proof] failed: {error}");
                std::process::exit(1);
            }
            return;
        }
        #[cfg(not(all(target_os = "windows", feature = "servo-backend")))]
        {
            eprintln!("--retro-egui-proof requires the Windows servo-backend build");
            std::process::exit(2);
        }
    }

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
    let browser_mode = match parse_browser_mode_arg(&args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("[sextant-browser] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    let user_render_path = match parse_user_render_path_arg(&args) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("[sextant-browser] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    let pre_size_visible_navigation = args
        .iter()
        .any(|arg| arg == "--pre-size-navigation" || arg == "--pre-size-visible-navigation");
    let certificate_path = operator_arg_value(&args, "--certificate-path").map(PathBuf::from);
    let local_appliance_cert_fingerprint =
        operator_arg_value(&args, "--local-appliance-cert-fingerprint");
    let remember_local_appliance_cert =
        operator_arg_value(&args, "--remember-local-appliance-cert");
    let allow_insecure_local_tls = window_smoke
        .as_ref()
        .map(|smoke| smoke.allow_insecure_local_tls)
        .unwrap_or(false)
        || args.iter().any(|arg| arg == "--allow-insecure-local-tls");
    let startup_input = operator_arg_value(&args, "--start");
    let start_showcase = args
        .iter()
        .any(|arg| arg == "--start-showcase" || arg == "--demo");
    let start_real_browsing = args.iter().any(|arg| arg == "--start-real-browsing");
    let start_shell_interaction = args.iter().any(|arg| arg == "--start-shell-interaction");
    let start_location_smoke = operator_arg_value(&args, "--location-smoke")
        .or_else(|| operator_arg_value(&args, "--window-location-smoke"));
    let start_history_smoke = operator_arg_value(&args, "--history-smoke")
        .or_else(|| operator_arg_value(&args, "--window-history-smoke"));
    let start_first_interaction_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.first_interaction_smoke)
        .unwrap_or(false)
        || args.iter().any(|arg| {
            arg == "--first-interaction-smoke" || arg == "--window-first-interaction-smoke"
        });
    let start_verified_input_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.verified_input_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--verified-input-smoke" || arg == "--window-verified-input-smoke");
    let start_search_submit_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.search_submit_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--search-submit-smoke" || arg == "--window-search-submit-smoke");
    let start_live_search_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.live_search_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--live-search-smoke" || arg == "--window-live-search-smoke");
    let start_live_form_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.live_form_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--live-form-smoke" || arg == "--window-live-form-smoke");
    let start_live_link_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.live_link_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--live-link-smoke" || arg == "--window-live-link-smoke");
    let direct_resource_audit = args
        .iter()
        .any(|arg| arg == "--direct-resource-audit" || arg == "--resource-audit");
    let raw_direct_window = args.iter().any(|arg| arg == "--raw-direct-window");
    let hosted_direct_smoke = args.iter().any(|arg| arg == "--hosted-direct-smoke");
    let hosted_direct_smoke_action = if hosted_direct_smoke {
        match HostedDirectSmokeAction::parse_arg(operator_arg_value(
            &args,
            "--hosted-direct-smoke-cert-action",
        )) {
            Ok(action) => action,
            Err(error) => {
                eprintln!("[hosted-direct-smoke] failed to parse arguments: {error}");
                std::process::exit(2);
            }
        }
    } else {
        None
    };
    let hosted_direct_smoke_timeout = if hosted_direct_smoke {
        let default_timeout =
            match parse_duration_arg(&args, "--window-smoke-timeout", Duration::from_secs(20)) {
                Ok(timeout) => timeout,
                Err(error) => {
                    eprintln!("[hosted-direct-smoke] failed to parse arguments: {error}");
                    std::process::exit(2);
                }
            };
        match parse_duration_arg(&args, "--hosted-direct-smoke-timeout", default_timeout) {
            Ok(timeout) => Some(timeout),
            Err(error) => {
                eprintln!("[hosted-direct-smoke] failed to parse arguments: {error}");
                std::process::exit(2);
            }
        }
    } else {
        None
    };
    let embed_parent_hwnd = parse_isize_flag(&args, "--embed-parent-hwnd");
    let embed_x = parse_i32_flag(&args, "--embed-x").unwrap_or(0);
    let embed_y = parse_i32_flag(&args, "--embed-y").unwrap_or(0);
    let embed_width = parse_u32_flag(&args, "--embed-width");
    let embed_height = parse_u32_flag(&args, "--embed-height");
    let start_load_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.load_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--load-smoke" || arg == "--window-load-smoke");
    let start_reload_smoke = args
        .iter()
        .any(|arg| arg == "--reload-smoke" || arg == "--window-reload-smoke");
    let start_resize_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.resize_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--resize-smoke" || arg == "--window-resize-smoke");

    if user_render_path == UserRenderPath::DirectServo
        && !raw_direct_window
        && window_smoke.is_none()
    {
        let interactive =
            hosted_direct_smoke_timeout.is_none() && hosted_direct_smoke_action.is_none();
        #[cfg(all(target_os = "windows", feature = "servo-backend"))]
        let hosted_result = if interactive {
            run_hosted_direct_app_egui(
                startup_input,
                browser_mode,
                certificate_path,
                local_appliance_cert_fingerprint,
                remember_local_appliance_cert,
                allow_insecure_local_tls,
                direct_resource_audit,
            )
        } else {
            run_hosted_direct_app(
                startup_input,
                browser_mode,
                certificate_path,
                local_appliance_cert_fingerprint,
                remember_local_appliance_cert,
                allow_insecure_local_tls,
                direct_resource_audit,
                hosted_direct_smoke_timeout,
                hosted_direct_smoke_action,
            )
        };
        #[cfg(not(all(target_os = "windows", feature = "servo-backend")))]
        let hosted_result = {
            let _ = interactive;
            run_hosted_direct_app(
                startup_input,
                browser_mode,
                certificate_path,
                local_appliance_cert_fingerprint,
                remember_local_appliance_cert,
                allow_insecure_local_tls,
                direct_resource_audit,
                hosted_direct_smoke_timeout,
                hosted_direct_smoke_action,
            )
        };
        if let Err(error) = hosted_result {
            eprintln!("[sextant-browser] failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    if user_render_path == UserRenderPath::DirectServo {
        if let Err(error) = run_direct_servo_browser(
            window_smoke,
            startup_input,
            start_showcase,
            start_real_browsing,
            start_shell_interaction,
            start_location_smoke,
            start_history_smoke,
            start_first_interaction_smoke,
            start_verified_input_smoke,
            start_search_submit_smoke,
            start_live_search_smoke,
            start_live_form_smoke,
            start_live_link_smoke,
            direct_resource_audit,
            start_load_smoke,
            start_reload_smoke,
            start_resize_smoke,
            browser_mode,
            pre_size_visible_navigation,
            certificate_path,
            local_appliance_cert_fingerprint,
            remember_local_appliance_cert,
            allow_insecure_local_tls,
            embed_parent_hwnd,
            embed_x,
            embed_y,
            embed_width,
            embed_height,
        ) {
            eprintln!("[sextant-browser] failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    // Experimental egui bridge shell (parity port in progress). Opt in with
    // SEXTANT_EGUI_BRIDGE=1; only the interactive bridge path (no smoke).
    #[cfg(all(
        target_os = "windows",
        feature = "servo-backend",
        feature = "xilem-shell"
    ))]
    if window_smoke.is_none() && env_override("SEXTANT_EGUI_BRIDGE").is_some() {
        if let Err(error) = run_visible_app_egui(browser_mode) {
            eprintln!("[sextant-browser] egui bridge failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = run_visible_app(
        window_smoke,
        startup_input,
        start_showcase,
        start_real_browsing,
        start_shell_interaction,
        browser_mode,
        user_render_path,
        pre_size_visible_navigation,
    ) {
        eprintln!("[sextant-browser] failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(feature = "servo-backend")]
fn run_direct_servo_browser(
    window_smoke: Option<WindowSmokeSpec>,
    startup_input: Option<String>,
    start_showcase: bool,
    start_real_browsing: bool,
    start_shell_interaction: bool,
    start_location_smoke: Option<String>,
    start_history_smoke: Option<String>,
    start_first_interaction_smoke: bool,
    start_verified_input_smoke: bool,
    start_search_submit_smoke: bool,
    start_live_search_smoke: bool,
    start_live_form_smoke: bool,
    start_live_link_smoke: bool,
    direct_resource_audit: bool,
    start_load_smoke: bool,
    start_reload_smoke: bool,
    start_resize_smoke: bool,
    browser_mode: BrowserMode,
    pre_size_visible_navigation: bool,
    certificate_path: Option<PathBuf>,
    local_appliance_cert_fingerprint: Option<String>,
    remember_local_appliance_cert: Option<String>,
    allow_insecure_local_tls: bool,
    embed_parent_hwnd: Option<isize>,
    embed_x: i32,
    embed_y: i32,
    embed_width: Option<u32>,
    embed_height: Option<u32>,
) -> Result<(), String> {
    if !matches!(browser_mode, BrowserMode::Direct | BrowserMode::Incognito) {
        return Err(format!(
            "raw direct Servo render path currently supports Direct or Incognito mode only; {} mode still requires the bridge shell for AI observation/control surfaces",
            browser_mode.label()
        ));
    }

    let mut unsupported = Vec::new();
    if start_showcase {
        unsupported.push("launch showcase");
    }
    if start_real_browsing {
        unsupported.push("real browsing workflow");
    }
    if let Some(smoke) = window_smoke.as_ref() {
        if smoke.user_distill {
            unsupported.push("user distill");
        }
        if smoke.input_latency && start_shell_interaction {
            unsupported.push("combined direct input and tab smoke");
        }
        if smoke.input_latency && start_location_smoke.is_some() {
            unsupported.push("combined direct input and location smoke");
        }
        if smoke.input_latency && start_history_smoke.is_some() {
            unsupported.push("combined direct input and history smoke");
        }
        if smoke.input_latency && start_first_interaction_smoke {
            unsupported.push("combined direct input and first-interaction smoke");
        }
        if smoke.input_latency && start_verified_input_smoke {
            unsupported.push("combined direct input and verified-input smoke");
        }
        if smoke.input_latency && start_search_submit_smoke {
            unsupported.push("combined direct input and search-submit smoke");
        }
        if smoke.input_latency && start_live_search_smoke {
            unsupported.push("combined direct input and live-search smoke");
        }
        if smoke.input_latency && start_live_form_smoke {
            unsupported.push("combined direct input and live-form smoke");
        }
        if smoke.input_latency && start_load_smoke {
            unsupported.push("combined direct input and load smoke");
        }
        if smoke.input_latency && start_reload_smoke {
            unsupported.push("combined direct input and reload smoke");
        }
        if smoke.input_latency && start_resize_smoke {
            unsupported.push("combined direct input and resize smoke");
        }
        if start_shell_interaction && start_location_smoke.is_some() {
            unsupported.push("combined direct tab and location smoke");
        }
        if start_shell_interaction && start_history_smoke.is_some() {
            unsupported.push("combined direct tab and history smoke");
        }
        if start_shell_interaction && start_first_interaction_smoke {
            unsupported.push("combined direct tab and first-interaction smoke");
        }
        if start_shell_interaction && start_verified_input_smoke {
            unsupported.push("combined direct tab and verified-input smoke");
        }
        if start_shell_interaction && start_search_submit_smoke {
            unsupported.push("combined direct tab and search-submit smoke");
        }
        if start_shell_interaction && start_live_search_smoke {
            unsupported.push("combined direct tab and live-search smoke");
        }
        if start_shell_interaction && start_live_form_smoke {
            unsupported.push("combined direct tab and live-form smoke");
        }
        if start_shell_interaction && start_load_smoke {
            unsupported.push("combined direct tab and load smoke");
        }
        if start_shell_interaction && start_resize_smoke {
            unsupported.push("combined direct tab and resize smoke");
        }
        if start_location_smoke.is_some() && start_history_smoke.is_some() {
            unsupported.push("combined direct location and history smoke");
        }
        if start_location_smoke.is_some() && start_first_interaction_smoke {
            unsupported.push("combined direct location and first-interaction smoke");
        }
        if start_location_smoke.is_some() && start_verified_input_smoke {
            unsupported.push("combined direct location and verified-input smoke");
        }
        if start_location_smoke.is_some() && start_search_submit_smoke {
            unsupported.push("combined direct location and search-submit smoke");
        }
        if start_location_smoke.is_some() && start_live_search_smoke {
            unsupported.push("combined direct location and live-search smoke");
        }
        if start_location_smoke.is_some() && start_live_form_smoke {
            unsupported.push("combined direct location and live-form smoke");
        }
        if start_location_smoke.is_some() && start_load_smoke {
            unsupported.push("combined direct location and load smoke");
        }
        if start_location_smoke.is_some() && start_resize_smoke {
            unsupported.push("combined direct location and resize smoke");
        }
        if start_history_smoke.is_some() && start_load_smoke {
            unsupported.push("combined direct history and load smoke");
        }
        if start_history_smoke.is_some() && start_first_interaction_smoke {
            unsupported.push("combined direct history and first-interaction smoke");
        }
        if start_history_smoke.is_some() && start_verified_input_smoke {
            unsupported.push("combined direct history and verified-input smoke");
        }
        if start_history_smoke.is_some() && start_search_submit_smoke {
            unsupported.push("combined direct history and search-submit smoke");
        }
        if start_history_smoke.is_some() && start_live_search_smoke {
            unsupported.push("combined direct history and live-search smoke");
        }
        if start_history_smoke.is_some() && start_live_form_smoke {
            unsupported.push("combined direct history and live-form smoke");
        }
        if start_first_interaction_smoke && start_load_smoke {
            unsupported.push("combined direct first-interaction and load smoke");
        }
        if start_first_interaction_smoke && start_resize_smoke {
            unsupported.push("combined direct first-interaction and resize smoke");
        }
        if start_first_interaction_smoke && start_verified_input_smoke {
            unsupported.push("combined direct first-interaction and verified-input smoke");
        }
        if start_first_interaction_smoke && start_live_search_smoke {
            unsupported.push("combined direct first-interaction and live-search smoke");
        }
        if start_first_interaction_smoke && start_live_form_smoke {
            unsupported.push("combined direct first-interaction and live-form smoke");
        }
        if start_verified_input_smoke && start_load_smoke {
            unsupported.push("combined direct verified-input and load smoke");
        }
        if start_verified_input_smoke && start_resize_smoke {
            unsupported.push("combined direct verified-input and resize smoke");
        }
        if start_verified_input_smoke && start_search_submit_smoke {
            unsupported.push("combined direct verified-input and search-submit smoke");
        }
        if start_verified_input_smoke && start_live_search_smoke {
            unsupported.push("combined direct verified-input and live-search smoke");
        }
        if start_verified_input_smoke && start_live_form_smoke {
            unsupported.push("combined direct verified-input and live-form smoke");
        }
        if start_search_submit_smoke && start_live_search_smoke {
            unsupported.push("combined direct search-submit and live-search smoke");
        }
        if start_search_submit_smoke && start_live_form_smoke {
            unsupported.push("combined direct search-submit and live-form smoke");
        }
        if start_search_submit_smoke && start_load_smoke {
            unsupported.push("combined direct search-submit and load smoke");
        }
        if start_search_submit_smoke && start_resize_smoke {
            unsupported.push("combined direct search-submit and resize smoke");
        }
        if start_live_search_smoke && start_load_smoke {
            unsupported.push("combined direct live-search and load smoke");
        }
        if start_live_search_smoke && start_resize_smoke {
            unsupported.push("combined direct live-search and resize smoke");
        }
        if start_live_search_smoke && start_live_form_smoke {
            unsupported.push("combined direct live-search and live-form smoke");
        }
        if start_live_form_smoke && start_load_smoke {
            unsupported.push("combined direct live-form and load smoke");
        }
        if start_live_form_smoke && start_resize_smoke {
            unsupported.push("combined direct live-form and resize smoke");
        }
        if start_history_smoke.is_some() && start_resize_smoke {
            unsupported.push("combined direct history and resize smoke");
        }
        if start_load_smoke && start_resize_smoke {
            unsupported.push("combined direct load and resize smoke");
        }
        if start_reload_smoke
            && (start_shell_interaction
                || start_location_smoke.is_some()
                || start_history_smoke.is_some()
                || start_first_interaction_smoke
                || start_verified_input_smoke
                || start_search_submit_smoke
                || start_live_search_smoke
                || start_live_form_smoke
                || start_load_smoke
                || start_resize_smoke)
        {
            unsupported.push("combined direct reload and other control smoke");
        }
        if start_live_link_smoke
            && (start_shell_interaction
                || start_location_smoke.is_some()
                || start_history_smoke.is_some()
                || start_first_interaction_smoke
                || start_verified_input_smoke
                || start_search_submit_smoke
                || start_live_search_smoke
                || start_live_form_smoke
                || start_load_smoke
                || start_reload_smoke
                || start_resize_smoke)
        {
            unsupported.push("combined direct live-link and other control smoke");
        }
    }
    if !unsupported.is_empty() {
        return Err(format!(
            "direct Servo render path currently supports raw URL browsing only; {} still require the bridge shell",
            unsupported.join(", ")
        ));
    }

    let smoke_mode = window_smoke.is_some();
    let timeout = window_smoke
        .as_ref()
        .map(|smoke| smoke.timeout)
        .unwrap_or(WINDOW_SMOKE_DEFAULT_TIMEOUT);
    let input_latency = window_smoke
        .as_ref()
        .map(|smoke| smoke.input_latency)
        .unwrap_or(false);
    let requested_target = window_smoke
        .as_ref()
        .and_then(|smoke| smoke.target.clone())
        .or(startup_input);
    let verified_input_fixture = if requested_target.is_none() && start_verified_input_smoke {
        Some(direct_servo::start_verified_input_fixture_server()?)
    } else {
        None
    };
    let search_submit_fixture = if requested_target.is_none() && start_search_submit_smoke {
        Some(direct_servo::start_search_submit_fixture_server()?)
    } else {
        None
    };
    let target_text = requested_target.unwrap_or_else(|| {
        if input_latency {
            input_latency_fixture_url()
        } else if start_first_interaction_smoke {
            input_latency_fixture_url()
        } else if start_verified_input_smoke {
            verified_input_fixture
                .as_ref()
                .expect("verified input fixture should be started")
                .url()
                .to_string()
        } else if start_search_submit_smoke {
            search_submit_fixture
                .as_ref()
                .expect("search submit fixture should be started")
                .url()
                .to_string()
        } else if start_live_search_smoke {
            "https://lite.duckduckgo.com/lite/".to_string()
        } else if start_live_form_smoke {
            "https://httpbin.org/forms/post".to_string()
        } else {
            "https://example.com".to_string()
        }
    });
    let target = parse_navigation_target(&target_text)?;
    if let Some(path) = certificate_path.as_ref() {
        if !path.is_file() {
            return Err(format!(
                "certificate path '{}' does not exist or is not a file",
                path.display()
            ));
        }
    }
    if allow_insecure_local_tls && !is_local_appliance_target(&target) {
        return Err(format!(
            "--allow-insecure-local-tls is limited to localhost, .local, private, and link-local targets; refused {}",
            target
        ));
    }
    let profile_data_dir = app_data_dir().join("browser");
    let mut profile_trusted_local_tls = false;
    if let Some(fingerprint) = remember_local_appliance_cert.as_ref() {
        if browser_mode == BrowserMode::Incognito {
            return Err(
                "--remember-local-appliance-cert is disabled in Incognito; use Trust Once semantics"
                    .to_string(),
            );
        }
        let mut trust_store = ApplianceCertTrustStore::load(&profile_data_dir)?;
        trust_store.remember(&target, fingerprint, target.host_str().map(str::to_string))?;
        trust_store.save(&profile_data_dir)?;
        profile_trusted_local_tls = true;
    } else if browser_mode != BrowserMode::Incognito {
        if let Some(fingerprint) = local_appliance_cert_fingerprint.as_ref() {
            let trust_store = ApplianceCertTrustStore::load(&profile_data_dir)?;
            profile_trusted_local_tls = trust_store.is_trusted(&target, fingerprint)?;
        }
    }
    let appliance_certificate_actions =
        if target.scheme() == "https" && is_local_appliance_target(&target) {
            let trust_target = target.clone();
            let trust_data_dir = profile_data_dir.clone();
            Some(direct_servo::DirectApplianceCertificateActions {
                persist_allowed: browser_mode != BrowserMode::Incognito,
                remember: Arc::new(move |fingerprint| {
                    let mut trust_store = ApplianceCertTrustStore::load(&trust_data_dir)?;
                    trust_store.remember(
                        &trust_target,
                        &fingerprint,
                        trust_target.host_str().map(str::to_string),
                    )?;
                    trust_store.save(&trust_data_dir)
                }),
            })
        } else {
            None
        };
    let scripted_text = input_latency.then(|| "sextant".to_string());
    let direct_data_dir = if browser_mode == BrowserMode::Incognito {
        let data_dir = env::temp_dir().join(format!(
            "sextant-browser-direct-incognito-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
        Some(data_dir)
    } else {
        None
    };

    // Servo config dir for the direct lane (cookies, IndexedDB, HSTS, auth cache):
    // the ephemeral dir in Incognito, otherwise a persistent profile dir so
    // storage is profile-scoped instead of falling back to Servo's cwd-relative
    // default (which would leak `IndexedDB/` next to the working directory and
    // persist in Incognito). `direct_data_dir` stays Incognito-only so the exit
    // cleanup never removes the persistent Direct profile.
    let direct_config_dir: Option<PathBuf> = match direct_data_dir.as_ref() {
        Some(ephemeral) => Some(ephemeral.clone()),
        None => {
            let data_dir = app_data_dir().join("browser").join("direct-servo");
            std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
            Some(data_dir)
        }
    };

    if smoke_mode {
        println!("[window-smoke] starting visible browser shell smoke");
        println!(
            "[window-smoke] browser mode {} ({})",
            browser_mode.label(),
            browser_mode.status()
        );
        println!(
            "[window-smoke] render path {} ({})",
            UserRenderPath::DirectServo.label(),
            UserRenderPath::DirectServo.status()
        );
        println!("[window-smoke] render gap shell chrome compositor pending");
        println!(
            "[window-smoke] pre-size navigation {}",
            pre_size_visible_navigation
        );
        if let Some(data_dir) = direct_data_dir.as_ref() {
            println!(
                "[window-smoke] ephemeral browser data dir {}",
                data_dir.display()
            );
            println!(
                "[window-smoke] direct Servo config dir {}",
                data_dir.display()
            );
            println!("[window-smoke] direct Servo http cache disabled true");
        }
        if let Some(path) = certificate_path.as_ref() {
            println!(
                "[window-smoke] direct Servo certificate path {}",
                path.display()
            );
        }
        if allow_insecure_local_tls {
            println!("[window-smoke] direct Servo insecure local TLS true");
        }
        if profile_trusted_local_tls {
            println!("[window-smoke] direct Servo trusted local appliance TLS true");
        }
        let viewport_width = embed_width.unwrap_or(1180);
        let viewport_height = embed_height.unwrap_or(760);
        println!("[window-smoke] direct viewport {viewport_width}x{viewport_height}");
    } else {
        println!(
            "[window-start] starting direct Servo raw browsing lane in {} mode",
            browser_mode.label()
        );
    }

    let ignore_certificate_errors = allow_insecure_local_tls || profile_trusted_local_tls;
    let bypass_proxy_for_target = is_local_appliance_target(&target)
        && (ignore_certificate_errors || target.scheme() == "https");
    let outcome = direct_servo::run(direct_servo::DirectServoOptions {
        target: target.clone(),
        smoke: smoke_mode,
        timeout,
        title: "Sextant Browser Direct".to_string(),
        log_prefix: "window-direct",
        setup_servo_logging: false,
        scripted_text,
        scripted_first_interaction_smoke: start_first_interaction_smoke,
        scripted_verified_input_smoke: start_verified_input_smoke,
        scripted_verified_input_repeat_smoke: false,
        scripted_search_submit_smoke: start_search_submit_smoke,
        scripted_live_search_smoke: start_live_search_smoke,
        scripted_live_form_smoke: start_live_form_smoke,
        scripted_live_link_smoke: start_live_link_smoke,
        scripted_retained_navigation_smoke: false,
        scripted_location: start_location_smoke,
        scripted_history: start_history_smoke,
        scripted_load_smoke: start_load_smoke,
        scripted_reload_smoke: start_reload_smoke,
        scripted_resize_smoke: start_resize_smoke,
        scripted_tab_smoke: start_shell_interaction,
        resource_audit: direct_resource_audit,
        config_dir: direct_config_dir.clone(),
        disable_http_cache: browser_mode == BrowserMode::Incognito,
        certificate_path: certificate_path.clone(),
        ignore_certificate_errors,
        appliance_certificate_actions,
        bypass_proxy_for_target,
        embed_parent_hwnd,
        embed_bounds: embed_width.zip(embed_height).map(|(width, height)| {
            direct_servo::DirectEmbedBounds {
                x: embed_x,
                y: embed_y,
                width,
                height,
            }
        }),
        host_command_rx: None,
        read_host_commands_from_stdin: embed_parent_hwnd.is_some(),
    })?;
    let cleanup_result = direct_data_dir
        .as_ref()
        .map(|data_dir| cleanup_or_defer_direct_incognito_data_dir(data_dir))
        .unwrap_or(Ok(()));
    if let Err(cleanup_error) = cleanup_result {
        return Err(cleanup_error);
    }

    if !smoke_mode
        && (outcome.appliance_certificate_trust_once_requested
            || outcome.appliance_certificate_remembered == Some(true))
    {
        if embed_parent_hwnd.is_some() {
            let decision = if outcome.appliance_certificate_remembered == Some(true) {
                "remembered"
            } else {
                "trust-once"
            };
            if let Some(fingerprint) = outcome.certificate_fingerprint_sha256.as_ref() {
                println!(
                    "[window-direct] hosted certificate trust {decision} fingerprint {fingerprint}"
                );
            } else {
                println!("[window-direct] hosted certificate trust {decision}");
            }
            return Ok(());
        }
        relaunch_direct_local_appliance_after_trust(
            browser_mode,
            &target,
            &outcome,
            certificate_path.as_deref(),
            direct_resource_audit,
        )?;
        return Ok(());
    }

    if smoke_mode {
        if let Some(first_present) = outcome.first_present {
            println!(
                "[window-smoke] first direct present in {}",
                direct_servo::format_duration(first_present)
            );
            println!(
                "[window-smoke] first Servo frame in {}",
                direct_servo::format_duration(first_present)
            );
        }
        println!(
            "[window-smoke] direct frame timing frames={} slow={}",
            outcome.direct_frame_count, outcome.direct_slow_frame_count
        );
        if let Some(max_frame) = outcome.direct_max_frame {
            println!(
                "[window-smoke] max direct frame total={} spin={} paint={} present={}",
                direct_servo::format_duration(max_frame),
                outcome
                    .direct_max_spin
                    .map(direct_servo::format_duration)
                    .unwrap_or_else(|| "n/a".to_string()),
                outcome
                    .direct_max_paint
                    .map(direct_servo::format_duration)
                    .unwrap_or_else(|| "n/a".to_string()),
                outcome
                    .direct_max_present
                    .map(direct_servo::format_duration)
                    .unwrap_or_else(|| "n/a".to_string())
            );
        }
        if let Some(input_frame) = outcome.input_frame {
            println!(
                "[window-smoke] input frame {}",
                direct_servo::format_duration(input_frame)
            );
        }
        if let Some(first_interaction_frame) = outcome.first_interaction_frame {
            println!(
                "[window-smoke] first interaction frame {}",
                direct_servo::format_duration(first_interaction_frame)
            );
        }
        if let Some(before_load_complete) = outcome.first_interaction_before_load_complete {
            println!(
                "[window-smoke] first interaction before load complete {}",
                before_load_complete
            );
        }
        if let Some(verified_input_frame) = outcome.verified_input_frame {
            println!(
                "[window-smoke] verified input frame {}",
                direct_servo::format_duration(verified_input_frame)
            );
        }
        if let Some(verified_input_second_frame) = outcome.verified_input_second_frame {
            println!(
                "[window-smoke] verified repeat input frame {}",
                direct_servo::format_duration(verified_input_second_frame)
            );
        }
        if let Some(verified_input) = outcome.verified_input {
            println!("[window-smoke] verified input {}", verified_input);
        }
        if let Some(before_load_complete) = outcome.verified_input_before_load_complete {
            println!(
                "[window-smoke] verified input before load complete {}",
                before_load_complete
            );
        }
        if let Some(search_submit_frame) = outcome.search_submit_frame {
            println!(
                "[window-smoke] search submit frame {}",
                direct_servo::format_duration(search_submit_frame)
            );
        }
        if let Some(search_submit) = outcome.search_submit {
            println!("[window-smoke] search submit {}", search_submit);
        }
        if let Some(search_submit_url) = outcome.search_submit_url {
            println!("[window-smoke] search submit url {}", search_submit_url);
        }
        if let Some(search_submit_title) = outcome.search_submit_title {
            println!("[window-smoke] search submit title {}", search_submit_title);
        }
        if let Some(live_search_frame) = outcome.live_search_frame {
            println!(
                "[window-smoke] live search frame {}",
                direct_servo::format_duration(live_search_frame)
            );
        }
        if let Some(live_search) = outcome.live_search {
            println!("[window-smoke] live search {}", live_search);
        }
        if let Some(live_search_url) = outcome.live_search_url {
            println!("[window-smoke] live search url {}", live_search_url);
        }
        if let Some(live_search_title) = outcome.live_search_title {
            println!("[window-smoke] live search title {}", live_search_title);
        }
        if let Some(live_form_frame) = outcome.live_form_frame {
            println!(
                "[window-smoke] live form frame {}",
                direct_servo::format_duration(live_form_frame)
            );
        }
        if let Some(live_form) = outcome.live_form {
            println!("[window-smoke] live form {}", live_form);
        }
        if let Some(live_form_url) = outcome.live_form_url {
            println!("[window-smoke] live form url {}", live_form_url);
        }
        if let Some(live_form_title) = outcome.live_form_title {
            println!("[window-smoke] live form title {}", live_form_title);
        }
        if let Some(live_link_frame) = outcome.live_link_frame {
            println!(
                "[window-smoke] live link frame {}",
                direct_servo::format_duration(live_link_frame)
            );
        }
        if let Some(live_link) = outcome.live_link {
            println!("[window-smoke] live link {}", live_link);
        }
        if let Some(live_link_url) = outcome.live_link_url {
            println!("[window-smoke] live link url {}", live_link_url);
        }
        if let Some(live_link_title) = outcome.live_link_title {
            println!("[window-smoke] live link title {}", live_link_title);
        }
        if let Some(location_frame) = outcome.location_frame {
            println!(
                "[window-smoke] location frame {}",
                direct_servo::format_duration(location_frame)
            );
        }
        if let Some(location_url) = outcome.location_url {
            println!("[window-smoke] location url {}", location_url);
        }
        if let Some(location_title) = outcome.location_title {
            println!("[window-smoke] location title {}", location_title);
        }
        if let Some(history_navigation_frame) = outcome.history_navigation_frame {
            println!(
                "[window-smoke] history navigation frame {}",
                direct_servo::format_duration(history_navigation_frame)
            );
        }
        if let Some(history_back_frame) = outcome.history_back_frame {
            println!(
                "[window-smoke] history back frame {}",
                direct_servo::format_duration(history_back_frame)
            );
        }
        if let Some(history_forward_frame) = outcome.history_forward_frame {
            println!(
                "[window-smoke] history forward frame {}",
                direct_servo::format_duration(history_forward_frame)
            );
        }
        if let Some(history_final_url) = outcome.history_final_url {
            println!("[window-smoke] history final url {}", history_final_url);
        }
        if let Some(history_final_title) = outcome.history_final_title {
            println!("[window-smoke] history final title {}", history_final_title);
        }
        if let Some(load_complete) = outcome.load_complete {
            println!(
                "[window-smoke] load complete {}",
                direct_servo::format_duration(load_complete)
            );
        }
        if let Some(load_url) = outcome.load_url {
            println!("[window-smoke] load url {}", load_url);
        }
        if let Some(load_title) = outcome.load_title {
            println!("[window-smoke] load title {}", load_title);
        }
        if let Some(fingerprint) = outcome.certificate_fingerprint_sha256 {
            println!("[window-smoke] certificate fingerprint sha256 {fingerprint}");
        }
        if let Some(remembered) = outcome.appliance_certificate_remembered {
            println!("[window-smoke] appliance certificate remembered {remembered}");
        }
        if let Some(error) = outcome.appliance_certificate_remember_error {
            println!("[window-smoke] appliance certificate remember error {error}");
        }
        if let Some(audit) = outcome.resource_audit_json {
            println!("[window-smoke] resource audit {audit}");
        }
        if let Some(reload_frame) = outcome.reload_frame {
            println!(
                "[window-smoke] reload frame {}",
                direct_servo::format_duration(reload_frame)
            );
        }
        if let Some(reload_url) = outcome.reload_url {
            println!("[window-smoke] reload url {}", reload_url);
        }
        if let Some(reload_title) = outcome.reload_title {
            println!("[window-smoke] reload title {}", reload_title);
        }
        if let Some(resize_frame) = outcome.resize_frame {
            println!(
                "[window-smoke] resize frame {}",
                direct_servo::format_duration(resize_frame)
            );
        }
        if let Some(tab_frame) = outcome.tab_frame {
            println!(
                "[window-smoke] tab frame {}",
                direct_servo::format_duration(tab_frame)
            );
        }
        if let Some(tab_url) = outcome.tab_url {
            println!("[window-smoke] tab url {}", tab_url);
        }
        if let Some(tab_title) = outcome.tab_title {
            println!("[window-smoke] tab title {}", tab_title);
        }
    }
    Ok(())
}

#[cfg(feature = "servo-backend")]
fn relaunch_direct_local_appliance_after_trust(
    browser_mode: BrowserMode,
    target: &Url,
    outcome: &direct_servo::DirectServoOutcome,
    certificate_path: Option<&Path>,
    direct_resource_audit: bool,
) -> Result<(), String> {
    let mut args = vec![
        "--browser-mode".to_string(),
        browser_mode.cli_arg().to_string(),
        "--render-path".to_string(),
        "direct".to_string(),
        "--start".to_string(),
        target.as_str().to_string(),
    ];
    if let Some(path) = certificate_path {
        args.extend([
            "--certificate-path".to_string(),
            path.to_string_lossy().to_string(),
        ]);
    }
    if direct_resource_audit {
        args.push("--direct-resource-audit".to_string());
    }
    if outcome.appliance_certificate_remembered == Some(true) {
        if let Some(fingerprint) = outcome.certificate_fingerprint_sha256.as_ref() {
            args.extend([
                "--local-appliance-cert-fingerprint".to_string(),
                fingerprint.clone(),
            ]);
        } else {
            args.push("--allow-insecure-local-tls".to_string());
        }
    } else {
        args.push("--allow-insecure-local-tls".to_string());
    }

    let exe = env::current_exe().map_err(|error| error.to_string())?;
    let mut command = std::process::Command::new(exe);
    command.args(args);
    if let Ok(current_dir) = env::current_dir() {
        command.current_dir(current_dir);
    }
    command
        .spawn()
        .map_err(|error| format!("failed to relaunch trusted local appliance window: {error}"))?;
    Ok(())
}

#[cfg(feature = "servo-backend")]
fn cleanup_or_defer_direct_incognito_data_dir(data_dir: &Path) -> Result<(), String> {
    match cleanup_direct_incognito_data_dir(data_dir) {
        Ok(()) => Ok(()),
        Err(cleanup_error) => {
            let exe = env::current_exe().map_err(|error| {
                format!(
                    "{cleanup_error}; additionally failed to locate cleanup helper executable: {error}"
                )
            })?;
            std::process::Command::new(exe)
                .arg(DIRECT_INCOGNITO_CLEANUP_ARG)
                .arg(data_dir)
                .spawn()
                .map(|_| ())
                .map_err(|error| {
                    format!(
                        "{cleanup_error}; additionally failed to launch cleanup helper: {error}"
                    )
                })
        }
    }
}

fn cleanup_direct_incognito_data_dir(data_dir: &Path) -> Result<(), String> {
    let temp_dir = env::temp_dir();
    let file_name = data_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !data_dir.starts_with(&temp_dir)
        || !file_name.starts_with("sextant-browser-direct-incognito-")
    {
        return Err(format!(
            "refusing to remove unexpected direct incognito data dir {}",
            data_dir.display()
        ));
    }

    let mut last_error = None;
    for _ in 0..50 {
        match std::fs::remove_dir_all(data_dir) {
            Ok(()) => return Ok(()),
            Err(_) if !data_dir.exists() => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    Err(format!(
        "direct incognito data cleanup failed for {}: {}",
        data_dir.display(),
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "unknown cleanup error".to_string())
    ))
}

fn window_hwnd(window: &Window) -> Option<isize> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(feature = "servo-backend"))]
fn run_direct_servo_browser(
    _window_smoke: Option<WindowSmokeSpec>,
    _startup_input: Option<String>,
    _start_showcase: bool,
    _start_real_browsing: bool,
    _start_shell_interaction: bool,
    _start_location_smoke: Option<String>,
    _start_history_smoke: Option<String>,
    _start_first_interaction_smoke: bool,
    _start_verified_input_smoke: bool,
    _start_search_submit_smoke: bool,
    _start_live_search_smoke: bool,
    _start_live_form_smoke: bool,
    _start_live_link_smoke: bool,
    _direct_resource_audit: bool,
    _start_load_smoke: bool,
    _start_reload_smoke: bool,
    _start_resize_smoke: bool,
    _browser_mode: BrowserMode,
    _pre_size_visible_navigation: bool,
    _certificate_path: Option<PathBuf>,
    _local_appliance_cert_fingerprint: Option<String>,
    _remember_local_appliance_cert: Option<String>,
    _allow_insecure_local_tls: bool,
    _embed_parent_hwnd: Option<isize>,
    _embed_x: i32,
    _embed_y: i32,
    _embed_width: Option<u32>,
    _embed_height: Option<u32>,
) -> Result<(), String> {
    Err("direct Servo render path requires the servo-backend feature".to_string())
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
    browser_mode: BrowserMode,
    user_render_path: UserRenderPath,
    pre_size_visible_navigation: bool,
) -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|error| format!("event loop initialization failed: {error}"))?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sextant Browser")
            .with_inner_size(PhysicalSize::new(1180, 760))
            .with_window_icon(sextant_window_icon())
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
    if user_render_path != app.user_render_path {
        app.set_user_render_path(user_render_path)?;
    }
    app.pre_size_visible_navigation = pre_size_visible_navigation;
    app.observation_warmup_enabled = true;
    app.set_browser_mode(browser_mode);
    if !app.last_ok {
        return Err(app.last_status.clone());
    }
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
        println!(
            "[window-smoke] browser mode {} ({})",
            app.browser_mode.label(),
            app.browser_mode.status()
        );
        println!(
            "[window-smoke] render path {} ({})",
            app.render_path_label(),
            app.render_path_status()
        );
        if let Some(gap) = app.direct_render_gap_label() {
            println!("[window-smoke] render gap {gap}");
        }
        println!(
            "[window-smoke] pre-size navigation {}",
            app.pre_size_visible_navigation
        );
        if let Some(data_dir) = app.incognito_data_dir.as_ref() {
            println!(
                "[window-smoke] ephemeral browser data dir {}",
                data_dir.display()
            );
        }
        if let Some(target) = smoke.target.as_ref() {
            pending_start_inputs.push(("window-smoke", target.clone()));
        } else if smoke.input_latency {
            pending_start_inputs.push(("window-smoke", input_latency_fixture_url()));
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
                .is_some()
            || window_smoke
                .as_ref()
                .map(|spec| spec.input_latency)
                .unwrap_or(false));
    let smoke_error = Arc::new(Mutex::new(None::<String>));
    let smoke_error_for_loop = smoke_error.clone();
    let smoke_started = visible_started;
    let mut smoke_first_draw: Option<Duration> = None;
    let mut smoke_first_frame: Option<Duration> = None;
    let mut smoke_first_frame_seen_at: Option<Instant> = None;
    let smoke_requires_user_distill = window_smoke
        .as_ref()
        .map(|spec| spec.user_distill)
        .unwrap_or(false);
    let smoke_requires_input_latency = cfg!(feature = "servo-backend")
        && window_smoke
            .as_ref()
            .map(|spec| spec.input_latency)
            .unwrap_or(false);
    let smoke_input_uses_fixture = window_smoke
        .as_ref()
        .map(|spec| spec.input_latency && spec.target.is_none())
        .unwrap_or(false);
    let mut smoke_user_distill_enqueued = false;
    let mut smoke_user_distill_done = false;
    let mut smoke_input_started: Option<Instant> = None;
    let mut smoke_input_done = false;
    let mut visible_first_draw_seen = false;

    let event_loop_result = event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    app.cleanup_incognito_storage();
                    elwt.exit();
                }
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
                                if smoke_requires_user_distill && !smoke_user_distill_done {
                                    return;
                                }
                                if app.latest_frame.is_some() && smoke_first_frame.is_none() {
                                    smoke_first_frame = Some(smoke_started.elapsed());
                                    smoke_first_frame_seen_at = Some(Instant::now());
                                }
                                if smoke_requires_input_latency && !smoke_input_done {
                                    return;
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
                                println!("[window-smoke] perf {}", app.perf.summary());
                                if let Some(event) = app.slowest_perf_event() {
                                    println!(
                                        "[window-smoke] slowest perf {} {} | {}",
                                        event.phase,
                                        format_duration(event.duration),
                                        event.label
                                    );
                                }
                                let slow_events = app.slowest_perf_events(6);
                                if !slow_events.is_empty() {
                                    let summary = slow_events
                                        .iter()
                                        .map(|event| {
                                            format!(
                                                "{} {} {}",
                                                event.phase,
                                                format_duration(event.duration),
                                                event.label
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" | ");
                                    println!("[window-smoke] slow perf {}", summary);
                                }
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
                                app.cleanup_incognito_storage();
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
                                app.cleanup_incognito_storage();
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
                    let mut started_async = false;
                    match action {
                        Action::Back => {
                            started_async =
                                app.start_visible_navigation_control(PendingNavigationKind::Back);
                        }
                        Action::DistillActive => {
                            started_async = app.start_visible_distillation();
                        }
                        Action::Forward => {
                            started_async =
                                app.start_visible_navigation_control(PendingNavigationKind::Forward);
                        }
                        Action::Reload => {
                            started_async =
                                app.start_visible_navigation_control(PendingNavigationKind::Reload);
                        }
                        _ => {}
                    }
                    if !started_async {
                        visible_startup_work += started.elapsed();
                        app.refresh_logs();
                    }
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                    return;
                }

                if let Some((source, input)) = active_start_input.take() {
                    println!("[{source}] opening {}", input);
                    let started = Instant::now();
                    match start_visible_input(&mut app, &input) {
                        Ok(started_async) => {
                            if !started_async {
                                visible_startup_work += started.elapsed();
                            }
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
                                app.cleanup_incognito_storage();
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
                        app.cleanup_incognito_storage();
                        elwt.exit();
                        return;
                    }
                }

                if smoke_requires_input_latency
                    && smoke_first_frame.is_some()
                    && smoke_first_frame_seen_at
                        .map(|seen| seen.elapsed() >= WINDOW_INPUT_SMOKE_FRAME_SETTLE)
                        .unwrap_or(false)
                    && smoke_input_started.is_none()
                    && !smoke_input_done
                    && app.pending_navigation.is_none()
                    && app.pending_frame_capture.is_none()
                    && app.pending_distillation.is_none()
                    && app.pending_persistence.is_none()
                    && app.pending_wake_search.is_none()
                {
                    let started = Instant::now();
                    let clicked = if smoke_input_uses_fixture {
                        true
                    } else {
                        let (x, y) = rect_center(app.browser_viewport_rect);
                        app.handle_mouse_input(x, y, ElementState::Pressed)
                            && app.handle_mouse_input(x, y, ElementState::Released)
                    };
                    app.queue_browser_text("sextant input smoke");
                    let text_flushed = app.flush_pending_browser_text();
                    let input_flushed = app.flush_viewport_input_lane();
                    if !clicked || !text_flushed || !input_flushed {
                        let error = "window input latency smoke failed to enqueue viewport input";
                        eprintln!("[window-smoke] {error}");
                        if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                            *smoke_error = Some(error.to_string());
                        }
                        app.cleanup_incognito_storage();
                        elwt.exit();
                        return;
                    }
                    println!(
                        "[window-smoke] input enqueue {}",
                        format_duration(started.elapsed())
                    );
                    smoke_input_started = Some(Instant::now());
                    window.request_redraw();
                }

                let mut input_flushed = false;
                if app.flush_pending_browser_text_if_due() {
                    input_flushed = true;
                }
                if app.flush_viewport_input_lane() {
                    input_flushed = true;
                }
                if input_flushed {
                    window.request_redraw();
                }
                if let Some(elapsed) = app.collect_pending_navigation() {
                    visible_startup_work += elapsed;
                    app.refresh_logs();
                    app.update_title(&window);
                    if smoke_requires_user_distill
                        && !smoke_user_distill_enqueued
                        && app.last_ok
                        && app.pending_navigation.is_none()
                    {
                        println!(
                            "[window-user] {}",
                            deferred_user_navigation_label(Action::DistillActive)
                        );
                        let started = Instant::now();
                        smoke_user_distill_enqueued = true;
                        if !app.start_visible_distillation() {
                            visible_startup_work += started.elapsed();
                            if smoke_mode {
                                let error =
                                    format!("window user distill failed: {}", app.last_status);
                                eprintln!("[window-smoke] {error}");
                                if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                    *smoke_error = Some(error);
                                }
                                app.cleanup_incognito_storage();
                                elwt.exit();
                                return;
                            }
                        }
                        println!("[window-user] {}", app.last_status);
                    }
                    window.request_redraw();
                }
                if app.collect_pending_observation_warmup().is_some() {
                    window.request_redraw();
                }
                if smoke_requires_user_distill
                    && !smoke_user_distill_enqueued
                    && active_start_input.is_none()
                    && pending_start_inputs.is_empty()
                    && app.pending_navigation.is_none()
                    && app.last_ok
                {
                    println!(
                        "[window-user] {}",
                        deferred_user_navigation_label(Action::DistillActive)
                    );
                    let started = Instant::now();
                    smoke_user_distill_enqueued = true;
                    if !app.start_visible_distillation() {
                        visible_startup_work += started.elapsed();
                        if smoke_mode {
                            let error = format!("window user distill failed: {}", app.last_status);
                            eprintln!("[window-smoke] {error}");
                            if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                *smoke_error = Some(error);
                            }
                            app.cleanup_incognito_storage();
                            elwt.exit();
                            return;
                        }
                    }
                    println!("[window-user] {}", app.last_status);
                    window.request_redraw();
                }
                if let Some(elapsed) = app.collect_pending_distillation() {
                    visible_startup_work += elapsed;
                    if smoke_requires_user_distill {
                        if app.last_ok {
                            if app.pending_persistence.is_none() {
                                smoke_user_distill_done = true;
                            }
                        } else if smoke_mode {
                            let error = format!("window user distill failed: {}", app.last_status);
                            eprintln!("[window-smoke] {error}");
                            if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                *smoke_error = Some(error);
                            }
                            app.cleanup_incognito_storage();
                            elwt.exit();
                            return;
                        }
                    }
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                if let Some(elapsed) = app.collect_pending_persistence() {
                    visible_startup_work += elapsed;
                    if smoke_requires_user_distill {
                        if app.last_ok {
                            smoke_user_distill_done = true;
                        } else if smoke_mode {
                            let error =
                                format!("window user persistence failed: {}", app.last_status);
                            eprintln!("[window-smoke] {error}");
                            if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                *smoke_error = Some(error);
                            }
                            app.cleanup_incognito_storage();
                            elwt.exit();
                            return;
                        }
                    }
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                if app.collect_pending_wake_search().is_some() {
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                if app.collect_pending_log_writes() {
                    app.update_title(&window);
                    window.request_redraw();
                }
                if app.collect_pending_log_refresh() {
                    app.update_title(&window);
                    window.request_redraw();
                }
                #[cfg(feature = "xilem-shell")]
                if app.collect_pending_pilot_plan().is_some() {
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                if app.collect_pending_frame_capture() {
                    if let Some(started) = smoke_input_started {
                        if smoke_requires_input_latency && !smoke_input_done {
                            smoke_input_done = true;
                            println!(
                                "[window-smoke] input frame {}",
                                format_duration(started.elapsed())
                            );
                        }
                    }
                    window.request_redraw();
                }
                if app.maybe_refresh_frame() {
                    window.request_redraw();
                }
                if app.maybe_start_scheduled_observation_warmup() {
                    window.request_redraw();
                }
                if smoke_requires_input_latency
                    && smoke_input_started.is_none()
                    && !smoke_input_done
                    && smoke_first_frame_seen_at.is_some()
                {
                    let wake_at = smoke_first_frame_seen_at
                        .and_then(|seen| seen.checked_add(WINDOW_INPUT_SMOKE_FRAME_SETTLE))
                        .unwrap_or_else(Instant::now);
                    elwt.set_control_flow(ControlFlow::WaitUntil(wake_at));
                } else if app.pending_frame_capture.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if app.pending_distillation.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if app.pending_persistence.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if app.pilot_plan_pending() {
                    // The local-model plan can take seconds; keep the loop ticking
                    // coarsely so `collect_pending_pilot_plan` runs without input.
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(50),
                    ));
                } else if app.pending_wake_search.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if !app.pending_log_writes.is_empty() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(50),
                    ));
                } else if app.pending_log_refresh.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(50),
                    ));
                } else if !app.pending_viewport_input.is_empty() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(1),
                    ));
                } else if let Some(due) = app.pending_browser_text_due() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(due));
                } else if app.pending_navigation.is_some() {
                    window.request_redraw();
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if app.pending_observation_warmup.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if let Some(due) = app.scheduled_observation_warmup_due() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(due));
                } else if app.main_view == MainView::Browser
                    && (app.latest_frame.is_some() || app.frame_refresh_budget > 0)
                {
                    let delay = if app.frame_refresh_budget > 0 {
                        app.viewport_input_capture_delay()
                            .unwrap_or(FRAME_REFRESH_INTERACTION)
                    } else if app.frame_dirty {
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
        .map_err(|error| format!("event loop failed: {error}"));

    event_loop_result?;

    if let Ok(mut smoke_error) = smoke_error.lock() {
        if let Some(error) = smoke_error.take() {
            return Err(error);
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "browser/tests.rs"]
mod tests;

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
