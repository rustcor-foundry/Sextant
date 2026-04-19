use serde::{Deserialize, Serialize};
use serde_json;
use std::sync::Arc;
use tokio::sync::Mutex;
use sextant_vault::CitadelVault;
use sextant_engine::SextantEngine;
use sextant_wake::DigitalWake;
use sextant_log::{CaptainsLog, LogEntry, LogStatus};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PilotStatus {
    Idle,
    Reasoning,
    ReasoningComplete(String), // Signature of the plan
    Navigating(Url),
    Distilling,
    AwaitingConsent(String), // Message for the user
    ExecutingAction(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PilotAction {
    Navigate(Url),
    Distill,
    Perceive, // Multi-tab semantic capture
    PerceiveMultiModal, // Neural Bridge (audio/video) capture
    RequestConsent(String),
    Analyze(String),
    OpenTab(Url),
    SwitchTab(Uuid),
    CloseTab(Uuid),
}

/// The trait for the Pilot's reasoning engine.
/// This allows the browser to be vendor-agnostic (Gemini, Local Llama, etc.)
#[async_trait::async_trait]
pub trait PilotBrain: Send + Sync {
    async fn reason(&self, intent: &str, context: &[sextant_wake::WakeEntry]) -> Result<Vec<PilotAction>, String>;
    async fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
    fn name(&self) -> &str;
}

pub struct SextantPilot {
    vault: Arc<Mutex<CitadelVault>>,
    engine: Arc<Mutex<SextantEngine>>,
    wake: Arc<Mutex<DigitalWake>>,
    log: Arc<Mutex<CaptainsLog>>,
    brain: Box<dyn PilotBrain>,
    status: PilotStatus,
    current_plan: Vec<PilotAction>,
    plan_results: Vec<String>,
    active_persona_id: Option<String>,
    active_log_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct BrainPlanEnvelope {
    actions: Vec<BrainPlanAction>,
}

#[derive(Debug, Deserialize)]
struct BrainPlanAction {
    action: String,
    url: Option<String>,
    message: Option<String>,
    tab_id: Option<String>,
}

impl SextantPilot {
    pub fn new(
        vault: Arc<Mutex<CitadelVault>>,
        engine: Arc<Mutex<SextantEngine>>,
        wake: Arc<Mutex<DigitalWake>>,
        log: Arc<Mutex<CaptainsLog>>,
        brain: Box<dyn PilotBrain>,
    ) -> Self {
        Self {
            vault,
            engine,
            wake,
            log,
            brain,
            status: PilotStatus::Idle,
            current_plan: Vec::new(),
            plan_results: Vec::new(),
            active_persona_id: None,
            active_log_id: None,
        }
    }

    pub fn set_persona(&mut self, persona_id: Option<String>) {
        self.active_persona_id = persona_id;
    }

    pub fn status(&self) -> PilotStatus {
        self.status.clone()
    }

    pub fn brain_name(&self) -> &str {
        self.brain.name()
    }

    pub fn switch_brain(&mut self, brain: Box<dyn PilotBrain>) {
        self.brain = brain;
    }

    async fn update_active_log_status(
        &self,
        status: LogStatus,
        consent_signature: Option<String>,
    ) {
        if let Some(log_id) = self.active_log_id {
            let log_guard = self.log.lock().await;
            let _ = log_guard.update_status(&log_id, status, consent_signature);
        }
    }

    /// The primary entry point for natural language orchestration.
    pub async fn process_intent(&mut self, intent: &str) -> Result<String, String> {
        self.status = PilotStatus::Reasoning;
        self.plan_results.clear();
        
        let persona_id = self.active_persona_id.as_deref().unwrap_or("default");

        // 1. Query the Wake for context using Hybrid Search
        // Generate an embedding for the intent to enable semantic re-ranking
        let query_vector = self.brain.embed(intent).await.ok();
        
        let wake_guard = self.wake.lock().await;
        let context = wake_guard.search_hybrid(persona_id, intent, query_vector.as_deref()).map_err(|e| e.to_string())?;
        drop(wake_guard); // Release lock before reasoning
        
        // 2. Formulate a plan using the selected Brain
        let mut plan = self.brain.reason(intent, &context).await?;
        
        // 3. Sign the plan via the Vault (Captain's Key)
        // This provides a cryptographic proof that the Pilot's reasoning was authorized.
        let plan_json = serde_json::to_string(&plan).map_err(|e| e.to_string())?;
        let vault = self.vault.lock().await;
        let signature = vault.sign_consent(&plan_json)?;
        drop(vault);

        // 4. Record in Captain's Log
        let log_id = Uuid::new_v4();
        self.active_log_id = Some(log_id);
        let log_entry = LogEntry {
            id: log_id,
            timestamp: chrono::Utc::now(),
            persona_id: persona_id.to_string(),
            intent: intent.to_string(),
            plan_json: plan_json.clone(),
            signature: signature.clone(),
            consent_signature: None,
            status: LogStatus::AwaitingConsent,
        };
        let log_guard = self.log.lock().await;
        let _ = log_guard.record(&log_entry);
        drop(log_guard);

        self.status = PilotStatus::ReasoningComplete(signature);

        plan.reverse(); // Reverse so we can pop from the end
        self.current_plan = plan;
        
        // 4. Execute the plan
        let execution = self.execute_remaining_plan().await;
        match &execution {
            Ok(_) if !matches!(self.status, PilotStatus::AwaitingConsent(_)) => {
                self.update_active_log_status(LogStatus::Success, None).await;
            }
            Err(e) => {
                self.status = PilotStatus::Idle;
                self.update_active_log_status(LogStatus::Failure(e.clone()), None)
                    .await;
            }
            _ => {}
        }
        execution
    }

    /// Resumes execution after user provides Captain's Key consent.
    pub async fn provide_consent(&mut self, signature: &str) -> Result<String, String> {
        if let PilotStatus::AwaitingConsent(_) = self.status {
            self.plan_results.push(format!("Captain's Key Authorized. Signature: {}", signature));
            let consent_signature = Some(signature.to_string());
            let execution = self.execute_remaining_plan().await;
            match &execution {
                Ok(_) if !matches!(self.status, PilotStatus::AwaitingConsent(_)) => {
                    self.update_active_log_status(LogStatus::Success, consent_signature)
                        .await;
                }
                Ok(_) => {
                    self.update_active_log_status(LogStatus::AwaitingConsent, consent_signature)
                        .await;
                }
                Err(e) => {
                    self.status = PilotStatus::Idle;
                    self.update_active_log_status(
                        LogStatus::Failure(e.clone()),
                        consent_signature,
                    )
                    .await;
                }
            }
            execution
        } else {
            Err("Pilot is not awaiting consent.".into())
        }
    }

    pub async fn deny_consent(&mut self) -> Result<String, String> {
        if let PilotStatus::AwaitingConsent(message) = self.status.clone() {
            self.current_plan.clear();
            self.status = PilotStatus::Idle;
            self.plan_results
                .push(format!("Captain's Key denied: {}", message));
            self.update_active_log_status(LogStatus::Aborted, None).await;
            Ok("Consent request denied. Pending plan aborted.".to_string())
        } else {
            Err("Pilot is not awaiting consent.".into())
        }
    }

    async fn execute_remaining_plan(&mut self) -> Result<String, String> {
        while let Some(action) = self.current_plan.pop() {
            match action {
                PilotAction::Navigate(url) => {
                    self.status = PilotStatus::Navigating(url.clone());
                    let mut engine = self.engine.lock().await;
                    let persona_id = self.active_persona_id.as_deref().unwrap_or("default");
                    engine.navigate_with_fallback(url.clone(), persona_id).map_err(|e| e.to_string())?;
                    self.plan_results.push(format!("Navigated to {}", url));
                }
                PilotAction::Distill => {
                    self.status = PilotStatus::Distilling;
                    
                    let mut engine = self.engine.lock().await;
                    let page = engine.distill_current_page().map_err(|e| e.to_string())?;
                    drop(engine);

                    // Generate embedding for the content
                    let embedding = self.brain.embed(&page.content).await.ok();
                    
                    let persona_id = self.active_persona_id.as_deref().unwrap_or("default");
                    let wake = self.wake.lock().await;
                    wake.record(persona_id, &page, embedding).map_err(|e| e.to_string())?;
                    
                    self.plan_results.push(format!("Distilled page '{}' and recorded to Digital Wake.", page.title));
                }
                PilotAction::PerceiveMultiModal => {
                    self.status = PilotStatus::ExecutingAction("Perceiving via Neural Bridge...".into());
                    
                    let engine = self.engine.lock().await;
                    match engine.bridge_perceive() {
                        Ok(perception) => {
                            self.plan_results.push(format!("Neural Bridge Perception: {}", perception.summary));
                            if let Some(trans) = perception.transcription {
                                self.plan_results.push(format!("Audio: {}", trans));
                            }
                            if let Some(vis) = perception.visual_description {
                                self.plan_results.push(format!("Visual: {}", vis));
                            }
                        }
                        Err(e) => self.plan_results.push(format!("Neural Bridge Error: {}", e)),
                    }
                }
                PilotAction::Perceive => {
                    self.status = PilotStatus::Distilling;
                    
                    let mut engine = self.engine.lock().await;
                    let pages = engine.perceive_all_tabs();
                    drop(engine);
                    
                    let persona_id = self.active_persona_id.as_deref().unwrap_or("default");
                    let wake = self.wake.lock().await;
                    for page in &pages {
                        let embedding = self.brain.embed(&page.content).await.ok();
                        wake.record(persona_id, page, embedding).map_err(|e| e.to_string())?;
                    }
                    
                    self.plan_results.push(format!("Perceived {} tabs and recorded to Digital Wake.", pages.len()));
                }
                PilotAction::RequestConsent(msg) => {
                    self.status = PilotStatus::AwaitingConsent(msg.clone());
                    return Ok(format!("AWAITING_CONSENT: {}", msg));
                }
                PilotAction::OpenTab(url) => {
                    let mut engine = self.engine.lock().await;
                    let id = engine.open_tab();
                    engine.switch_to_tab(id).map_err(|e| e.to_string())?;
                    let persona = self.active_persona_id.as_deref().unwrap_or("default");
                    engine.navigate_with_fallback(url.clone(), persona).map_err(|e| e.to_string())?;
                    self.plan_results.push(format!("Opened new tab with URL {} (ID: {})", url, id));
                }
                PilotAction::SwitchTab(id) => {
                    let mut engine = self.engine.lock().await;
                    engine.switch_to_tab(id).map_err(|e| e.to_string())?;
                    self.plan_results.push(format!("Switched to tab {}", id));
                }
                PilotAction::CloseTab(id) => {
                    let mut engine = self.engine.lock().await;
                    engine.close_tab(&id).map_err(|e| e.to_string())?;
                    self.plan_results.push(format!("Closed tab {}", id));
                }
                PilotAction::Analyze(msg) => {
                    self.plan_results.push(format!("Pilot Analysis: {}", msg));
                }
            }
        }

        self.status = PilotStatus::Idle;
        Ok(self.plan_results.join("\n"))
    }
}

fn plan_prompt(intent: &str, context: &[sextant_wake::WakeEntry]) -> String {
    let context_summary = if context.is_empty() {
        "No relevant Wake context found.".to_string()
    } else {
        context
            .iter()
            .take(5)
            .map(|entry| format!("- {} | {} | {}", entry.title, entry.url, entry.timestamp))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "You are the Sextant Pilot for a sovereign browser.\n\
         Turn the user intent into a compact execution plan.\n\
         Return JSON only, with this exact shape:\n\
         {{\"actions\":[{{\"action\":\"navigate\",\"url\":\"https://example.com\"}},{{\"action\":\"distill\"}},{{\"action\":\"analyze\",\"message\":\"short summary\"}}]}}\n\
         Allowed action values: navigate, distill, perceive, perceive_multimodal, request_consent, analyze, open_tab, switch_tab, close_tab.\n\
         Include request_consent for destructive, purchasing, signing, or account-changing actions.\n\
         Prefer direct navigation when the intent clearly names a site or URL.\n\
         Wake context:\n{}\n\
         User intent: {}",
        context_summary, intent
    )
}

fn extract_json_payload(response: &str) -> &str {
    let trimmed = response.trim();
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        let after_lang = if let Some(newline) = after_fence.find('\n') {
            &after_fence[newline + 1..]
        } else {
            after_fence
        };
        if let Some(end) = after_lang.find("```") {
            return after_lang[..end].trim();
        }
    }
    trimmed
}

fn parse_uuid(value: &str) -> Option<Uuid> {
    Uuid::parse_str(value.trim()).ok()
}

fn parse_url_candidate(candidate: &str) -> Option<Url> {
    let trimmed = candidate
        .trim()
        .trim_matches(|c: char| ",.;:()[]{}<>\"'".contains(c));
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(url) = Url::parse(trimmed) {
        return Some(url);
    }
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return Url::parse(&format!("https://{}", trimmed)).ok();
    }
    None
}

fn first_url_in_text(text: &str) -> Option<Url> {
    text.split_whitespace().find_map(parse_url_candidate)
}

fn search_url(intent: &str) -> Url {
    let query = url::form_urlencoded::byte_serialize(intent.trim().as_bytes()).collect::<String>();
    Url::parse(&format!("https://duckduckgo.com/?q={}", query))
        .expect("search URL should always be valid")
}

fn intent_needs_consent(intent: &str) -> bool {
    let lowered = intent.to_ascii_lowercase();
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
    .any(|needle| lowered.contains(needle))
}

fn fallback_plan(intent: &str) -> Vec<PilotAction> {
    let mut actions = Vec::new();

    if intent_needs_consent(intent) {
        actions.push(PilotAction::Analyze(
            "Deterministic policy triggered consent gating for a sensitive action.".to_string(),
        ));
        actions.push(PilotAction::RequestConsent(
            "Authorize the sensitive action with the Captain's Key?".to_string(),
        ));
        return actions;
    }

    let target_url = first_url_in_text(intent).unwrap_or_else(|| search_url(intent));
    actions.push(PilotAction::Navigate(target_url.clone()));
    actions.push(PilotAction::Distill);
    actions.push(PilotAction::Analyze(format!(
        "Fallback planner selected {} for intent '{}'.",
        target_url, intent
    )));
    actions
}

fn parse_model_plan(intent: &str, response: &str) -> Result<Vec<PilotAction>, String> {
    let payload = extract_json_payload(response);
    let envelope: BrainPlanEnvelope = serde_json::from_str(payload)
        .or_else(|_| serde_json::from_str::<Vec<BrainPlanAction>>(payload).map(|actions| BrainPlanEnvelope { actions }))
        .map_err(|e| format!("Failed to parse model plan JSON: {}", e))?;

    if envelope.actions.is_empty() {
        return Ok(fallback_plan(intent));
    }

    let mut plan = Vec::new();
    for item in envelope.actions {
        let normalized = item.action.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "navigate" => {
                let url = item
                    .url
                    .as_deref()
                    .and_then(parse_url_candidate)
                    .ok_or_else(|| "Model plan navigate action missing valid URL.".to_string())?;
                plan.push(PilotAction::Navigate(url));
            }
            "distill" => plan.push(PilotAction::Distill),
            "perceive" => plan.push(PilotAction::Perceive),
            "perceive_multimodal" => plan.push(PilotAction::PerceiveMultiModal),
            "request_consent" => {
                let message = item
                    .message
                    .unwrap_or_else(|| "Authorize this sensitive action?".to_string());
                plan.push(PilotAction::RequestConsent(message));
            }
            "analyze" => {
                let message = item
                    .message
                    .unwrap_or_else(|| format!("Analysis complete for '{}'.", intent));
                plan.push(PilotAction::Analyze(message));
            }
            "open_tab" => {
                let url = item
                    .url
                    .as_deref()
                    .and_then(parse_url_candidate)
                    .ok_or_else(|| "Model plan open_tab action missing valid URL.".to_string())?;
                plan.push(PilotAction::OpenTab(url));
            }
            "switch_tab" => {
                let tab_id = item
                    .tab_id
                    .as_deref()
                    .and_then(parse_uuid)
                    .ok_or_else(|| "Model plan switch_tab action missing valid tab_id.".to_string())?;
                plan.push(PilotAction::SwitchTab(tab_id));
            }
            "close_tab" => {
                let tab_id = item
                    .tab_id
                    .as_deref()
                    .and_then(parse_uuid)
                    .ok_or_else(|| "Model plan close_tab action missing valid tab_id.".to_string())?;
                plan.push(PilotAction::CloseTab(tab_id));
            }
            _ => {}
        }
    }

    if plan.is_empty() {
        Ok(fallback_plan(intent))
    } else {
        Ok(plan)
    }
}

