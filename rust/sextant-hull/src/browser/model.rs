//! Shell data model — the geometry, enum, perf, pending-async, persistence
//! result, AI-config, and pilot-record types (plus their small impls) that the
//! BrowserApp and its behavior modules pass around. Split out of browser.rs;
//! fields/methods are pub(crate) and the crate root re-exports the module
//! (`use model::*`) so every sibling resolves these via its own `use super::*`.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Rect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) w: u32,
    pub(crate) h: u32,
}

impl Rect {
    pub(crate) fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && y >= self.y as f64
            && x < (self.x + self.w) as f64
            && y < (self.y + self.h) as f64
    }

    pub(crate) fn intersects(self, other: Rect) -> bool {
        self.x < other.x.saturating_add(other.w)
            && self.x.saturating_add(self.w) > other.x
            && self.y < other.y.saturating_add(other.h)
            && self.y.saturating_add(self.h) > other.y
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
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
pub(crate) enum FocusTarget {
    Address,
    Wake,
    Browser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserShortcut {
    FocusAddress,
    NewTab,
    CloseTab,
    Reload,
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserMode {
    Agent,
    Assisted,
    Observe,
    Direct,
    Incognito,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UserRenderPath {
    FrameBridge,
    DirectServo,
}

impl UserRenderPath {
    pub(crate) fn label(self) -> &'static str {
        match self {
            UserRenderPath::FrameBridge => "BRIDGE",
            UserRenderPath::DirectServo => "DIRECT",
        }
    }

    pub(crate) fn status(self) -> &'static str {
        match self {
            UserRenderPath::FrameBridge => {
                "temporary frame-capture render bridge feeding Softbuffer"
            }
            UserRenderPath::DirectServo => {
                "direct Servo WindowRenderingContext raw window; shell chrome compositor pending"
            }
        }
    }

    pub(crate) fn integrated(self) -> bool {
        matches!(self, UserRenderPath::FrameBridge)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BrowserCapabilities {
    pub(crate) ai_control: bool,
    pub(crate) ai_observe_dom: bool,
    pub(crate) ai_observe_frame: bool,
    pub(crate) read_wake: bool,
    pub(crate) write_wake: bool,
    pub(crate) write_log_content: bool,
    pub(crate) expose_mcp_tools: bool,
    pub(crate) direct_render_required: bool,
}

impl BrowserMode {
    pub(crate) const ALL: [BrowserMode; 5] = [
        BrowserMode::Agent,
        BrowserMode::Assisted,
        BrowserMode::Observe,
        BrowserMode::Direct,
        BrowserMode::Incognito,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            BrowserMode::Agent => "AGENT",
            BrowserMode::Assisted => "ASSIST",
            BrowserMode::Observe => "OBSERVE",
            BrowserMode::Direct => "DIRECT",
            BrowserMode::Incognito => "INCOG",
        }
    }

    pub(crate) fn status(self) -> &'static str {
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

    pub(crate) fn capabilities(self) -> BrowserCapabilities {
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

    pub(crate) fn cli_arg(self) -> &'static str {
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
pub(crate) enum MainView {
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
pub(crate) struct ValidationState {
    pub(crate) navigation_seen: bool,
    pub(crate) browser_focus_seen: bool,
    pub(crate) browser_input_seen: bool,
    pub(crate) frame_seen: bool,
    pub(crate) distill_seen: bool,
    pub(crate) wake_seen: bool,
    pub(crate) log_seen: bool,
    pub(crate) tab_control_seen: bool,
    pub(crate) error_seen: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum ValidationStatus {
    Pass,
    Waiting,
    Attention,
}

pub(crate) struct ValidationRow {
    pub(crate) label: &'static str,
    pub(crate) status: ValidationStatus,
    pub(crate) detail: String,
}

pub(crate) enum OperatorStep {
    Fill { selector: String, value: String },
    Click { selector: String },
    Submit { selector: String },
    Expect { text: String },
}

pub(crate) struct OperatorRunSpec {
    pub(crate) target: String,
    pub(crate) steps: Vec<OperatorStep>,
}

pub(crate) struct WindowSmokeSpec {
    pub(crate) target: Option<String>,
    pub(crate) timeout: Duration,
    pub(crate) user_distill: bool,
    pub(crate) input_latency: bool,
    pub(crate) first_interaction_smoke: bool,
    pub(crate) verified_input_smoke: bool,
    pub(crate) search_submit_smoke: bool,
    pub(crate) live_search_smoke: bool,
    pub(crate) live_form_smoke: bool,
    pub(crate) live_link_smoke: bool,
    pub(crate) load_smoke: bool,
    pub(crate) resize_smoke: bool,
    pub(crate) allow_insecure_local_tls: bool,
}

#[derive(Clone)]
pub(crate) struct NativeIntentPlan {
    pub(crate) intent: String,
    pub(crate) target: Url,
    pub(crate) should_distill: bool,
    pub(crate) steps: Vec<String>,
}

pub(crate) struct ButtonRegion {
    pub(crate) rect: Rect,
    pub(crate) label: &'static str,
    pub(crate) action: Action,
}

pub(crate) struct PageTabRegion {
    pub(crate) rect: Rect,
    pub(crate) tab_id: Uuid,
}

pub(crate) struct ModeRegion {
    pub(crate) rect: Rect,
    pub(crate) mode: BrowserMode,
}

#[derive(Clone)]
pub(crate) struct PendingConsent {
    pub(crate) intent: String,
    pub(crate) message: String,
    #[cfg(feature = "xilem-shell")]
    pub(crate) remaining_actions: Vec<PilotAction>,
}

impl PendingConsent {
    pub(crate) fn payload(&self) -> String {
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
pub(crate) struct BrowserPerf {
    pub(crate) navigation: Option<Duration>,
    pub(crate) distill: Option<Duration>,
    pub(crate) wake: Option<Duration>,
    pub(crate) resize: Option<Duration>,
    pub(crate) frame: Option<Duration>,
}

impl BrowserPerf {
    pub(crate) fn summary(&self) -> String {
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
pub(crate) struct PerfEvent {
    pub(crate) phase: &'static str,
    pub(crate) label: String,
    pub(crate) duration: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameCapturePurpose {
    RenderBridge,
    AiObservation,
}

impl FrameCapturePurpose {
    pub(crate) fn queue_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "capture queue render bridge",
            FrameCapturePurpose::AiObservation => "capture queue ai observation",
        }
    }

    pub(crate) fn capture_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "capture render bridge",
            FrameCapturePurpose::AiObservation => "capture ai observation",
        }
    }

    pub(crate) fn capture_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge capture failed",
            FrameCapturePurpose::AiObservation => "ai observation capture failed",
        }
    }

    pub(crate) fn capture_start_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge start failed",
            FrameCapturePurpose::AiObservation => "ai observation start failed",
        }
    }

    pub(crate) fn capture_dropped_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "render bridge capture dropped",
            FrameCapturePurpose::AiObservation => "ai observation capture dropped",
        }
    }

    pub(crate) fn resize_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge",
            FrameCapturePurpose::AiObservation => "viewport ai observation",
        }
    }

    pub(crate) fn resize_failed_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge failed",
            FrameCapturePurpose::AiObservation => "viewport ai observation failed",
        }
    }

    pub(crate) fn resize_async_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge async",
            FrameCapturePurpose::AiObservation => "viewport ai observation async",
        }
    }

    pub(crate) fn resize_async_missing_label(self) -> &'static str {
        match self {
            FrameCapturePurpose::RenderBridge => "viewport render bridge async missing",
            FrameCapturePurpose::AiObservation => "viewport ai observation async missing",
        }
    }
}

