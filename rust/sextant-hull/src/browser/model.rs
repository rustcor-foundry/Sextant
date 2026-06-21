//! Shell data model — the geometry, enum, perf, pending-async, persistence
//! result, AI-config, and pilot-record types (plus their small impls) that the
//! BrowserApp and its behavior modules pass around. Split out of browser.rs;
//! fields/methods are pub and the crate root re-exports the module
//! (`use model::*`) so every sibling resolves these via its own `use super::*`.

use super::*;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && y >= self.y as f64
            && x < (self.x + self.w) as f64
            && y < (self.y + self.h) as f64
    }

    pub fn intersects(self, other: Rect) -> bool {
        self.x < other.x.saturating_add(other.w)
            && self.x.saturating_add(self.w) > other.x
            && self.y < other.y.saturating_add(other.h)
            && self.y.saturating_add(self.h) > other.y
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
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
pub enum FocusTarget {
    Address,
    Wake,
    Browser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserShortcut {
    FocusAddress,
    NewTab,
    CloseTab,
    Reload,
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserMode {
    Agent,
    Assisted,
    Observe,
    Direct,
    Incognito,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserRenderPath {
    FrameBridge,
    DirectServo,
}

impl UserRenderPath {
    pub fn label(self) -> &'static str {
        match self {
            UserRenderPath::FrameBridge => "BRIDGE",
            UserRenderPath::DirectServo => "DIRECT",
        }
    }

    pub fn status(self) -> &'static str {
        match self {
            UserRenderPath::FrameBridge => {
                "temporary frame-capture render bridge feeding Softbuffer"
            }
            UserRenderPath::DirectServo => {
                "direct Servo WindowRenderingContext raw window; shell chrome compositor pending"
            }
        }
    }

    pub fn integrated(self) -> bool {
        matches!(self, UserRenderPath::FrameBridge)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrowserCapabilities {
    pub ai_control: bool,
    pub ai_observe_dom: bool,
    pub ai_observe_frame: bool,
    pub read_wake: bool,
    pub write_wake: bool,
    pub write_log_content: bool,
    pub expose_mcp_tools: bool,
    pub direct_render_required: bool,
}

impl BrowserMode {
    pub const ALL: [BrowserMode; 5] = [
        BrowserMode::Agent,
        BrowserMode::Assisted,
        BrowserMode::Observe,
        BrowserMode::Direct,
        BrowserMode::Incognito,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BrowserMode::Agent => "AGENT",
            BrowserMode::Assisted => "ASSIST",
            BrowserMode::Observe => "OBSERVE",
            BrowserMode::Direct => "DIRECT",
            BrowserMode::Incognito => "INCOG",
        }
    }

    pub fn status(self) -> &'static str {
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

    pub fn capabilities(self) -> BrowserCapabilities {
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

    pub fn cli_arg(self) -> &'static str {
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
pub enum MainView {
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
pub struct ValidationState {
    pub navigation_seen: bool,
    pub browser_focus_seen: bool,
    pub browser_input_seen: bool,
    pub frame_seen: bool,
    pub distill_seen: bool,
    pub wake_seen: bool,
    pub log_seen: bool,
    pub tab_control_seen: bool,
    pub error_seen: bool,
}

#[derive(Clone, Copy)]
pub enum ValidationStatus {
    Pass,
    Waiting,
    Attention,
}

pub struct ValidationRow {
    pub label: &'static str,
    pub status: ValidationStatus,
    pub detail: String,
}

pub enum OperatorStep {
    Fill { selector: String, value: String },
    Click { selector: String },
    Submit { selector: String },
    Expect { text: String },
}

pub struct OperatorRunSpec {
    pub target: String,
    pub steps: Vec<OperatorStep>,
}

pub struct WindowSmokeSpec {
    pub target: Option<String>,
    pub timeout: Duration,
    pub user_distill: bool,
    pub input_latency: bool,
    pub first_interaction_smoke: bool,
    pub verified_input_smoke: bool,
    pub search_submit_smoke: bool,
    pub live_search_smoke: bool,
    pub live_form_smoke: bool,
    pub live_link_smoke: bool,
    pub load_smoke: bool,
    pub resize_smoke: bool,
    pub allow_insecure_local_tls: bool,
}

#[derive(Clone)]
pub struct NativeIntentPlan {
    pub intent: String,
    pub target: Url,
    pub should_distill: bool,
    pub steps: Vec<String>,
}

pub struct ButtonRegion {
    pub rect: Rect,
    pub label: &'static str,
    pub action: Action,
}

pub struct PageTabRegion {
    pub rect: Rect,
    pub tab_id: Uuid,
}

pub struct ModeRegion {
    pub rect: Rect,
    pub mode: BrowserMode,
}

#[derive(Clone)]
pub struct PendingConsent {
    pub intent: String,
    pub message: String,
    #[cfg(feature = "xilem-shell")]
    pub remaining_actions: Vec<PilotAction>,
}

impl PendingConsent {
    pub fn payload(&self) -> String {
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
pub struct BrowserPerf {
    pub navigation: Option<Duration>,
    pub distill: Option<Duration>,
    pub wake: Option<Duration>,
    pub resize: Option<Duration>,
    pub frame: Option<Duration>,
}

impl BrowserPerf {
    pub fn summary(&self) -> String {
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
pub struct PerfEvent {
    pub phase: &'static str,
    pub label: String,
    pub duration: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameCapturePurpose {
    RenderBridge,
    AiObservation,
}

impl FrameCapturePurpose {
    pub fn queue_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "capture queue render bridge",
            FrameCapturePurpose::AiObservation => "capture queue ai observation",
        }
    }

    pub fn capture_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "capture render bridge",
            FrameCapturePurpose::AiObservation => "capture ai observation",
        }
    }

    pub fn capture_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge capture failed",
            FrameCapturePurpose::AiObservation => "ai observation capture failed",
        }
    }

    pub fn capture_start_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge start failed",
            FrameCapturePurpose::AiObservation => "ai observation start failed",
        }
    }

    pub fn capture_dropped_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge capture dropped",
            FrameCapturePurpose::AiObservation => "ai observation capture dropped",
        }
    }

    pub fn resize_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge",
            FrameCapturePurpose::AiObservation => "viewport ai observation",
        }
    }

    pub fn resize_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge failed",
            FrameCapturePurpose::AiObservation => "viewport ai observation failed",
        }
    }

    pub fn resize_async_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge async",
            FrameCapturePurpose::AiObservation => "viewport ai observation async",
        }
    }

    pub fn resize_async_missing_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge async missing",
            FrameCapturePurpose::AiObservation => "viewport ai observation async missing",
        }
    }
}

