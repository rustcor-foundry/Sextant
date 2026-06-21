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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrowserMode {
    Agent,
    Assisted,
    Observe,
    Direct,
    Incognito,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UserRenderPath {
    FrameBridge,
    DirectServo,
}

impl UserRenderPath {
    fn label(self) -> &'static str {
        match self {
            UserRenderPath::FrameBridge => "BRIDGE",
            UserRenderPath::DirectServo => "DIRECT",
        }
    }

    fn status(self) -> &'static str {
        match self {
            UserRenderPath::FrameBridge => {
                "temporary frame-capture render bridge feeding Softbuffer"
            }
            UserRenderPath::DirectServo => {
                "direct Servo WindowRenderingContext raw window; shell chrome compositor pending"
            }
        }
    }

    fn integrated(self) -> bool {
        matches!(self, UserRenderPath::FrameBridge)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BrowserCapabilities {
    ai_control: bool,
    ai_observe_dom: bool,
    ai_observe_frame: bool,
    read_wake: bool,
    write_wake: bool,
    write_log_content: bool,
    expose_mcp_tools: bool,
    direct_render_required: bool,
}

impl BrowserMode {
    const ALL: [BrowserMode; 5] = [
        BrowserMode::Agent,
        BrowserMode::Assisted,
        BrowserMode::Observe,
        BrowserMode::Direct,
        BrowserMode::Incognito,
    ];

    fn label(self) -> &'static str {
        match self {
            BrowserMode::Agent => "AGENT",
            BrowserMode::Assisted => "ASSIST",
            BrowserMode::Observe => "OBSERVE",
            BrowserMode::Direct => "DIRECT",
            BrowserMode::Incognito => "INCOG",
        }
    }

    fn status(self) -> &'static str {
        match self {
            BrowserMode::Agent => "Agent Mode: AI can observe, control, persist, and expose tools.",
            BrowserMode::Assisted => {
                "Assisted Mode: human first, AI sidecar observation with consent for actions."
            }
            BrowserMode::Observe => "Observe Mode: AI can observe but cannot act.",
            BrowserMode::Direct => "Direct Mode: human performance path, AI observation disabled.",
            BrowserMode::Incognito => {
                "Incognito Mode: human-only, no AI observation or content persistence."
            }
        }
    }

    fn capabilities(self) -> BrowserCapabilities {
        match self {
            BrowserMode::Agent => BrowserCapabilities {
                ai_control: true,
                ai_observe_dom: true,
                ai_observe_frame: true,
                read_wake: true,
                write_wake: true,
                write_log_content: true,
                expose_mcp_tools: true,
                direct_render_required: false,
            },
            BrowserMode::Assisted => BrowserCapabilities {
                ai_control: false,
                ai_observe_dom: true,
                ai_observe_frame: true,
                read_wake: true,
                write_wake: true,
                write_log_content: true,
                expose_mcp_tools: true,
                direct_render_required: false,
            },
            BrowserMode::Observe => BrowserCapabilities {
                ai_control: false,
                ai_observe_dom: true,
                ai_observe_frame: true,
                read_wake: true,
                write_wake: false,
                write_log_content: false,
                expose_mcp_tools: true,
                direct_render_required: false,
            },
            BrowserMode::Direct => BrowserCapabilities {
                ai_control: false,
                ai_observe_dom: false,
                ai_observe_frame: false,
                read_wake: false,
                write_wake: false,
                write_log_content: false,
                expose_mcp_tools: false,
                direct_render_required: true,
            },
            BrowserMode::Incognito => BrowserCapabilities {
                ai_control: false,
                ai_observe_dom: false,
                ai_observe_frame: false,
                read_wake: false,
                write_wake: false,
                write_log_content: false,
                expose_mcp_tools: false,
                direct_render_required: true,
            },
        }
    }

    fn cli_arg(self) -> &'static str {
        match self {
            BrowserMode::Agent => "agent",
            BrowserMode::Assisted => "assisted",
            BrowserMode::Observe => "observe",
            BrowserMode::Direct => "direct",
            BrowserMode::Incognito => "incognito",
        }
    }
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
    #[cfg(feature = "xilem-shell")]
    Settings,
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
    user_distill: bool,
    input_latency: bool,
    first_interaction_smoke: bool,
    verified_input_smoke: bool,
    search_submit_smoke: bool,
    live_search_smoke: bool,
    live_form_smoke: bool,
    live_link_smoke: bool,
    load_smoke: bool,
    resize_smoke: bool,
    allow_insecure_local_tls: bool,
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

struct ModeRegion {
    rect: Rect,
    mode: BrowserMode,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameCapturePurpose {
    RenderBridge,
    AiObservation,
}

impl FrameCapturePurpose {
    fn queue_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "capture queue render bridge",
            FrameCapturePurpose::AiObservation => "capture queue ai observation",
        }
    }

    fn capture_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "capture render bridge",
            FrameCapturePurpose::AiObservation => "capture ai observation",
        }
    }

    fn capture_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge capture failed",
            FrameCapturePurpose::AiObservation => "ai observation capture failed",
        }
    }

    fn capture_start_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge start failed",
            FrameCapturePurpose::AiObservation => "ai observation start failed",
        }
    }

    fn capture_dropped_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge capture dropped",
            FrameCapturePurpose::AiObservation => "ai observation capture dropped",
        }
    }

    fn resize_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge",
            FrameCapturePurpose::AiObservation => "viewport ai observation",
        }
    }

    fn resize_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge failed",
            FrameCapturePurpose::AiObservation => "viewport ai observation failed",
        }
    }

    fn resize_async_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge async",
            FrameCapturePurpose::AiObservation => "viewport ai observation async",
        }
    }

    fn resize_async_missing_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge async missing",
            FrameCapturePurpose::AiObservation => "viewport ai observation async missing",
        }
    }
}

struct PendingFrameCapture {
    tab_id: Uuid,
    result_rx: mpsc::Receiver<Result<AsyncFrameCapture, String>>,
    started: Instant,
    viewport_size: (u32, u32),
    requested_resize: bool,
    purpose: FrameCapturePurpose,
}

struct PendingNavigation {
    url: Url,
    kind: PendingNavigationKind,
    result_rx: mpsc::Receiver<Result<AsyncNavigationResult, String>>,
    started: Instant,
    viewport_size: Option<(u32, u32)>,
}

struct PendingObservationWarmup {
    tab_id: Uuid,
    result_rx: mpsc::Receiver<Result<BrowserEvalProbe, String>>,
    started: Instant,
}

struct ScheduledObservationWarmup {
    tab_id: Uuid,
    due: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum PendingNavigationKind {
    Open,
    Reload,
    Back,
    Forward,
}

impl PendingNavigationKind {
    fn pending_status(self, url: &Url) -> String {
        match self {
            PendingNavigationKind::Open => format!("Opening {} with Servo...", short_url(url)),
            PendingNavigationKind::Reload => "Reloading active tab with Servo...".to_string(),
            PendingNavigationKind::Back => "Going back with Servo...".to_string(),
            PendingNavigationKind::Forward => "Going forward with Servo...".to_string(),
        }
    }

    fn success_status(self, final_url: &Url, backend_name: &str, elapsed: Duration) -> String {
        match self {
            PendingNavigationKind::Open => format!(
                "{} {} with {} in {}. Frame capture queued.",
                render_action_label(),
                short_url(final_url),
                backend_name,
                format_duration(elapsed)
            ),
            PendingNavigationKind::Reload => format!(
                "Reloaded active tab with {} in {}. Frame capture queued.",
                backend_name,
                format_duration(elapsed)
            ),
            PendingNavigationKind::Back => format!(
                "Went back with {} in {}. Frame capture queued.",
                backend_name,
                format_duration(elapsed)
            ),
            PendingNavigationKind::Forward => format!(
                "Went forward with {} in {}. Frame capture queued.",
                backend_name,
                format_duration(elapsed)
            ),
        }
    }