/// A Brain implementation that uses the Gemini API.
pub struct GeminiBrain {
    inference: SextantInference,
    config: InferenceConfig,
}

impl GeminiBrain {
    pub fn new(api_key: String, model: &str) -> Self {
        Self {
            inference: SextantInference::new(),
            config: InferenceConfig {
                backend: InferenceBackend::Gemini,
                endpoint: Url::parse("https://generativelanguage.googleapis.com").unwrap(),
                model_name: model.to_string(),
                api_key: Some(api_key),
                temperature: 0.7,
                max_tokens: 2048,
            },
        }
    }
}

#[async_trait::async_trait]
impl PilotBrain for GeminiBrain {
    fn name(&self) -> &str {
        "Gemini-1.5-Pro"
    }

    async fn reason(&self, intent: &str, context: &[sextant_wake::WakeEntry]) -> Result<Vec<PilotAction>, String> {
        let prompt = plan_prompt(intent, context);
        let response = self.inference.generate(&prompt, &self.config).await?;
        parse_model_plan(intent, &response).or_else(|_| Ok(fallback_plan(intent)))
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        self.inference.embed(text, &self.config).await
    }
}

/// A Brain implementation that uses the OpenAI API.
pub struct OpenAIBrain {
    inference: SextantInference,
    config: InferenceConfig,
}