pub struct PendingFrameCapture {
    pub tab_id: Uuid,
    pub result_rx: mpsc::Receiver<Result<AsyncFrameCapture, String>>,
    pub started: Instant,
    pub viewport_size: (u32, u32),
    pub requested_resize: bool,
    pub purpose: FrameCapturePurpose,
}

pub struct PendingNavigation {
    pub url: Url,
    pub kind: PendingNavigationKind,
    pub result_rx: mpsc::Receiver<Result<AsyncNavigationResult, String>>,
    pub started: Instant,
    pub viewport_size: Option<(u32, u32)>,
}

pub struct PendingObservationWarmup {
    pub tab_id: Uuid,
    pub result_rx: mpsc::Receiver<Result<BrowserEvalProbe, String>>,
    pub started: Instant,
}

pub struct ScheduledObservationWarmup {
    pub tab_id: Uuid,
    pub due: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PendingNavigationKind {
    Open,
    Reload,
    Back,
    Forward,
}

impl PendingNavigationKind {
    pub fn pending_status(self, url: &Url) -> String {
        match self {
            PendingNavigationKind::Open => format!("Opening {} with Servo...", short_url(url)),
            PendingNavigationKind::Reload => "Reloading active tab with Servo...".to_string(),
            PendingNavigationKind::Back => "Going back with Servo...".to_string(),
            PendingNavigationKind::Forward => "Going forward with Servo...".to_string(),
        }
    }

    pub fn success_status(self, final_url: &Url, backend_name: &str, elapsed: Duration) -> String {
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

    pub fn failure_status(self, error: &str) -> String {
        match self {
            PendingNavigationKind::Open => format!("Open failed: {}", error),
            PendingNavigationKind::Reload => format!("Reload failed: {}", error),
            PendingNavigationKind::Back => format!("Back unavailable: {}", error),
            PendingNavigationKind::Forward => format!("Forward unavailable: {}", error),
        }
    }

    pub fn perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open async",
            PendingNavigationKind::Reload => "reload async",
            PendingNavigationKind::Back => "back async",
            PendingNavigationKind::Forward => "forward async",
        }
    }

    pub fn failure_perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open async failed",
            PendingNavigationKind::Reload => "reload async failed",
            PendingNavigationKind::Back => "back async failed",
            PendingNavigationKind::Forward => "forward async failed",
        }
    }

    pub fn dropped_perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open worker dropped",
            PendingNavigationKind::Reload => "reload worker dropped",
            PendingNavigationKind::Back => "back worker dropped",
            PendingNavigationKind::Forward => "forward worker dropped",
        }
    }

    pub fn phase_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open",
            PendingNavigationKind::Reload => "reload",
            PendingNavigationKind::Back => "back",
            PendingNavigationKind::Forward => "forward",
        }
    }

    pub fn log_intent(self, url: &Url) -> String {
        match self {
            PendingNavigationKind::Open => format!("navigate {}", url),
            PendingNavigationKind::Reload => "reload".to_string(),
            PendingNavigationKind::Back => "back".to_string(),
            PendingNavigationKind::Forward => "forward".to_string(),
        }
    }
}

