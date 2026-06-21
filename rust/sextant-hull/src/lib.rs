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
pub mod direct_servo;

// Palette, layout, and timing constants live in the theme submodule (first cut
// of the browser.rs split). Glob-imported so existing unqualified references
// (BG, PANEL, CHROME_H, ...) keep resolving unchanged.
#[path = "browser/theme.rs"]
mod theme;
pub use theme::*;
#[path = "browser/draw.rs"]
mod draw;
pub use draw::*;
#[path = "browser/harness.rs"]
mod harness;
pub use harness::*;
#[path = "browser/model.rs"]
mod model;
pub use model::*;

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
#[path = "browser/egui_retro.rs"]
mod egui_retro;
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub use egui_retro::{apply_retro_egui_theme, run_retro_egui_proof};

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
pub use egui_bridge::run_visible_app_egui;

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
#[path = "browser/egui_hosted.rs"]
mod egui_hosted;
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub use egui_hosted::run_hosted_direct_app_egui;
#[path = "browser/hosted_direct.rs"]
mod hosted_direct;
pub use hosted_direct::*;

#[path = "browser/lanes.rs"]
mod lanes;
pub use lanes::PersistenceLane;
#[cfg(feature = "xilem-shell")]
pub use lanes::PilotBrainLane;

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

pub struct BrowserApp {
    pub engine: SextantEngine,
    pub wake: DigitalWake,
    pub log: CaptainsLog,
    pub persistence_lane: PersistenceLane,
    pub data_dir: PathBuf,
    pub profile_data_dir: PathBuf,
    pub incognito_data_dir: Option<PathBuf>,
    #[cfg(feature = "xilem-shell")]
    pub consent_vault: CitadelVault,
    #[cfg(feature = "xilem-shell")]
    pub guard_firewall: SextantFirewall,
    #[cfg(feature = "xilem-shell")]
    pub guard_policy_source: String,
    pub persona_id: String,
    pub address_input: String,
    pub wake_query: String,
    pub last_status: String,
    pub last_ok: bool,
    pub defer_user_navigation: bool,
    pub observation_warmup_enabled: bool,
    pub pre_size_visible_navigation: bool,
    pub pending_user_navigation: Option<String>,
    pub pending_user_action: Option<Action>,
    pub pending_navigation: Option<PendingNavigation>,
    pub scheduled_observation_warmup: Option<ScheduledObservationWarmup>,
    pub pending_observation_warmup: Option<PendingObservationWarmup>,
    pub pending_distillation: Option<PendingDistillation>,
    pub pending_persistence: Option<PendingPersistence>,
    pub pending_wake_search: Option<PendingWakeSearch>,
    pub pending_log_writes: Vec<PendingLogWrite>,
    pub pending_log_refresh: Option<PendingLogRefresh>,
    pub browser_mode: BrowserMode,
    pub user_render_path: UserRenderPath,
    pub perf: BrowserPerf,
    pub perf_events: Vec<PerfEvent>,
    pub last_intent: String,
    pub pilot_status: String,
    pub pilot_plan: Vec<String>,
    pub pilot_result: String,
    pub pending_consent: Option<PendingConsent>,
    pub showcase_report: Vec<String>,
    pub cursor: Option<(f64, f64)>,
    pub modifiers: ModifiersState,
    pub focus: FocusTarget,
    pub address_rect: Rect,
    pub wake_rect: Rect,
    pub browser_tab_rect: Rect,
    pub wake_tab_rect: Rect,
    pub log_tab_rect: Rect,
    pub guard_tab_rect: Rect,
    pub perception_tab_rect: Rect,
    pub perf_tab_rect: Rect,
    pub validation_tab_rect: Rect,
    pub browser_viewport_rect: Rect,
    pub buttons: Vec<ButtonRegion>,
    pub mode_rects: Vec<ModeRegion>,
    pub page_tab_rects: Vec<PageTabRegion>,
    pub page_tab_prev_rect: Rect,
    pub page_tab_next_rect: Rect,
    pub consent_authorize_rect: Rect,
    pub consent_deny_rect: Rect,
    pub page_tab_window_start: usize,
    pub window_size: PhysicalSize<u32>,
    pub wake_results: Vec<WakeEntry>,
    pub recent_logs: Vec<LogEntry>,
    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    pub appliance_cert_entries: Vec<ApplianceCertTrustEntry>,
    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    pub selected_appliance_cert: Option<usize>,
    pub appliance_cert_status: String,
    pub page_scroll: i32,
    pub main_view: MainView,
    pub latest_frame: Option<RenderedFrame>,
    pub pending_frame_capture: Option<PendingFrameCapture>,
    pub last_frame_viewport: Option<(u32, u32)>,
    pub last_frame_refresh: Instant,
    pub last_viewport_mouse_move_forward: Option<Instant>,
    pub last_viewport_mouse_move_point: Option<(f32, f32)>,
    pub pending_browser_text_tab: Option<Uuid>,
    pub pending_browser_text: String,
    pub last_browser_text_input: Option<Instant>,
    pub pending_viewport_input_tab: Option<Uuid>,
    pub pending_viewport_input: VecDeque<ViewportInputEvent>,
    pub last_viewport_input_flush: Option<Instant>,
    pub frame_dirty: bool,
    pub frame_refresh_budget: u8,
    pub validation: ValidationState,
    #[cfg(feature = "xilem-shell")]
    pub pilot_brain_lane: PilotBrainLane,
    #[cfg(feature = "xilem-shell")]
    pub pending_pilot_plan: Option<PendingPilotPlan>,
    // Latest structured pilot run, retained as the hook a future visual dashboard /
    // MCP reads; the human-readable view already renders via pilot_plan/result.
    #[cfg(feature = "xilem-shell")]
    #[allow(dead_code)]
    pub last_pilot_run: Option<PilotRunArtifact>,
    #[cfg(feature = "xilem-shell")]
    pub ai_config: AiLocalConfig,
    #[cfg(feature = "xilem-shell")]
    pub settings_tab_rect: Rect,
}