impl OpenAIBrain {
    pub fn new(api_key: String, model: &str) -> Self {
        Self {
            inference: SextantInference::new(),
            config: InferenceConfig {
                backend: InferenceBackend::OpenAI,
                endpoint: Url::parse("https://api.openai.com").unwrap(),
                model_name: model.to_string(),
                api_key: Some(api_key),
                temperature: 0.7,
                max_tokens: 2048,
            },
        }
    }
}

#[async_trait::async_trait]
impl PilotBrain for OpenAIBrain {
    fn name(&self) -> &str {
        "GPT-4o"
    }

    async fn reason(&self, intent: &str, context: &[sextant_wake::WakeEntry]) -> Result<Vec<PilotAction>, String> {
        let prompt = plan_prompt(intent, context);
        let response = self.inference.generate(&prompt, &self.config).await?;
        parse_model_plan(intent, &response).or_else(|_| Ok(fallback_plan(intent)))
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        self.inference.embed(text, &self.config).await
    }
}

/// A Brain implementation that uses the Anthropic API.
pub struct AnthropicBrain {
    inference: SextantInference,
    config: InferenceConfig,
}

impl AnthropicBrain {
    pub fn new(api_key: String, model: &str) -> Self {
        Self {
            inference: SextantInference::new(),
            config: InferenceConfig {
                backend: InferenceBackend::Anthropic,
                endpoint: Url::parse("https://api.anthropic.com").unwrap(),
                model_name: model.to_string(),
                api_key: Some(api_key),
                temperature: 0.7,
                max_tokens: 2048,
            },
        }
    }
}