pub struct PendingDistillation {
    pub result_rx: mpsc::Receiver<Result<AsyncDistillResult, String>>,
    pub started: Instant,
}

pub struct PendingPersistence {
    pub result_rx: mpsc::Receiver<Result<PersistenceDistillResult, String>>,
    pub started: Instant,
    pub page_title: String,
}

pub struct PendingWakeSearch {
    pub result_rx: mpsc::Receiver<Result<PersistenceWakeSearchResult, String>>,
    pub started: Instant,
    pub query: String,
}

pub struct PendingLogWrite {
    pub result_rx: mpsc::Receiver<Result<PersistenceLogResult, String>>,
    pub started: Instant,
    pub intent: String,
}

pub struct PendingLogRefresh {
    pub result_rx: mpsc::Receiver<Result<PersistenceLogResult, String>>,
    pub started: Instant,
}

pub enum ViewportInputEvent {
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

pub struct PersistenceDistillResult {
    pub wake_results: Vec<WakeEntry>,
    pub recent_logs: Vec<LogEntry>,
}

pub struct PersistenceWakeSearchResult {
    pub wake_results: Vec<WakeEntry>,
    pub recent_logs: Vec<LogEntry>,
}

pub struct PersistenceLogResult {
    pub recent_logs: Vec<LogEntry>,
}

/// User-configurable local AI backend selection, persisted to
/// `<profile>/ai-provider.json` and editable from the Settings tab. Resolution
/// order on load: file -> env overrides -> built-in defaults.
#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AiLocalConfig {
    pub backend: String,
    pub endpoint: String,
    pub model: String,
}

#[cfg(feature = "xilem-shell")]
impl AiLocalConfig {
    pub const BACKENDS: [&'static str; 3] = ["llamacpp", "ollama", "vllm"];

    pub fn default_for(backend: &str) -> Self {
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

    pub fn config_path(profile_dir: &Path) -> PathBuf {
        profile_dir.join("ai-provider.json")
    }

    pub fn load(profile_dir: &Path) -> Self {
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

    pub fn save(&self, profile_dir: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(Self::config_path(profile_dir), json).map_err(|error| error.to_string())
    }

    pub fn normalize(&mut self) {
        self.backend = self.backend.trim().to_lowercase();
        if !Self::BACKENDS.contains(&self.backend.as_str()) {
            self.backend = "llamacpp".to_string();
        }
        self.endpoint = self.endpoint.trim().to_string();
        self.model = self.model.trim().to_string();
    }

    pub fn backend_enum(&self) -> InferenceBackend {
        match self.backend.as_str() {
            "ollama" => InferenceBackend::Ollama,
            "vllm" => InferenceBackend::VLLM,
            _ => InferenceBackend::LlamaCpp,
        }
    }

    pub fn backend_label(backend: &str) -> &'static str {
        match backend {
            "ollama" => "Ollama",
            "vllm" => "vLLM",
            _ => "llama.cpp",
        }
    }
}

#[cfg(feature = "xilem-shell")]
pub fn env_override(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(feature = "xilem-shell")]
pub struct PendingPilotPlan {
    pub intent: String,
    pub result_rx: mpsc::Receiver<Result<Vec<PilotAction>, String>>,
    pub started: Instant,
}

/// Structured record of one pilot run: intent -> typed plan -> analysis -> status.
/// Serializable so the egui shell can render it today and a future visual
/// dashboard (or MCP) can consume the same stream without a rewrite.
#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize)]
pub struct PilotStepRecord {
    pub kind: String,
    pub detail: String,
}

#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize)]
pub struct PilotRunArtifact {
    pub intent: String,
    pub planner: String,
    pub plan: Vec<PilotStepRecord>,
    pub analysis: Vec<String>,
    pub status: String,
    pub reason_ms: u128,
}

#[cfg(feature = "xilem-shell")]
pub fn pilot_action_record(action: &PilotAction) -> PilotStepRecord {
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
pub struct DrawStats {
    pub total: Duration,
    pub frame_blit: Option<Duration>,
    pub present: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardDecision {
    pub action: &'static str,
    pub reason: String,
    pub allowed: bool,
    pub network_allowed: bool,
}

impl GuardDecision {
    pub fn summary(&self) -> String {
        let network = if self.network_allowed {
            "network allowed"
        } else {
            "network blocked"
        };
        format!("{} | {} | {}", self.action, network, self.reason)
    }
}