    fn failure_status(self, error: &str) -> String {
        match self {
            PendingNavigationKind::Open => format!("Open failed: {}", error),
            PendingNavigationKind::Reload => format!("Reload failed: {}", error),
            PendingNavigationKind::Back => format!("Back unavailable: {}", error),
            PendingNavigationKind::Forward => format!("Forward unavailable: {}", error),
        }
    }

    fn perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open async",
            PendingNavigationKind::Reload => "reload async",
            PendingNavigationKind::Back => "back async",
            PendingNavigationKind::Forward => "forward async",
        }
    }

    fn failure_perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open async failed",
            PendingNavigationKind::Reload => "reload async failed",
            PendingNavigationKind::Back => "back async failed",
            PendingNavigationKind::Forward => "forward async failed",
        }
    }

    fn dropped_perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open worker dropped",
            PendingNavigationKind::Reload => "reload worker dropped",
            PendingNavigationKind::Back => "back worker dropped",
            PendingNavigationKind::Forward => "forward worker dropped",
        }
    }

    fn phase_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open",
            PendingNavigationKind::Reload => "reload",
            PendingNavigationKind::Back => "back",
            PendingNavigationKind::Forward => "forward",
        }
    }

    fn log_intent(self, url: &Url) -> String {
        match self {
            PendingNavigationKind::Open => format!("navigate {}", url),
            PendingNavigationKind::Reload => "reload".to_string(),
            PendingNavigationKind::Back => "back".to_string(),
            PendingNavigationKind::Forward => "forward".to_string(),
        }
    }
}

struct PendingDistillation {
    result_rx: mpsc::Receiver<Result<AsyncDistillResult, String>>,
    started: Instant,
}

struct PendingPersistence {
    result_rx: mpsc::Receiver<Result<PersistenceDistillResult, String>>,
    started: Instant,
    page_title: String,
}

struct PendingWakeSearch {
    result_rx: mpsc::Receiver<Result<PersistenceWakeSearchResult, String>>,
    started: Instant,
    query: String,
}

struct PendingLogWrite {
    result_rx: mpsc::Receiver<Result<PersistenceLogResult, String>>,
    started: Instant,
    intent: String,
}

struct PendingLogRefresh {
    result_rx: mpsc::Receiver<Result<PersistenceLogResult, String>>,
    started: Instant,
}

enum ViewportInputEvent {
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseButton {
        x: f32,
        y: f32,
        pressed: bool,
    },
    Wheel {
        delta_x: f64,
        delta_y: f64,
        pixel_mode: bool,
    },
    KeyNamed {
        key: BrowserKey,
        pressed: bool,
    },
    Text {
        text: String,
    },
}

struct PersistenceDistillResult {
    wake_results: Vec<WakeEntry>,
    recent_logs: Vec<LogEntry>,
}

struct PersistenceWakeSearchResult {
    wake_results: Vec<WakeEntry>,
    recent_logs: Vec<LogEntry>,
}

struct PersistenceLogResult {
    recent_logs: Vec<LogEntry>,
}

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

/// User-configurable local AI backend selection, persisted to
/// `<profile>/ai-provider.json` and editable from the Settings tab. Resolution
/// order on load: file -> env overrides -> built-in defaults.
#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AiLocalConfig {
    backend: String,
    endpoint: String,
    model: String,
}

#[cfg(feature = "xilem-shell")]
impl AiLocalConfig {
    const BACKENDS: [&'static str; 3] = ["llamacpp", "ollama", "vllm"];

    fn default_for(backend: &str) -> Self {
        let (endpoint, model) = match backend {
            "ollama" => ("http://127.0.0.1:11434", "qwen2.5:3b"),
            "vllm" => ("http://127.0.0.1:8000", "qwen2.5-coder-32b"),
            // llama.cpp default points at the on-box 32B server.
            _ => ("http://127.0.0.1:8101", "qwen2.5-coder-32b"),
        };
        Self {
            backend: backend.to_string(),
            endpoint: endpoint.to_string(),
            model: model.to_string(),
        }
    }

    fn config_path(profile_dir: &Path) -> PathBuf {
        profile_dir.join("ai-provider.json")
    }

    fn load(profile_dir: &Path) -> Self {
        let mut config = std::fs::read_to_string(Self::config_path(profile_dir))
            .ok()
            .and_then(|raw| serde_json::from_str::<AiLocalConfig>(&raw).ok())
            .unwrap_or_else(|| Self::default_for("llamacpp"));
        // Environment overrides win so a launch can target a different server
        // without rewriting the file.
        if let Some(value) = env_override("SEXTANT_LOCAL_BACKEND") {
            config.backend = value.to_lowercase();
        }
        if let Some(value) = env_override("SEXTANT_LOCAL_ENDPOINT") {
            config.endpoint = value;
        }
        if let Some(value) = env_override("SEXTANT_LOCAL_MODEL") {
            config.model = value;
        }
        config.normalize();
        config
    }

    fn save(&self, profile_dir: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(Self::config_path(profile_dir), json).map_err(|error| error.to_string())
    }

    fn normalize(&mut self) {
        self.backend = self.backend.trim().to_lowercase();
        if !Self::BACKENDS.contains(&self.backend.as_str()) {
            self.backend = "llamacpp".to_string();
        }
        self.endpoint = self.endpoint.trim().to_string();
        self.model = self.model.trim().to_string();
    }

    fn backend_enum(&self) -> InferenceBackend {
        match self.backend.as_str() {
            "ollama" => InferenceBackend::Ollama,
            "vllm" => InferenceBackend::VLLM,
            _ => InferenceBackend::LlamaCpp,
        }
    }

    fn backend_label(backend: &str) -> &'static str {
        match backend {
            "ollama" => "Ollama",
            "vllm" => "vLLM",
            _ => "llama.cpp",
        }
    }
}

#[cfg(feature = "xilem-shell")]
fn env_override(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(feature = "xilem-shell")]
struct PendingPilotPlan {
    intent: String,
    result_rx: mpsc::Receiver<Result<Vec<PilotAction>, String>>,
    started: Instant,
}

/// Structured record of one pilot run: intent -> typed plan -> analysis -> status.
/// Serializable so the egui shell can render it today and a future visual
/// dashboard (or MCP) can consume the same stream without a rewrite.
#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize)]
struct PilotStepRecord {
    kind: String,
    detail: String,
}

#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize)]
struct PilotRunArtifact {
    intent: String,
    planner: String,
    plan: Vec<PilotStepRecord>,
    analysis: Vec<String>,
    status: String,
    reason_ms: u128,
}