impl BrowserApp {
    pub fn new() -> Result<Self, String> {
        Self::new_with_data_dir(app_data_dir().join("browser"))
    }

    pub fn new_with_data_dir(data_dir: PathBuf) -> Result<Self, String> {
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

    pub fn update_title(&self, window: &Window) {
        window.set_title(&format!("Sextant Browser - {}", self.last_status));
    }

    pub fn capabilities(&self) -> BrowserCapabilities {
        self.browser_mode.capabilities()
    }

    pub fn ai_status_label(&self) -> &'static str {
        match self.browser_mode {
            BrowserMode::Agent => "CONTROL",
            BrowserMode::Assisted => "ASSIST",
            BrowserMode::Observe => "OBSERVE",
            BrowserMode::Direct | BrowserMode::Incognito => "OFF",
        }
    }

    pub fn storage_status_label(&self) -> &'static str {
        if self.incognito_data_dir.is_some() {
            "EPHEMERAL"
        } else {
            "PROFILE"
        }
    }

    pub fn render_path_label(&self) -> &'static str {
        self.user_render_path.label()
    }

    pub fn render_path_status(&self) -> &'static str {
        self.user_render_path.status()
    }

    pub fn direct_render_gap_label(&self) -> Option<&'static str> {
        if self.capabilities().direct_render_required
            && self.user_render_path == UserRenderPath::FrameBridge
        {
            Some("direct compositor pending")
        } else {
            None
        }
    }

    pub fn set_user_render_path(&mut self, path: UserRenderPath) -> Result<(), String> {
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

    pub fn native_intent_allowed(&self) -> bool {
        matches!(
            self.browser_mode,
            BrowserMode::Agent | BrowserMode::Assisted
        )
    }

    pub fn proof_workflow_allowed(&self) -> bool {
        self.native_intent_allowed()
    }

    pub fn block_mode_control_work(&mut self, label: &str) {
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

    pub fn set_browser_mode(&mut self, mode: BrowserMode) {
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

    pub fn enter_incognito_storage(&mut self) -> Result<(), String> {
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

    pub fn leave_incognito_storage(&mut self) -> Result<(), String> {
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

    pub fn reopen_persistent_stores(&mut self, data_dir: &Path) -> Result<(), String> {
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

    pub fn cleanup_incognito_storage(&mut self) {
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

    pub fn layout(&mut self, size: PhysicalSize<u32>) {
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

    pub fn reset_validation(&mut self) {
        self.validation = ValidationState::default();
        self.last_status = "Validation session reset. Runtime state is unchanged.".to_string();
        self.last_ok = true;
    }
}

pub fn app_data_dir() -> PathBuf {
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
pub struct ApplianceCertTrustEntry {
    pub origin: String,
    pub fingerprint_sha256: String,
    pub label: Option<String>,
    pub created_at: String,
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplianceCertTrustStore {
    pub version: u32,
    pub entries: Vec<ApplianceCertTrustEntry>,
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
    pub fn load(data_dir: &Path) -> Result<Self, String> {
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

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
        let path = appliance_cert_trust_path(data_dir);
        let contents = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to encode {}: {}", path.display(), error))?;
        std::fs::write(&path, contents)
            .map_err(|error| format!("failed to write {}: {}", path.display(), error))
    }

    pub fn remember(
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

    pub fn is_trusted(&self, target: &Url, fingerprint_sha256: &str) -> Result<bool, String> {
        let origin = appliance_origin(target)?;
        let fingerprint_sha256 = normalize_certificate_fingerprint(fingerprint_sha256)?;
        Ok(self
            .entries
            .iter()
            .any(|entry| entry.origin == origin && entry.fingerprint_sha256 == fingerprint_sha256))
    }

    pub fn forget(&mut self, target: &Url) -> Result<bool, String> {
        let origin = appliance_origin(target)?;
        let before = self.entries.len();
        self.entries.retain(|entry| entry.origin != origin);
        Ok(self.entries.len() != before)
    }

    pub fn clear(&mut self) -> bool {
        let had_entries = !self.entries.is_empty();
        self.entries.clear();
        had_entries
    }
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub fn appliance_cert_trust_path(data_dir: &Path) -> PathBuf {
    data_dir.join(APPLIANCE_CERT_TRUST_FILE)
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub fn normalize_certificate_fingerprint(value: &str) -> Result<String, String> {
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
pub fn appliance_origin(target: &Url) -> Result<String, String> {
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
pub fn handle_list_local_appliance_certs_arg(args: &[String]) -> Result<bool, String> {
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
pub fn handle_list_local_appliance_certs_arg(args: &[String]) -> Result<bool, String> {
    if args.iter().any(|arg| arg == "--list-local-appliance-certs") {
        return Err(
            "local appliance certificate trust storage is unavailable in this build".to_string(),
        );
    }
    Ok(false)
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub fn handle_forget_local_appliance_cert_arg(args: &[String]) -> Result<bool, String> {
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
pub fn handle_forget_local_appliance_cert_arg(args: &[String]) -> Result<bool, String> {
    if operator_arg_value(args, "--forget-local-appliance-cert").is_some() {
        return Err(
            "local appliance certificate trust storage is unavailable in this build".to_string(),
        );
    }
    Ok(false)
}

#[cfg(feature = "xilem-shell")]
pub fn load_browser_guard_firewall(data_dir: &Path) -> Result<(SextantFirewall, String), String> {
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
pub fn init_browser_consent_vault() -> Result<CitadelVault, String> {
    let mut vault = CitadelVault::new();
    let _mnemonic = vault.initialize_new("browser-captains-key")?;
    Ok(vault)
}

pub fn window_hwnd(window: &Window) -> Option<isize> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "browser/tests.rs"]
mod tests;

pub fn browser_key_from_winit(key: &NamedKey) -> Option<BrowserKey> {
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

pub fn browser_shortcut_for_key(modifiers: ModifiersState, key: &Key) -> Option<BrowserShortcut> {
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

pub fn short_url(url: &Url) -> String {
    let value = url.to_string();
    if value.len() > 58 {
        format!("{}...", &value[..55])
    } else {
        value
    }
}

pub fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

pub fn fmt_duration(duration: Option<Duration>) -> String {
    duration
        .map(format_duration)
        .unwrap_or_else(|| "pending".to_string())
}

pub fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis >= 1000 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        format!("{millis}ms")
    }
}

pub fn page_tab_label(tab: &Tab) -> String {
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

pub fn backend(status: EngineStatus) -> &'static str {
    match status.active_backend {
        EngineBackend::Servo => "Servo",
        EngineBackend::Gecko => "Gecko",
        EngineBackend::Chromium => "Chromium",
    }
}

pub fn display_backend(app: &BrowserApp, tab: &Tab) -> &'static str {
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

pub fn render_action_label() -> &'static str {
    if cfg!(feature = "servo-backend") {
        "Opened"
    } else {
        "Tracked"
    }
}

pub fn is_deferred_user_navigation_action(action: Action) -> bool {
    matches!(
        action,
        Action::Back | Action::Forward | Action::Reload | Action::DistillActive
    )
}

pub fn deferred_user_navigation_status(action: Action) -> &'static str {
    match action {
        Action::Back => "Going back...",
        Action::DistillActive => "Distilling active page...",
        Action::Forward => "Going forward...",
        Action::Reload => "Reloading active tab...",
        _ => "Opening...",
    }
}

pub fn deferred_user_navigation_label(action: Action) -> &'static str {
    match action {
        Action::Back => "back",
        Action::DistillActive => "distill",
        Action::Forward => "forward",
        Action::Reload => "reload",
        _ => "navigation",
    }
}