pub(crate) struct PendingFrameCapture {
    pub(crate) tab_id: Uuid,
    pub(crate) result_rx: mpsc::Receiver<Result<AsyncFrameCapture, String>>,
    pub(crate) started: Instant,
    pub(crate) viewport_size: (u32, u32),
    pub(crate) requested_resize: bool,
    pub(crate) purpose: FrameCapturePurpose,
}

pub(crate) struct PendingNavigation {
    pub(crate) url: Url,
    pub(crate) kind: PendingNavigationKind,
    pub(crate) result_rx: mpsc::Receiver<Result<AsyncNavigationResult, String>>,
    pub(crate) started: Instant,
    pub(crate) viewport_size: Option<(u32, u32)>,
}

pub(crate) struct PendingObservationWarmup {
    pub(crate) tab_id: Uuid,
    pub(crate) result_rx: mpsc::Receiver<Result<BrowserEvalProbe, String>>,
    pub(crate) started: Instant,
}

pub(crate) struct ScheduledObservationWarmup {
    pub(crate) tab_id: Uuid,
    pub(crate) due: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum PendingNavigationKind {
    Open,
    Reload,
    Back,
    Forward,
}

impl PendingNavigationKind {
    pub(crate) fn pending_status(self, url: &Url) -> String {
        match self {
            PendingNavigationKind::Open => format!("Opening {} with Servo...", short_url(url)),
            PendingNavigationKind::Reload => "Reloading active tab with Servo...".to_string(),
            PendingNavigationKind::Back => "Going back with Servo...".to_string(),
            PendingNavigationKind::Forward => "Going forward with Servo...".to_string(),
        }
    }