#[cfg(feature = "xilem-shell")]
fn pilot_action_record(action: &PilotAction) -> PilotStepRecord {
    let (kind, detail) = match action {
        PilotAction::Navigate(url) => ("navigate", short_url(url)),
        PilotAction::OpenTab(url) => ("open_tab", short_url(url)),
        PilotAction::Distill => ("distill", String::new()),
        PilotAction::Perceive => ("perceive", String::new()),
        PilotAction::PerceiveMultiModal => ("perceive_multimodal", String::new()),
        PilotAction::Analyze(message) => ("analyze", message.clone()),
        PilotAction::RequestConsent(message) => ("request_consent", message.clone()),
        PilotAction::SwitchTab(tab_id) => ("switch_tab", short_id(*tab_id)),
        PilotAction::CloseTab(tab_id) => ("close_tab", short_id(*tab_id)),
    };
    PilotStepRecord {
        kind: kind.to_string(),
        detail,
    }
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

fn run_hosted_direct_app(
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
                        resize_hosted_direct_child(parent_hwnd, child, size);
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
                        if let Some(fingerprint) =
                            direct_summary.certificate_fingerprint_sha256.clone()
                        {
                            let relaunch_allow_insecure_local_tls = match trust {
                                HostedDirectCertificateTrust::Once => {
                                    true
                                }
                                HostedDirectCertificateTrust::Remember => {
                                    hosted_local_appliance_cert_fingerprint = Some(fingerprint);
                                    false
                                }
                            };
                            let restart_target = direct_summary
                                .latest_url
                                .as_deref()
                                .unwrap_or(shell.target.as_str())
                                .to_string();
                            match spawn_hosted_direct_child(
                                parent_hwnd,
                                latest_parent_size,
                                &restart_target,
                                browser_mode,
                                certificate_path.as_deref(),
                                hosted_local_appliance_cert_fingerprint.as_deref(),
                                remember_local_appliance_cert.as_deref(),
                                relaunch_allow_insecure_local_tls,
                                direct_resource_audit,
                            ) {
                                Ok(child) => {
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
                        } else {
                            eprintln!(
                                "[hosted-direct] child exited after certificate trust request without a fingerprint: {status}"
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
                        if resize_hosted_direct_child(parent_hwnd, child, latest_parent_size) {
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
const HOSTED_DIRECT_CHROME_POINTS: f32 = 108.0;

/// A saved bookmark shown in the hosted-direct chrome bookmarks bar.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
#[derive(Clone, Serialize, Deserialize)]
struct HostedDirectBookmark {
    title: String,
    url: String,
}

/// An action requested from the egui chrome for the event loop to apply.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
enum HostedDirectChromeAction {
    Child(String),
    SelectTab(usize),
    CloseTab(usize),
    AddBookmark,
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn hosted_direct_bookmarks_path() -> PathBuf {
    app_data_dir().join("browser").join("bookmarks.json")
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn load_hosted_direct_bookmarks() -> Vec<HostedDirectBookmark> {
    std::fs::read_to_string(hosted_direct_bookmarks_path())
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn save_hosted_direct_bookmarks(bookmarks: &[HostedDirectBookmark]) {
    let path = hosted_direct_bookmarks_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(bookmarks) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn truncate_label(text: &str, max: usize) -> String {
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
fn position_hosted_direct_child(
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
struct ChromeTextureStore {
    textures: std::collections::HashMap<egui::TextureId, ChromeTexture>,
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
struct ChromeTexture {
    w: usize,
    h: usize,
    px: Vec<egui::Color32>,
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
impl ChromeTextureStore {
    fn apply(&mut self, delta: &egui::TexturesDelta) {
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

    fn get(&self, id: egui::TextureId) -> Option<&ChromeTexture> {
        self.textures.get(&id)
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn chrome_sample_bilinear(tex: &ChromeTexture, u: f32, v: f32) -> (f32, f32, f32, f32) {
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
fn chrome_edge(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)
}

/// Software-rasterize egui's tessellated output into a `0x00RRGGBB` pixel buffer.
/// Triangles are barycentric-filled with per-vertex colour, bilinear texture
/// sampling, and premultiplied-alpha "over" blending. Only rows `< max_y` are
/// touched so we never paint into the embedded child window's area below the
/// chrome. This keeps the chrome on a pure-CPU path (no D3D/WARP device), which
/// is essential on GPU-less hosts where wgpu's DX12+WARP rasterizer pool spins.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn rasterize_chrome(
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
struct GpuChrome {
    _instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: egui_wgpu::Renderer,
}

/// CPU-backed chrome: egui software-rasterized into a softbuffer surface.
/// Selected on GPU-less hosts where wgpu would fall back to the WARP software
/// device, whose rasterizer thread pool spins even while idle.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
struct SoftwareChrome {
    // Declared before `_context` so the surface is dropped first.
    surface: Surface<Arc<Window>, Arc<Window>>,
    _context: Context<Arc<Window>>,
    textures: ChromeTextureStore,
    surface_size: Option<(u32, u32)>,
}

/// The selected chrome render backend. The web content is always rendered by the
/// Servo child (which uses the GPU when available); this only governs the chrome
/// strip drawn by the parent.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
enum ChromeBackend {
    Gpu(GpuChrome),
    Software(SoftwareChrome),
}

/// Choose the chrome render backend: prefer a hardware GPU (wgpu/DX12), and fall
/// back to a pure-CPU softbuffer surface when the only adapter is WARP (the
/// "Microsoft Basic Render Driver", reported as `DeviceType::Cpu`) or none. This
/// keeps GPU rendering on capable machines while avoiding the WARP idle spin on
/// GPU-less hosts (e.g. Windows Server / RDP).
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn init_chrome_backend(window: &Arc<Window>) -> Result<ChromeBackend, String> {
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
    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if let ChromeBackend::Gpu(gpu) = self {
            if new_size.width > 0 && new_size.height > 0 {
                gpu.config.width = new_size.width;
                gpu.config.height = new_size.height;
                gpu.surface.configure(&gpu.device, &gpu.config);
            }
        }
        // The software backend resizes its softbuffer surface lazily in `render`.
    }

    fn render(
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

#[derive(Clone, Debug, Default, PartialEq)]
struct HostedDirectLogSummary {
    first_present: Option<String>,
    latest_load: Option<String>,
    latest_swap: Option<String>,
    latest_input: Option<String>,
    latest_url: Option<String>,
    certificate_fingerprint_sha256: Option<String>,
    active_title: Option<String>,
    active_tab_index: Option<usize>,
    tab_count: Option<usize>,
    can_go_back: bool,
    can_go_forward: bool,
    latest_load_complete: bool,
    slow_frames: usize,
    max_frame_ms: Option<f64>,
    max_frame_label: Option<String>,
    max_paint_ms: Option<f64>,
    max_paint_label: Option<String>,
    latest_resource_audit: Option<String>,
    certificate_back_requested: bool,
    certificate_trust_once_requested: bool,
    certificate_trust_this_appliance_requested: bool,
    tabs: Vec<HostedDirectTab>,
}

/// One tab reported by the hosted-direct child for the chrome tab strip.
#[derive(Clone, Debug, Default, PartialEq)]
struct HostedDirectTab {
    url: String,
    active: bool,
}

/// The site label shown on a tab: the host without a leading `www.`, or "New Tab".
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn hosted_direct_tab_site(url: &str) -> String {
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
struct HostedDirectLogMonitor {
    last_len: u64,
    last_modified: Option<SystemTime>,
}

impl HostedDirectLogMonitor {
    fn refresh(&mut self, path: &Path) -> Option<HostedDirectLogSummary> {
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
    fn compact_perf_status(&self) -> String {
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

    fn compact_audit_status(&self) -> Option<&str> {
        self.latest_resource_audit.as_deref()
    }

    fn certificate_warning_active(&self) -> bool {
        self.certificate_fingerprint_sha256.is_some()
            || self
                .active_title
                .as_deref()
                .is_some_and(|title| title.eq_ignore_ascii_case("Certificate error"))
    }

    fn compact_certificate_status(&self) -> Option<String> {
        if let Some(fingerprint) = self.certificate_fingerprint_sha256.as_deref() {
            return Some(format!("cert {}", compact_fingerprint(fingerprint)));
        }
        self.certificate_warning_active()
            .then(|| "cert blocked".to_string())
    }

    fn load_progress(&self) -> f32 {
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

    fn is_loading(&self) -> bool {
        !self.latest_load_complete
    }

    fn can_switch_tabs(&self) -> bool {
        self.tab_count.unwrap_or(1) > 1
    }
}

#[derive(Clone, Copy)]
struct HostedDirectChromeRects {
    back: Rect,
    forward: Rect,
    reload: Rect,
    new_tab: Rect,
    previous_tab: Rect,
    next_tab: Rect,
    close_tab: Rect,
    certificate_back: Rect,
    trust_once: Rect,
    trust_appliance: Rect,
    address: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostedDirectCertificateTrust {
    Once,
    Remember,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostedDirectSmokeAction {
    Back,
    TrustOnce,
    TrustThisAppliance,
}

impl HostedDirectSmokeAction {
    fn parse_arg(value: Option<String>) -> Result<Option<Self>, String> {
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

    fn label(self) -> &'static str {
        match self {
            Self::Back => "back",
            Self::TrustOnce => "once",
            Self::TrustThisAppliance => "trust",
        }
    }

    fn rect(self, rects: HostedDirectChromeRects) -> Rect {
        match self {
            Self::Back => rects.certificate_back,
            Self::TrustOnce => rects.trust_once,
            Self::TrustThisAppliance => rects.trust_appliance,
        }
    }
}

struct HostedDirectShellState {
    target: String,
    address_input: String,
    address_cursor: usize,
    address_focused: bool,
    address_replace_on_text: bool,
    pending_certificate_trust: Option<HostedDirectCertificateTrust>,
    cursor_position: Option<(f64, f64)>,
    modifiers: ModifiersState,
}

impl HostedDirectShellState {
    fn new(target: String) -> Self {
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

    fn handle_mouse_down(
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

    fn take_pending_certificate_trust(&mut self) -> Option<HostedDirectCertificateTrust> {
        self.pending_certificate_trust.take()
    }

    fn handle_keyboard(&mut self, event: KeyEvent, child: Option<&mut HostedDirectChild>) -> bool {
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

    fn push_address_text(&mut self, value: &str) {
        if self.address_replace_on_text {
            self.address_input.clear();
            self.address_cursor = 0;
            self.address_replace_on_text = false;
        }
        self.address_input.insert_str(self.address_cursor, value);
        self.address_cursor += value.len();
    }

    fn handle_text_commit(&mut self, text: String) -> bool {
        if !self.address_focused || text.is_empty() {
            return false;
        }
        if text.chars().all(|ch| !ch.is_control()) {
            self.push_address_text(&text);
            return true;
        }
        false
    }

    fn address_display_text(&self, max_chars: usize) -> String {
        if !self.address_focused {
            return truncate(&self.target, max_chars);
        }
        let cursor = self.address_cursor.min(self.address_input.len());
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

fn previous_char_boundary(value: &str, index: usize) -> usize {
    let index = index.min(value.len());
    value[..index]
        .char_indices()
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn next_char_boundary(value: &str, index: usize) -> usize {
    let index = index.min(value.len());
    if index >= value.len() {
        return value.len();
    }
    value[index..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| index + offset)
        .unwrap_or(value.len())
}

struct HostedDirectChild {
    child: Child,
    stdin: Option<ChildStdin>,
    log_path: PathBuf,
    #[cfg(all(target_os = "windows", feature = "servo-backend"))]
    window_hwnd: Option<isize>,
    #[cfg(all(target_os = "windows", feature = "servo-backend"))]
    last_resized_parent_size: Option<PhysicalSize<u32>>,
}

fn spawn_hosted_direct_child(
    parent_hwnd: isize,
    parent_size: PhysicalSize<u32>,
    target: &str,
    browser_mode: BrowserMode,
    certificate_path: Option<&Path>,
    local_appliance_cert_fingerprint: Option<&str>,
    remember_local_appliance_cert: Option<&str>,
    allow_insecure_local_tls: bool,
    direct_resource_audit: bool,
) -> Result<HostedDirectChild, String> {
    let (_, _, width, height) = hosted_direct_child_bounds(parent_size);
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
        .arg(HOSTED_DIRECT_CHROME_H.to_string())
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

fn send_hosted_direct_command(child: Option<&mut HostedDirectChild>, command: &str) -> bool {
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

fn poll_hosted_direct_child_exit(
    child: &mut Option<HostedDirectChild>,
) -> Option<(ExitStatus, PathBuf)> {
    let status = child.as_mut()?.child.try_wait().ok()??;
    let child = child.take()?;
    Some((status, child.log_path))
}

fn hosted_direct_chrome_rects(size: PhysicalSize<u32>) -> HostedDirectChromeRects {
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

fn hosted_direct_child_bounds(parent_size: PhysicalSize<u32>) -> (i32, i32, u32, u32) {
    (
        0,
        HOSTED_DIRECT_CHROME_H as i32,
        parent_size.width.max(320),
        parent_size
            .height
            .saturating_sub(HOSTED_DIRECT_CHROME_H)
            .max(240),
    )
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn resize_hosted_direct_child(
    parent_hwnd: isize,
    child: &mut HostedDirectChild,
    parent_size: PhysicalSize<u32>,
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
    let (x, y, width, height) = hosted_direct_child_bounds(parent_size);
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
fn resize_hosted_direct_child(
    _parent_hwnd: isize,
    _child: &mut HostedDirectChild,
    _parent_size: PhysicalSize<u32>,
) -> bool {
    false
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
struct HostedDirectChildWindowSearch {
    pid: u32,
    hwnd: Option<isize>,
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn find_hosted_direct_child_window(parent_hwnd: isize, child_pid: u32) -> Option<isize> {
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

fn hosted_direct_child_log_path() -> Result<PathBuf, String> {
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

fn terminate_child_process(child: &mut Option<HostedDirectChild>) {
    if let Some(mut child) = child.take() {
        let _ = child.child.kill();
        let _ = child.child.wait();
    }
}

#[cfg(all(target_os = "windows", feature = "servo-backend"))]
fn focus_hosted_direct_parent_window(window: &Window, parent_hwnd: isize) {
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
fn focus_hosted_direct_parent_window(window: &Window, _parent_hwnd: isize) {
    window.focus_window();
}

fn draw_hosted_direct_shell(
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

fn draw_hosted_direct_button(
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

fn draw_hosted_direct_progress(
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

fn parse_hosted_direct_log_summary(path: &Path) -> HostedDirectLogSummary {
    let Ok(contents) = read_hosted_direct_log_window(path) else {
        return HostedDirectLogSummary::default();
    };
    parse_hosted_direct_log_summary_text(&contents)
}

fn read_hosted_direct_log_window(path: &Path) -> Result<String, std::io::Error> {
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

fn parse_hosted_direct_log_summary_text(contents: &str) -> HostedDirectLogSummary {
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

fn summarize_resource_audit(raw_audit: &str) -> Option<String> {
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

fn format_optional_count(value: Option<u64>) -> String {
    value
        .map(|count| count.to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn json_u64_field(raw: &str, field: &str) -> Option<u64> {
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

fn json_array_len_field(raw: &str, field: &str) -> Option<u64> {
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

fn extract_between<'a>(value: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let (_, rest) = value.split_once(start)?;
    let (between, _) = rest.split_once(end)?;
    Some(between.trim())
}

fn extract_after<'a>(value: &'a str, marker: &str) -> Option<&'a str> {
    let (_, rest) = value.split_once(marker)?;
    Some(rest.trim())
}

fn extract_token_after<'a>(value: &'a str, marker: &str) -> Option<&'a str> {
    let (_, rest) = value.split_once(marker)?;
    rest.split_whitespace().next()
}

fn compact_fingerprint(value: &str) -> String {
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
fn appliance_cert_entry_label(entry: &ApplianceCertTrustEntry) -> String {
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

fn update_max_duration(max_ms: &mut Option<f64>, max_label: &mut Option<String>, label: &str) {
    let Some(duration_ms) = parse_duration_label_ms(label) else {
        return;
    };
    if max_ms.is_none_or(|current| duration_ms > current) {
        *max_ms = Some(duration_ms);
        *max_label = Some(label.to_string());
    }
}

fn parse_duration_label_ms(label: &str) -> Option<f64> {
    if let Some(value) = label.strip_suffix("ms") {
        return value.parse::<f64>().ok();
    }
    if let Some(value) = label.strip_suffix('s') {
        return value.parse::<f64>().ok().map(|seconds| seconds * 1000.0);
    }
    None
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

fn sextant_window_icon() -> Option<Icon> {
    let size = 64u32;
    let rgba = include_bytes!("../assets/icons/sextant-64.rgba");
    Icon::from_rgba(rgba.to_vec(), size, size).ok()
}

fn start_visible_input(app: &mut BrowserApp, input: &str) -> Result<bool, String> {
    app.address_input = input.to_string();
    if native_intent_body(input).is_some() {
        app.run_native_intent_sync(input);
        if !app.last_ok {
            return Err(app.last_status.clone());
        }
        return Ok(false);
    } else {
        let url = parse_navigation_target(input)?;
        app.start_visible_navigation(url);
    }
    if !app.last_ok {
        return Err(app.last_status.clone());
    }
    Ok(app.pending_navigation.is_some())
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
    app.run_native_intent_sync(intent);
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
    app.run_native_intent_sync(intent);
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
    push_eval_probe_report(&mut report, &mut app, "before distillation");

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
    push_eval_probe_report(&mut report, &mut app, "after distillation");

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
    if let Some(eval_ms) = page.metadata.get("live_dom_eval_ms") {
        let queue_ms = page
            .metadata
            .get("live_dom_queue_ms")
            .map(String::as_str)
            .unwrap_or("unknown");
        let parse_ms = page
            .metadata
            .get("live_dom_parse_ms")
            .map(String::as_str)
            .unwrap_or("unknown");
        let script_ms = page
            .metadata
            .get("live_dom_script_ms")
            .map(String::as_str)
            .unwrap_or("unknown");
        report.push(format!(
            "perception live-dom timing: queue={}ms eval={}ms parse={}ms script={}ms",
            queue_ms, eval_ms, parse_ms, script_ms
        ));
    }
    if let Some(load_before) = page.metadata.get("live_dom_load_status_before") {
        let load_after = page
            .metadata
            .get("live_dom_load_status_after")
            .map(String::as_str)
            .unwrap_or("unknown");
        report.push(format!(
            "perception live-dom status: load_before={} load_after={}",
            load_before, load_after
        ));
    }
}

fn push_eval_probe_report(report: &mut Vec<String>, app: &mut BrowserApp, label: &str) {
    let Some(tab_id) = app.active_tab().map(|tab| tab.id) else {
        return;
    };
    match app.engine.eval_probe_tab(tab_id) {
        Ok(probe) => report.push(format!(
            "eval probe {label}: queue={}ms eval={}ms script={}ms load_before={} load_after={} value={}",
            probe.queue_ms,
            probe.eval_ms,
            probe.script_ms,
            probe.load_status_before,
            probe.load_status_after,
            truncate(&probe.value, 48)
        )),
        Err(error) => report.push(format!(
            "eval probe {label} failed: {}",
            truncate(&error, 120)
        )),
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
    let user_distill = args
        .iter()
        .any(|arg| arg == "--window-smoke-distill" || arg == "--user-distill");
    let input_latency = args
        .iter()
        .any(|arg| arg == "--window-input-smoke" || arg == "--input-latency-smoke");
    let first_interaction_smoke = args
        .iter()
        .any(|arg| arg == "--first-interaction-smoke" || arg == "--window-first-interaction-smoke");
    let verified_input_smoke = args
        .iter()
        .any(|arg| arg == "--verified-input-smoke" || arg == "--window-verified-input-smoke");
    let search_submit_smoke = args
        .iter()
        .any(|arg| arg == "--search-submit-smoke" || arg == "--window-search-submit-smoke");
    let live_search_smoke = args
        .iter()
        .any(|arg| arg == "--live-search-smoke" || arg == "--window-live-search-smoke");
    let live_form_smoke = args
        .iter()
        .any(|arg| arg == "--live-form-smoke" || arg == "--window-live-form-smoke");
    let live_link_smoke = args
        .iter()
        .any(|arg| arg == "--live-link-smoke" || arg == "--window-live-link-smoke");
    let load_smoke = args
        .iter()
        .any(|arg| arg == "--load-smoke" || arg == "--window-load-smoke");
    let resize_smoke = args
        .iter()
        .any(|arg| arg == "--resize-smoke" || arg == "--window-resize-smoke");
    let allow_insecure_local_tls = args.iter().any(|arg| arg == "--allow-insecure-local-tls");
    Ok(Some(WindowSmokeSpec {
        target,
        timeout,
        user_distill,
        input_latency,
        first_interaction_smoke,
        verified_input_smoke,
        search_submit_smoke,
        live_search_smoke,
        live_form_smoke,
        live_link_smoke,
        load_smoke,
        resize_smoke,
        allow_insecure_local_tls,
    }))
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend", test))]
fn is_local_appliance_target(target: &Url) -> bool {
    let Some(host) = target.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return true;
    }
    let Ok(addr) = host.parse::<IpAddr>() else {
        return false;
    };
    match addr {
        IpAddr::V4(addr) => addr.is_loopback() || addr.is_private() || addr.is_link_local(),
        IpAddr::V6(addr) => {
            let first = addr.segments()[0];
            addr.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

fn input_latency_fixture_url() -> String {
    let html = "<!doctype html><meta charset='utf-8'><title>Sextant Input Smoke</title>\
<body style='margin:0;background:#10161d;color:white;font:20px sans-serif;display:grid;place-items:center;height:100vh'>\
<input id='q' autofocus style='font:24px sans-serif;width:70vw;padding:18px' value=''></body>";
    let encoded: String = url::form_urlencoded::byte_serialize(html.as_bytes()).collect();
    format!("data:text/html,{}", encoded)
}

fn parse_browser_mode_arg(args: &[String]) -> Result<BrowserMode, String> {
    let Some(value) = operator_arg_value(args, "--browser-mode") else {
        return Ok(BrowserMode::Assisted);
    };
    parse_browser_mode(&value)
}

fn parse_user_render_path_arg(args: &[String]) -> Result<UserRenderPath, String> {
    let Some(value) = operator_arg_value(args, "--render-path")
        .or_else(|| operator_arg_value(args, "--user-render-path"))
    else {
        return Ok(UserRenderPath::FrameBridge);
    };
    parse_user_render_path(&value)
}

fn parse_browser_mode(value: &str) -> Result<BrowserMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "agent" => Ok(BrowserMode::Agent),
        "assist" | "assisted" => Ok(BrowserMode::Assisted),
        "observe" => Ok(BrowserMode::Observe),
        "direct" => Ok(BrowserMode::Direct),
        "incog" | "incognito" => Ok(BrowserMode::Incognito),
        _ => Err(format!(
            "unknown browser mode '{}'; expected agent, assisted, observe, direct, or incognito",
            value
        )),
    }
}

fn parse_user_render_path(value: &str) -> Result<UserRenderPath, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "bridge" | "frame-bridge" | "frame_bridge" | "render-bridge" | "render_bridge" => {
            Ok(UserRenderPath::FrameBridge)
        }
        "direct" | "direct-servo" | "direct_servo" | "servo-direct" | "servo_direct" => {
            Ok(UserRenderPath::DirectServo)
        }
        _ => Err(format!(
            "unknown user render path '{}'; expected bridge or direct",
            value
        )),
    }
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

fn parse_isize_flag(args: &[String], flag: &str) -> Option<isize> {
    operator_arg_value(args, flag).and_then(|value| value.parse::<isize>().ok())
}

fn parse_i32_flag(args: &[String], flag: &str) -> Option<i32> {
    operator_arg_value(args, flag).and_then(|value| value.parse::<i32>().ok())
}

fn parse_u32_flag(args: &[String], flag: &str) -> Option<u32> {
    operator_arg_value(args, flag).and_then(|value| value.parse::<u32>().ok())
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
    draw_page_load_bar(&mut buffer, width, height, app);
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
        #[cfg(feature = "xilem-shell")]
        MainView::Settings => {
            draw_settings_panel(&mut buffer, width, height, app);
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
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: 0,
            w: rail_x,
            h: 2,
        },
        BUTTON_BRIGHT,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 0,
            w: RAIL_WIDTH,
            h: 2,
        },
        STATUS_OK,
    );
    draw_text(buffer, width, height, 24, 12, "SEXTANT", TEXT, 1);
    draw_text(buffer, width, height, 24, 32, "AI BROWSER", TEXT_DIM, 1);
    for region in &app.mode_rects {
        draw_pill(
            buffer,
            width,
            height,
            region.rect,
            region.mode.label(),
            region.mode == app.browser_mode,
        );
    }
    let tabs = app.engine.get_tabs();
    let tab_label = format!("TABS {}", tabs.len());
    draw_text(
        buffer,
        width,
        height,
        rail_x + 198,
        24,
        &tab_label,
        TEXT_DIM,
        1,
    );
    draw_status_dot(buffer, width, height, rail_x + 18, 23, STATUS_OK);
    draw_text(buffer, width, height, rail_x + 34, 15, "MAYA", TEXT, 1);
    draw_text(
        buffer,
        width,
        height,
        rail_x + 34,
        33,
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
        PANEL_SOFT,
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
    draw_field_with_placeholder(
        buffer,
        width,
        height,
        app.address_rect,
        &app.address_input,
        "ENTER URL OR ASK SEXTANT",
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
    #[cfg(feature = "xilem-shell")]
    draw_pill(
        buffer,
        width,
        height,
        app.settings_tab_rect,
        "SETTINGS",
        app.main_view == MainView::Settings,
    );
    let ai_ready = app.capabilities().ai_observe_dom;
    draw_status_chip(
        buffer,
        width,
        height,
        Rect {
            x: rail_x.saturating_sub(102),
            y: CHROME_H + STRIP_H + 8,
            w: 84,
            h: 24,
        },
        if ai_ready { "AI READY" } else { "AI OFF" },
        if ai_ready { STATUS_OK } else { TEXT_DIM },
        ai_ready,
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

fn draw_page_load_bar(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let track = page_load_bar_rect(width);
    if track.w == 0 || track.h == 0 {
        return;
    }

    fill_rect(buffer, width, height, track, FIELD);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: track.x,
            y: track.y,
            w: track.w,
            h: 1,
        },
        BORDER,
    );

    if let Some(active) = page_load_bar_active_rect(app, track) {
        fill_rect(buffer, width, height, active, BUTTON_BRIGHT);
        let pulse = Rect {
            x: active.x,
            y: active.y,
            w: active.w,
            h: 1,
        };
        fill_rect(buffer, width, height, pulse, STATUS_OK);
    }
}

fn page_load_bar_rect(width: u32) -> Rect {
    let rail_x = right_rail_x(width);
    Rect {
        x: 24,
        y: CHROME_H + STRIP_H + TAB_H + PAGE_TAB_H + 4,
        w: rail_x.saturating_sub(48),
        h: LOAD_BAR_H,
    }
}

fn page_load_bar_active_rect(app: &BrowserApp, track: Rect) -> Option<Rect> {
    let pending = app.pending_navigation.as_ref()?;
    if track.w == 0 {
        return None;
    }

    let segment_w = (track.w / 3).max(64).min(track.w);
    let travel = track.w.saturating_sub(segment_w);
    let elapsed_ms = pending.started.elapsed().as_millis() as u32;
    let offset = if travel == 0 {
        0
    } else {
        let cycle = travel.saturating_mul(2).max(1);
        let phase = (elapsed_ms / 8) % cycle;
        if phase <= travel {
            phase
        } else {
            cycle.saturating_sub(phase)
        }
    };

    Some(Rect {
        x: track.x.saturating_add(offset),
        y: track.y,
        w: segment_w,
        h: track.h,
    })
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
    draw_panel_surface(buffer, width, height, panel, BUTTON_BRIGHT);
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
            "TAB {} | BACK {} | FORWARD {} | ENGINE {} | RENDER {}",
            short_id(tab.id),
            tab.can_go_back,
            tab.can_go_forward,
            display_backend(app, tab),
            app.render_path_label()
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
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: 32,
                y: panel.y + 70,
                w: panel.w.saturating_sub(64),
                h: 106,
            },
            PANEL_DARK,
        );
        stroke_rect(
            buffer,
            width,
            height,
            Rect {
                x: 32,
                y: panel.y + 70,
                w: panel.w.saturating_sub(64),
                h: 106,
            },
            BORDER_SOFT,
        );
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: 32,
                y: panel.y + 70,
                w: 2,
                h: 106,
            },
            BUTTON_BRIGHT,
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 86,
            "READY FOR A PAGE",
            TEXT,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 112,
            "ENTER A URL OR ASK SEXTANT FROM THE ADDRESS BAR.",
            TEXT_SOFT,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 138,
            "THE AI SIDECAR WILL TRACK CONTEXT WITHOUT TAKING CONTROL.",
            TEXT_DIM,
            1,
        );
    }
    None
}

fn draw_wake_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, STATUS_OK);
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
    draw_panel_surface(buffer, width, height, panel, BUTTON_BRIGHT);
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
    draw_panel_surface(buffer, width, height, panel, STATUS_WARN);
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
    let cert_section_y = panel.y + GUARD_CERT_SECTION_Y;
    let max_rows = cert_section_y.saturating_sub(panel.y + 58) / 26;
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
    draw_appliance_cert_settings(buffer, width, height, panel, app);
}

#[cfg(feature = "xilem-shell")]
fn settings_backend_button_rects(panel: Rect) -> Vec<(&'static str, Rect)> {
    let button_w = 150;
    let gap = 14;
    let y = panel.y + 78;
    AiLocalConfig::BACKENDS
        .iter()
        .enumerate()
        .map(|(index, backend)| {
            let x = panel.x + 24 + index as u32 * (button_w + gap);
            (
                *backend,
                Rect {
                    x,
                    y,
                    w: button_w,
                    h: 34,
                },
            )
        })
        .collect()
}

#[cfg(feature = "xilem-shell")]
fn draw_settings_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, STATUS_OK);
    draw_text(
        buffer,
        width,
        height,
        panel.x + 20,
        panel.y + 18,
        "LOCAL AI MODEL",
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        panel.y + 52,
        "BACKEND",
        TEXT_DIM,
        1,
    );

    let active = app.ai_config.backend.as_str();
    for (backend, rect) in settings_backend_button_rects(panel) {
        // Highlight the active backend by drawing it in the "hovered" style.
        let selected = backend == active;
        draw_button(
            buffer,
            width,
            height,
            rect,
            AiLocalConfig::backend_label(backend),
            selected,
            true,
        );
    }

    let info_y = panel.y + 138;
    let max_chars = (panel.w.saturating_sub(48) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y,
        &truncate(&format!("ENDPOINT  {}", app.ai_config.endpoint), max_chars),
        TEXT_SOFT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y + 26,
        &truncate(&format!("MODEL     {}", app.ai_config.model), max_chars),
        TEXT_SOFT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y + 52,
        &truncate(
            &format!(
                "ACTIVE    {} brain @ {}",
                AiLocalConfig::backend_label(active),
                app.ai_config.endpoint
            ),
            max_chars,
        ),
        STATUS_OK,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y + 92,
        "Click a backend to select it and rebuild the agent brain.",
        TEXT_DIM,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y + 114,
        &truncate(
            "Endpoint/model use that backend's default; edit <profile>/ai-provider.json or set SEXTANT_LOCAL_ENDPOINT / SEXTANT_LOCAL_MODEL to customize.",
            max_chars,
        ),
        TEXT_DIM,
        1,
    );
}

fn guard_appliance_refresh_rect(panel: Rect) -> Rect {
    Rect {
        x: panel.x + panel.w.saturating_sub(226),
        y: panel.y + GUARD_CERT_SECTION_Y,
        w: 96,
        h: 28,
    }
}

fn guard_appliance_forget_rect(panel: Rect) -> Rect {
    Rect {
        x: panel.x + panel.w.saturating_sub(118),
        y: panel.y + GUARD_CERT_SECTION_Y,
        w: 92,
        h: 28,
    }
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
fn guard_appliance_row_rects(panel: Rect, count: usize) -> Vec<(usize, Rect)> {
    let row_start = panel.y + GUARD_CERT_SECTION_Y + 76;
    let max_rows = panel
        .y
        .saturating_add(panel.h)
        .saturating_sub(row_start + 8)
        / GUARD_CERT_ROW_H;
    (0..count.min(max_rows as usize))
        .map(|index| {
            (
                index,
                Rect {
                    x: panel.x + 20,
                    y: row_start + index as u32 * GUARD_CERT_ROW_H,
                    w: panel.w.saturating_sub(40),
                    h: GUARD_CERT_ROW_H.saturating_sub(8),
                },
            )
        })
        .collect()
}

fn draw_appliance_cert_settings(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    panel: Rect,
    app: &BrowserApp,
) {
    let section_y = panel.y + GUARD_CERT_SECTION_Y;
    let left = panel.x + 20;
    let max_chars = ((panel.w.saturating_sub(40)) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        left,
        section_y,
        "LOCAL APPLIANCE CERTIFICATES",
        TEXT,
        1,
    );
    let refresh_rect = guard_appliance_refresh_rect(panel);
    let forget_rect = guard_appliance_forget_rect(panel);
    let refresh_hovered = app
        .cursor
        .map(|(x, y)| refresh_rect.contains(x, y))
        .unwrap_or(false);
    let forget_hovered = app
        .cursor
        .map(|(x, y)| forget_rect.contains(x, y))
        .unwrap_or(false);
    draw_button(
        buffer,
        width,
        height,
        refresh_rect,
        "REFRESH",
        refresh_hovered,
        true,
    );

    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    let can_forget = app
        .selected_appliance_cert
        .is_some_and(|index| index < app.appliance_cert_entries.len());
    #[cfg(not(any(feature = "xilem-shell", feature = "servo-backend")))]
    let can_forget = false;
    draw_button(
        buffer,
        width,
        height,
        forget_rect,
        "FORGET",
        forget_hovered,
        can_forget,
    );

    draw_text(
        buffer,
        width,
        height,
        left,
        section_y + 34,
        &truncate(&app.appliance_cert_status, max_chars),
        TEXT_DIM,
        1,
    );

    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    {
        if app.appliance_cert_entries.is_empty() {
            draw_text(
                buffer,
                width,
                height,
                left,
                section_y + 78,
                "NO TRUSTED LOCAL APPLIANCE CERTIFICATES.",
                TEXT_DIM,
                1,
            );
            return;
        }

        let row_rects = guard_appliance_row_rects(panel, app.appliance_cert_entries.len());
        for (index, rect) in row_rects {
            let selected = app.selected_appliance_cert == Some(index);
            let hovered = app
                .cursor
                .map(|(x, y)| rect.contains(x, y))
                .unwrap_or(false);
            fill_rect(
                buffer,
                width,
                height,
                rect,
                if selected {
                    FIELD_FOCUS
                } else if hovered {
                    PANEL_ALT
                } else {
                    FIELD
                },
            );
            stroke_rect(
                buffer,
                width,
                height,
                rect,
                if selected { BUTTON_ACTIVE } else { BORDER },
            );
            let Some(entry) = app.appliance_cert_entries.get(index) else {
                continue;
            };
            let label = appliance_cert_entry_label(entry);
            let fingerprint = compact_fingerprint(&entry.fingerprint_sha256);
            let created = truncate(&entry.created_at, 24);
            draw_text(
                buffer,
                width,
                height,
                rect.x + 12,
                rect.y + 8,
                &label,
                TEXT,
                1,
            );
            draw_text(
                buffer,
                width,
                height,
                rect.x + 12,
                rect.y + 24,
                &truncate(&format!("{} | {}", fingerprint, created), max_chars),
                TEXT_DIM,
                1,
            );
        }
    }
    #[cfg(not(any(feature = "xilem-shell", feature = "servo-backend")))]
    {
        draw_text(
            buffer,
            width,
            height,
            left,
            section_y + 78,
            "LOCAL APPLIANCE CERTIFICATE STORAGE IS UNAVAILABLE IN THIS BUILD.",
            TEXT_DIM,
            1,
        );
    }
}

fn draw_perception_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, BUTTON_BRIGHT);
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
    draw_panel_surface(buffer, width, height, panel, STATUS_WARN);

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
    draw_panel_surface(buffer, width, height, panel, STATUS_OK);

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

fn draw_field_with_placeholder(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    value: &str,
    placeholder: &str,
    focused: bool,
) {
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
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x + 1,
            y: rect.y + 1,
            w: rect.w.saturating_sub(2),
            h: 2,
        },
        if focused { BUTTON_ACTIVE } else { BORDER_SOFT },
    );
    let showing_placeholder = value.is_empty() && !placeholder.is_empty() && !focused;
    let visible_source = if showing_placeholder {
        placeholder
    } else {
        value
    };
    let visible = truncate(visible_source, ((rect.w.saturating_sub(24)) / 8) as usize);
    draw_text(
        buffer,
        width,
        height,
        rect.x + 12,
        rect.y + 9,
        &visible,
        if showing_placeholder {
            TEXT_PLACEHOLDER
        } else {
            TEXT
        },
        1,
    );
    if focused {
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: rect.x,
                y: rect.y,
                w: 3,
                h: rect.h,
            },
            BUTTON_ACTIVE,
        );
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

/// Quick-intent presets (Xilem dashboard salvage: the intent-bar preset buttons).
/// Shown in the AI rail's consent band when no consent is pending and the mode
/// allows native intents; clicking runs the intent through the local model brain.
/// Always compiled (label data) so the softbuffer reader shell builds without the
/// `xilem-shell` feature; the click path that runs an intent stays feature-gated.
const AI_RAIL_PRESETS: [(&str, &str); 2] = [
    (
        "SUMMARIZE",
        "intent: open example.com and summarize the page",
    ),
    ("DISTILL HERE", "intent: distill the active page into Wake"),
];

/// Map a pilot status to a palette color (Xilem dashboard salvage): green for
/// idle/complete/authorized, amber for consent/blocked, red for failed, cyan for
/// in-progress (reasoning/navigating/distilling/perceiving/planning). Tolerates
/// an optional leading "PILOT " prefix on the status string.
fn pilot_status_color(status: &str) -> u32 {
    match status.trim_start_matches("PILOT ").trim() {
        "IDLE" | "COMPLETE" | "CONSENT AUTHORIZED" => STATUS_OK,
        "FAILED" => STATUS_ERROR,
        "BLOCKED" | "AWAITING CONSENT" | "CONSENT DENIED" | "CONSENT RESUMING" => STATUS_WARN,
        _ => STATUS_INFO,
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
            w: RAIL_WIDTH,
            h: 74,
        },
        PANEL_HEADER,
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
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 64,
            w: 2,
            h: height.saturating_sub(64 + STATUS_BAR_H),
        },
        BUTTON_BRIGHT,
    );
    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        86,
        "MAYA SIDECAR",
        TEXT,
        1,
    );
    draw_status_chip(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 20,
            y: 108,
            w: 126,
            h: 22,
        },
        &format!(
            "PILOT {}",
            truncate(app.pilot_status.trim_start_matches("PILOT ").trim(), 11)
        ),
        pilot_status_color(&app.pilot_status),
        app.pilot_status != "FAILED",
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

    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        146,
        "ACTIVE CONTEXT",
        TEXT_DIM,
        1,
    );
    draw_chat_bubble(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 20,
            y: 164,
            w: RAIL_WIDTH.saturating_sub(40),
            h: 88,
        },
        &page_state,
        false,
    );
    draw_text(
        buffer,
        width,
        height,
        rail_x + 56,
        266,
        "NEXT STEP",
        TEXT_DIM,
        1,
    );
    draw_chat_bubble(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 56,
            y: 284,
            w: RAIL_WIDTH.saturating_sub(76),
            h: 66,
        },
        &next_step,
        true,
    );
    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        364,
        "SESSION STATUS",
        TEXT_DIM,
        1,
    );
    draw_chat_bubble(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 20,
            y: 382,
            w: RAIL_WIDTH.saturating_sub(40),
            h: 54,
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
    } else if app.native_intent_allowed() {
        // Quick-intent presets reuse the (free) consent button band.
        draw_text(
            buffer,
            width,
            height,
            rail_x + 20,
            444,
            "QUICK INTENTS",
            TEXT_DIM,
            1,
        );
        for (index, (label, _intent)) in AI_RAIL_PRESETS.iter().enumerate() {
            let rect = if index == 0 {
                app.consent_authorize_rect
            } else {
                app.consent_deny_rect
            };
            let hovered = app
                .cursor
                .map(|(x, y)| rect.contains(x, y))
                .unwrap_or(false);
            draw_button(buffer, width, height, rect, label, hovered, true);
        }
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
    draw_field_with_placeholder(
        buffer,
        width,
        height,
        app.wake_rect,
        &app.wake_query,
        "ASK OR SEARCH WAKE",
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
        "MODE {}    AI {}    STORAGE {}    ENGINE {}    RENDER {}    {}    WAKE RESULTS {}    LOG ENTRIES {}    VALIDATION {}/{}",
        app.browser_mode.label(),
        app.ai_status_label(),
        app.storage_status_label(),
        backend,
        app.render_path_label(),
        app.perf.summary(),
        app.wake_results.len(),
        app.recent_logs.len(),
        app.validation_pass_count(),
        validation_total
    );
    let max_status_chars = (width.saturating_sub(172) / char_advance(1)).max(1) as usize;
    draw_status_dot(
        buffer,
        width,
        height,
        14,
        y + 12,
        if app.last_ok { STATUS_OK } else { STATUS_WARN },
    );
    draw_text(
        buffer,
        width,
        height,
        30,
        y + 14,
        &truncate(&status, max_status_chars),
        TEXT_SOFT,
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

fn draw_panel_surface(buffer: &mut [u32], width: u32, height: u32, rect: Rect, accent: u32) {
    fill_rect(buffer, width, height, rect, PANEL_ALT);
    stroke_rect(buffer, width, height, rect, BORDER);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x + 1,
            y: rect.y + 1,
            w: rect.w.saturating_sub(2),
            h: 38.min(rect.h.saturating_sub(2)),
        },
        PANEL_HEADER,
    );
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
}

fn draw_status_dot(buffer: &mut [u32], width: u32, height: u32, x: u32, y: u32, color: u32) {
    fill_rect(buffer, width, height, Rect { x, y, w: 7, h: 7 }, color);
    stroke_rect(
        buffer,
        width,
        height,
        Rect {
            x: x.saturating_sub(2),
            y: y.saturating_sub(2),
            w: 11,
            h: 11,
        },
        BORDER_SOFT,
    );
}

fn draw_status_chip(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    accent: u32,
    active: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if active { FIELD_FOCUS } else { PANEL },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if active { accent } else { BORDER },
    );
    draw_status_dot(buffer, width, height, rect.x + 8, rect.y + 8, accent);
    let max_chars = ((rect.w.saturating_sub(28)) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        rect.x + 24,
        rect.y + 8,
        &truncate(label, max_chars),
        if active { TEXT } else { TEXT_DIM },
        1,
    );
}

fn draw_text_centered(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    color: u32,
    scale: u32,
) {
    let scale = scale.max(1);
    let max_chars = ((rect.w.saturating_sub(8)) / char_advance(scale)) as usize;
    let visible = truncate(label, max_chars);
    let text_w = text_width(&visible, scale);
    let glyph_h = 7 * scale;
    let x = rect.x + rect.w.saturating_sub(text_w) / 2;
    let y = rect.y + rect.h.saturating_sub(glyph_h) / 2;
    draw_text(buffer, width, height, x, y, &visible, color, scale);
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
    fill_rect(buffer, width, height, rect, PANEL_HEADER);
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
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: 2,
            h: rect.h,
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
        if active { FIELD_FOCUS } else { PANEL_ALT },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if active { BUTTON_BRIGHT } else { BORDER },
    );
    if active {
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
            BUTTON_BRIGHT,
        );
    }
    draw_text_centered(
        buffer,
        width,
        height,
        rect,
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
        if user { FIELD_FOCUS } else { PANEL_ALT },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if user { BUTTON_BRIGHT } else { BORDER_SOFT },
    );
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
        if user { BUTTON_BRIGHT } else { BORDER_SOFT },
    );
    let color = if user { TEXT } else { TEXT_SOFT };
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
            PANEL_ALT
        },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if enabled { BORDER } else { PANEL },
    );
    if enabled {
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
            if hovered { BUTTON_BRIGHT } else { BORDER_SOFT },
        );
    }
    draw_text_centered(
        buffer,
        width,
        height,
        rect,
        label,
        if enabled { TEXT } else { TEXT_DIM },
        1,
    );
}

fn right_rail_x(width: u32) -> u32 {
    let min_main_width = 420;
    if width >= min_main_width + RAIL_WIDTH {
        width.saturating_sub(RAIL_WIDTH)
    } else {
        width
    }
}

fn main_panel_rect(rail_x: u32, height: u32) -> Rect {
    let top = METRIC_Y + METRIC_H + 14;
    let bottom_limit = height.saturating_sub(STATUS_BAR_H + 12);
    Rect {
        x: 24,
        y: top,
        w: rail_x.saturating_sub(48),
        h: bottom_limit.saturating_sub(top),
    }
}

fn browser_viewport_rect(panel: Rect) -> Rect {
    let header_h = panel.h.min(126);
    let bottom_pad = if panel.h > header_h { 16 } else { 0 };
    Rect {
        x: panel.x + 18,
        y: panel.y + header_h,
        w: panel.w.saturating_sub(36),
        h: panel.h.saturating_sub(header_h + bottom_pad),
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
    if max_chars == 0 {
        return String::new();
    }
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
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
        '<' => [0x01, 0x02, 0x04, 0x08, 0x04, 0x02, 0x01],
        '>' => [0x10, 0x08, 0x04, 0x02, 0x04, 0x08, 0x10],
        '+' => [0x00, 0x04, 0x04, 0x1f, 0x04, 0x04, 0x00],
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