#[async_trait::async_trait]
impl PilotBrain for AnthropicBrain {
    fn name(&self) -> &str {
        "Claude-3.5-Sonnet"
    }

    async fn reason(&self, intent: &str, context: &[sextant_wake::WakeEntry]) -> Result<Vec<PilotAction>, String> {
        let prompt = plan_prompt(intent, context);
        let response = self.inference.generate(&prompt, &self.config).await?;
        parse_model_plan(intent, &response).or_else(|_| Ok(fallback_plan(intent)))
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        self.inference.embed(text, &self.config).await
    }
}

use sextant_inference::{SextantInference, InferenceConfig, InferenceBackend};

/// A Brain implementation that uses a local model (e.g., via llama.cpp, vLLM, Ollama, or MLC-LLM)
pub struct LocalBrain {
    inference: SextantInference,
    config: InferenceConfig,
}

impl LocalBrain {
    pub fn new(backend: InferenceBackend, endpoint: Url, model_name: &str) -> Self {
        Self {
            inference: SextantInference::new(),
            config: InferenceConfig {
                backend,
                endpoint,
                model_name: model_name.to_string(),
                api_key: None,
                temperature: 0.7,
                max_tokens: 512,
            },
        }
    }
}

#[async_trait::async_trait]
impl PilotBrain for LocalBrain {
    fn name(&self) -> &str {
        match self.config.backend {
            InferenceBackend::LlamaCpp => "Local-LlamaCpp",
            InferenceBackend::VLLM => "Local-vLLM",
            InferenceBackend::Ollama => "Local-Ollama",
            InferenceBackend::MLCLLM => "Local-MLC-LLM",
            InferenceBackend::OpenAI => "Local-OpenAI",
            InferenceBackend::Anthropic => "Local-Anthropic",
            InferenceBackend::Gemini => "Local-Gemini",
        }
    }