    pub(crate) fn success_status(
        self,
        final_url: &Url,
        backend_name: &str,
        elapsed: Duration,
    ) -> String {
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

    pub(crate) fn failure_status(self, error: &str) -> String {
        match self {
            PendingNavigationKind::Open => format!("Open failed: {}", error),
            PendingNavigationKind::Reload => format!("Reload failed: {}", error),
            PendingNavigationKind::Back => format!("Back unavailable: {}", error),
            PendingNavigationKind::Forward => format!("Forward unavailable: {}", error),
        }
    }

    pub(crate) fn perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open async",
            PendingNavigationKind::Reload => "reload async",
            PendingNavigationKind::Back => "back async",
            PendingNavigationKind::Forward => "forward async",
        }
    }

    pub(crate) fn failure_perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open async failed",
            PendingNavigationKind::Reload => "reload async failed",
            PendingNavigationKind::Back => "back async failed",
            PendingNavigationKind::Forward => "forward async failed",
        }
    }

    pub(crate) fn dropped_perf_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open worker dropped",
            PendingNavigationKind::Reload => "reload worker dropped",
            PendingNavigationKind::Back => "back worker dropped",
            PendingNavigationKind::Forward => "forward worker dropped",
        }
    }

    pub(crate) fn phase_label(self) -> &'static str {
        match self {
            PendingNavigationKind::Open => "open",
            PendingNavigationKind::Reload => "reload",
            PendingNavigationKind::Back => "back",
            PendingNavigationKind::Forward => "forward",
        }
    }

    pub(crate) fn log_intent(self, url: &Url) -> String {
        match self {
            PendingNavigationKind::Open => format!("navigate {}", url),
            PendingNavigationKind::Reload => "reload".to_string(),
            PendingNavigationKind::Back => "back".to_string(),
            PendingNavigationKind::Forward => "forward".to_string(),
        }
    }
}

pub(crate) struct PendingDistillation {
    pub(crate) result_rx: mpsc::Receiver<Result<AsyncDistillResult, String>>,
    pub(crate) started: Instant,
}

pub(crate) struct PendingPersistence {
    pub(crate) result_rx: mpsc::Receiver<Result<PersistenceDistillResult, String>>,
    pub(crate) started: Instant,
    pub(crate) page_title: String,
}

pub(crate) struct PendingWakeSearch {
    pub(crate) result_rx: mpsc::Receiver<Result<PersistenceWakeSearchResult, String>>,
    pub(crate) started: Instant,
    pub(crate) query: String,
}

pub(crate) struct PendingLogWrite {
    pub(crate) result_rx: mpsc::Receiver<Result<PersistenceLogResult, String>>,
    pub(crate) started: Instant,
    pub(crate) intent: String,
}

pub(crate) struct PendingLogRefresh {
    pub(crate) result_rx: mpsc::Receiver<Result<PersistenceLogResult, String>>,
    pub(crate) started: Instant,
}

pub(crate) enum ViewportInputEvent {
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

pub(crate) struct PersistenceDistillResult {
    pub(crate) wake_results: Vec<WakeEntry>,
    pub(crate) recent_logs: Vec<LogEntry>,
}

pub(crate) struct PersistenceWakeSearchResult {
    pub(crate) wake_results: Vec<WakeEntry>,
    pub(crate) recent_logs: Vec<LogEntry>,
}

pub(crate) struct PersistenceLogResult {
    pub(crate) recent_logs: Vec<LogEntry>,
}

/// User-configurable local AI backend selection, persisted to
/// `<profile>/ai-provider.json` and editable from the Settings tab. Resolution
/// order on load: file -> env overrides -> built-in defaults.
#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AiLocalConfig {
    pub(crate) backend: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
}

#[cfg(feature = "xilem-shell")]
impl AiLocalConfig {
    pub(crate) const BACKENDS: [&'static str; 3] = ["llamacpp", "ollama", "vllm"];

    pub(crate) fn default_for(backend: &str) -> Self {
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

    pub(crate) fn config_path(profile_dir: &Path) -> PathBuf {
        profile_dir.join("ai-provider.json")
    }

    pub(crate) fn load(profile_dir: &Path) -> Self {
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

    pub(crate) fn save(&self, profile_dir: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(Self::config_path(profile_dir), json).map_err(|error| error.to_string())
    }

    pub(crate) fn normalize(&mut self) {
        self.backend = self.backend.trim().to_lowercase();
        if !Self::BACKENDS.contains(&self.backend.as_str()) {
            self.backend = "llamacpp".to_string();
        }
        self.endpoint = self.endpoint.trim().to_string();
        self.model = self.model.trim().to_string();
    }

    pub(crate) fn backend_enum(&self) -> InferenceBackend {
        match self.backend.as_str() {
            "ollama" => InferenceBackend::Ollama,
            "vllm" => InferenceBackend::VLLM,
            _ => InferenceBackend::LlamaCpp,
        }
    }

    pub(crate) fn backend_label(backend: &str) -> &'static str {
        match backend {
            "ollama" => "Ollama",
            "vllm" => "vLLM",
            _ => "llama.cpp",
        }
    }
}

#[cfg(feature = "xilem-shell")]
pub(crate) fn env_override(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(feature = "xilem-shell")]
pub(crate) struct PendingPilotPlan {
    pub(crate) intent: String,
    pub(crate) result_rx: mpsc::Receiver<Result<Vec<PilotAction>, String>>,
    pub(crate) started: Instant,
}

/// Structured record of one pilot run: intent -> typed plan -> analysis -> status.
/// Serializable so the egui shell can render it today and a future visual
/// dashboard (or MCP) can consume the same stream without a rewrite.
#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct PilotStepRecord {
    pub(crate) kind: String,
    pub(crate) detail: String,
}

#[cfg(feature = "xilem-shell")]
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct PilotRunArtifact {
    pub(crate) intent: String,
    pub(crate) planner: String,
    pub(crate) plan: Vec<PilotStepRecord>,
    pub(crate) analysis: Vec<String>,
    pub(crate) status: String,
    pub(crate) reason_ms: u128,
}

#[cfg(feature = "xilem-shell")]
pub(crate) fn pilot_action_record(action: &PilotAction) -> PilotStepRecord {
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
pub(crate) struct DrawStats {
    pub(crate) total: Duration,
    pub(crate) frame_blit: Option<Duration>,
    pub(crate) present: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuardDecision {
    pub(crate) action: &'static str,
    pub(crate) reason: String,
    pub(crate) allowed: bool,
    pub(crate) network_allowed: bool,
}

impl GuardDecision {
    pub(crate) fn summary(&self) -> String {
        let network = if self.network_allowed {
            "network allowed"
        } else {
            "network blocked"
        };
        format!("{} | {} | {}", self.action, network, self.reason)
    }
}