    async fn reason(&self, intent: &str, context: &[sextant_wake::WakeEntry]) -> Result<Vec<PilotAction>, String> {
        let prompt = plan_prompt(intent, context);
        let response = self.inference.generate(&prompt, &self.config).await?;
        parse_model_plan(intent, &response).or_else(|_| Ok(fallback_plan(intent)))
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        self.inference.embed(text, &self.config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sextant_log::CaptainsLog;
    use sextant_wake::DigitalWake;

    struct StubBrain {
        plan: Vec<PilotAction>,
    }

    #[async_trait::async_trait]
    impl PilotBrain for StubBrain {
        async fn reason(
            &self,
            _intent: &str,
            _context: &[sextant_wake::WakeEntry],
        ) -> Result<Vec<PilotAction>, String> {
            Ok(self.plan.clone())
        }

        async fn embed(&self, _text: &str) -> Result<Vec<f32>, String> {
            Ok(vec![0.0, 1.0, 0.5])
        }

        fn name(&self) -> &str {
            "Stub"
        }
    }

    async fn create_test_pilot(
        plan: Vec<PilotAction>,
    ) -> (
        SextantPilot,
        Arc<Mutex<SextantEngine>>,
        Arc<Mutex<DigitalWake>>,
        Arc<Mutex<CaptainsLog>>,
    ) {
        let vault = Arc::new(Mutex::new(CitadelVault::new()));
        {
            let mut vault_guard = vault.lock().await;
            vault_guard.initialize_new("password123").unwrap();
        }
        let engine = Arc::new(Mutex::new(SextantEngine::new()));
        let wake = Arc::new(Mutex::new(DigitalWake::new_in_memory().unwrap()));
        let log = Arc::new(Mutex::new(CaptainsLog::new_in_memory().unwrap()));

        let mut pilot = SextantPilot::new(
            vault,
            engine.clone(),
            wake.clone(),
            log.clone(),
            Box::new(StubBrain { plan }),
        );
        pilot.set_persona(Some("default".to_string()));

        (pilot, engine, wake, log)
    }

    #[tokio::test]
    async fn open_tab_action_targets_the_new_tab() {
        let target_url = Url::parse("https://example.com").unwrap();
        let (mut pilot, engine, _wake, _log) =
            create_test_pilot(vec![PilotAction::OpenTab(target_url.clone())]).await;

        let result = pilot.process_intent("open example in a new tab").await;
        assert!(result.is_ok(), "pilot returned error: {:?}", result.err());

        let engine_guard = engine.lock().await;
        let tabs = engine_guard.get_tabs();
        assert_eq!(tabs.len(), 2, "expected a new tab to be opened");
        let active_tab = engine_guard.get_active_tab().expect("active tab should exist");
        assert_eq!(active_tab.url.as_ref(), Some(&target_url));
    }

    #[tokio::test]
    async fn process_intent_navigate_and_distill_records_to_wake() {
        let target_url = Url::parse("about:blank").unwrap();
        let (mut pilot, engine, wake, log) = create_test_pilot(vec![
            PilotAction::Navigate(target_url.clone()),
            PilotAction::Distill,
        ])
        .await;

        let result = pilot.process_intent("open a blank page").await;
        assert!(result.is_ok(), "pilot returned error: {:?}", result.err());
        let result_text = result.unwrap();
        assert!(result_text.contains("Navigated to about:blank"));
        assert!(result_text.contains("recorded to Digital Wake"));

        let engine_guard = engine.lock().await;
        let active_tab = engine_guard.get_active_tab().expect("active tab should exist");
        let page = active_tab
            .distilled_page
            .as_ref()
            .expect("distill should update the active tab");
        assert_eq!(page.title, "Blank Page");
        assert_eq!(active_tab.url.as_ref(), Some(&target_url));
        drop(engine_guard);

        let wake_guard = wake.lock().await;
        let wake_entries = wake_guard.search("default", "").unwrap();
        assert_eq!(wake_entries.len(), 1, "expected one wake entry");
        assert_eq!(wake_entries[0].title, "Blank Page");
        assert_eq!(wake_entries[0].url, target_url);
        drop(wake_guard);

        let log_guard = log.lock().await;
        let log_entries = log_guard.get_entries("default", 10).unwrap();
        assert_eq!(log_entries.len(), 1, "expected a single captain's log entry");
        assert!(matches!(log_entries[0].status, LogStatus::Success));
    }

    #[tokio::test]
    async fn process_intent_request_consent_sets_awaiting_status_and_can_be_denied() {
        let (mut pilot, _engine, _wake, log) = create_test_pilot(vec![PilotAction::RequestConsent(
            "Authorize this sensitive action?".to_string(),
        )])
        .await;

        let result = pilot.process_intent("delete the current thing").await;
        assert!(result.is_ok(), "pilot returned error: {:?}", result.err());
        let result_text = result.unwrap();
        assert!(result_text.contains("AWAITING_CONSENT"));
        assert!(matches!(pilot.status(), PilotStatus::AwaitingConsent(_)));

        let deny_result = pilot.deny_consent().await;
        assert!(deny_result.is_ok(), "deny returned error: {:?}", deny_result.err());
        assert!(matches!(pilot.status(), PilotStatus::Idle));

        let log_guard = log.lock().await;
        let log_entries = log_guard.get_entries("default", 10).unwrap();
        assert_eq!(log_entries.len(), 1, "expected a single captain's log entry");
        assert!(matches!(log_entries[0].status, LogStatus::Aborted));
    }

    #[tokio::test]
    async fn provide_consent_resumes_plan_and_finishes_successfully() {
        let target_url = Url::parse("about:blank").unwrap();
        let (mut pilot, engine, wake, log) = create_test_pilot(vec![
            PilotAction::RequestConsent("Authorize blank-page navigation?".to_string()),
            PilotAction::Navigate(target_url.clone()),
            PilotAction::Distill,
        ])
        .await;

        let result = pilot.process_intent("do the protected blank-page flow").await;
        assert!(result.is_ok(), "pilot returned error: {:?}", result.err());
        assert!(matches!(pilot.status(), PilotStatus::AwaitingConsent(_)));

        let resume_result = pilot.provide_consent("test-signature").await;
        assert!(
            resume_result.is_ok(),
            "provide_consent returned error: {:?}",
            resume_result.err()
        );
        let resume_text = resume_result.unwrap();
        assert!(resume_text.contains("Captain's Key Authorized"));
        assert!(resume_text.contains("recorded to Digital Wake"));
        assert!(matches!(pilot.status(), PilotStatus::Idle));

        let engine_guard = engine.lock().await;
        let active_tab = engine_guard.get_active_tab().expect("active tab should exist");
        assert_eq!(active_tab.url.as_ref(), Some(&target_url));
        drop(engine_guard);

        let wake_guard = wake.lock().await;
        let wake_entries = wake_guard.search("default", "").unwrap();
        assert_eq!(wake_entries.len(), 1, "expected one wake entry after consent");
        drop(wake_guard);

        let log_guard = log.lock().await;
        let log_entries = log_guard.get_entries("default", 10).unwrap();
        assert_eq!(log_entries.len(), 1, "expected a single captain's log entry");
        assert_eq!(log_entries[0].consent_signature.as_deref(), Some("test-signature"));
        assert!(matches!(log_entries[0].status, LogStatus::Success));
    }
}
