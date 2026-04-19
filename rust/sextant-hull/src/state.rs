use crate::deferred::DeferredState;
use crate::app_core::{
    build_brain, spawn_command, AiProvider, AppCommand, AppEvent, AppServices, AppSession,
    ProviderConfig, RefreshSnapshot,
};
use sextant_airgap::SextantAirGap;
use sextant_engine::{DistilledPage, EngineStatus, SextantEngine, Tab};
use sextant_log::{CaptainsLog, LogEntry};
use sextant_pilot::{PilotStatus, SextantPilot};
use sextant_privacy::PrivacyLevel;
use sextant_vault::{CitadelVault, Persona};
use sextant_wake::{DigitalWake, WakeEntry};
use std::sync::{mpsc, Arc};
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

pub struct SextantState {
    pub runtime: Handle,
    pub vault: Arc<Mutex<CitadelVault>>,
    pub engine: Arc<Mutex<SextantEngine>>,
    pub wake: Arc<Mutex<DigitalWake>>,
    pub log: Arc<Mutex<CaptainsLog>>,
    pub pilot: Arc<Mutex<SextantPilot>>,
    pub intent: String,
    pub pilot_status: PilotStatus,
    pub pilot_brain_name: String,
    pub engine_status: Option<EngineStatus>,
    pub active_page: Option<DistilledPage>,
    pub recent_memory: Vec<WakeEntry>,
    pub audit_trail: Vec<LogEntry>,
    pub logs: Vec<String>,
    pub last_command_summary: String,
    pub last_command_is_error: bool,
    pub is_vault_unlocked: bool,
    pub active_persona: Option<Persona>,
    pub airgap: SextantAirGap,
    pub privacy_level: PrivacyLevel,
    pub is_mesh_open: bool,
    pub tabs: Vec<Tab>,
    pub active_tab_id: Option<Uuid>,
    pub is_settings_open: bool,
    pub applied_provider: AiProvider,
    pub selected_provider: AiProvider,
    pub gemini_key: String,
    pub openai_key: String,
    pub anthropic_key: String,
    pub local_endpoint: String,
    pub local_model: String,
    pub ai_status_message: String,
    pub ai_status_is_error: bool,
    pub last_tested_provider: Option<AiProvider>,
    pub last_test_success: bool,
    pub last_test_message: String,
    pub wake_search_query: String,
    pub wake_search_results: Vec<WakeEntry>,
    pub active_did: String,
    pub deferred: DeferredState,
    pub async_result_tx: mpsc::Sender<AppEvent>,
    pub async_result_rx: mpsc::Receiver<AppEvent>,
    pub pending_async_jobs: usize,
}

impl SextantState {
    pub fn add_log(&mut self, msg: &str) {
        self.logs.push(format!(
            "[{}] {}",
            chrono::Local::now().format("%H:%M:%S"),
            msg
        ));
    }

    pub fn set_command_summary(&mut self, message: impl Into<String>, is_error: bool) {
        self.last_command_summary = message.into();
        self.last_command_is_error = is_error;
    }

    pub async fn refresh_memory(&mut self) {
        let wake = self.wake.lock().await;
        let persona_id = self
            .active_persona
            .as_ref()
            .map(|p| p.id.as_str())
            .unwrap_or("default");
        if let Ok(entries) = wake.search(persona_id, "") {
            self.recent_memory = entries;
        }

        let log = self.log.lock().await;
        if let Ok(entries) = log.get_entries(persona_id, 10) {
            self.audit_trail = entries;
        }
    }

    pub async fn sync_tabs(&mut self) {
        let engine = self.engine.lock().await;
        self.tabs = engine.get_tabs();
        self.active_tab_id = engine.get_active_tab().map(|t| t.id);
        if let Some(tab) = engine.get_active_tab() {
            self.active_page = tab.distilled_page.clone();
            self.engine_status = Some(tab.status.clone());
        }
    }

    pub async fn refresh_pilot_snapshot(&mut self) {
        let pilot = self.pilot.lock().await;
        self.pilot_status = pilot.status();
        self.pilot_brain_name = pilot.brain_name().to_string();
    }

    pub fn apply_refresh_snapshot(&mut self, snapshot: RefreshSnapshot) {
        self.tabs = snapshot.tabs;
        self.active_tab_id = snapshot.active_tab_id;
        self.active_page = snapshot.active_page;
        self.engine_status = snapshot.engine_status;
        self.recent_memory = snapshot.recent_memory;
        self.audit_trail = snapshot.audit_trail;
        self.pilot_status = snapshot.pilot.status.clone();
        self.pilot_brain_name = snapshot.pilot.brain_name;
    }

    pub fn queue_async_command(&mut self, command: AppCommand) {
        let session = AppSession {
            persona_id: self
                .active_persona
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_else(|| "default".to_string()),
            current_airgap: self.airgap.get_status(),
            current_privacy: self.privacy_level.clone(),
            active_tab_id: self.active_tab_id,
        };
        let provider_config = ProviderConfig {
            provider: self.selected_provider,
            gemini_key: self.gemini_key.clone(),
            openai_key: self.openai_key.clone(),
            anthropic_key: self.anthropic_key.clone(),
            local_endpoint: self.local_endpoint.clone(),
            local_model: self.local_model.clone(),
        };
        let services = AppServices {
            vault: self.vault.clone(),
            pilot: self.pilot.clone(),
            engine: self.engine.clone(),
            wake: self.wake.clone(),
            log: self.log.clone(),
        };
        self.pending_async_jobs += 1;
        spawn_command(
            &self.runtime,
            self.async_result_tx.clone(),
            services,
            session,
            provider_config,
            command,
        );
    }

    pub fn provider_ready(&self, provider: AiProvider) -> bool {
        match provider {
            AiProvider::Gemini => !self.gemini_key.trim().is_empty(),
            AiProvider::OpenAI => !self.openai_key.trim().is_empty(),
            AiProvider::Anthropic => !self.anthropic_key.trim().is_empty(),
            AiProvider::Local => Url::parse(&self.local_endpoint).is_ok(),
        }
    }

    pub fn active_provider_ready(&self) -> bool {
        self.provider_ready(self.applied_provider)
    }

    pub fn active_provider_readiness_message(&self) -> String {
        match self.applied_provider {
            AiProvider::Gemini => "Active Gemini provider needs an API key.".to_string(),
            AiProvider::OpenAI => "Active OpenAI provider needs an API key.".to_string(),
            AiProvider::Anthropic => "Active Anthropic provider needs an API key.".to_string(),
            AiProvider::Local => {
                "Active local provider needs a valid endpoint before commands can run.".to_string()
            }
        }
    }

    pub fn latest_audit_status_line(&self) -> String {
        if let Some(entry) = self.audit_trail.first() {
            let status = match &entry.status {
                sextant_log::LogStatus::Success => "SUCCESS".to_string(),
                sextant_log::LogStatus::Failure(err) => format!("FAILURE {}", err),
                sextant_log::LogStatus::Aborted => "ABORTED".to_string(),
                sextant_log::LogStatus::AwaitingConsent => "AWAITING CONSENT".to_string(),
            };
            format!("LATEST AUDIT {} {}", status, entry.intent)
        } else {
            "LATEST AUDIT none yet".to_string()
        }
    }

    pub fn provider_button_label(&self, provider: AiProvider) -> String {
        let selected = if self.selected_provider == provider {
            "[selected]"
        } else {
            ""
        };
        let applied = if self.applied_provider == provider {
            "[active]"
        } else {
            ""
        };
        let ready = if self.provider_ready(provider) {
            "[ready]"
        } else {
            "[needs config]"
        };
        format!("{} {} {} {}", provider.label(), selected, applied, ready)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn select_provider(&mut self, provider: AiProvider) {
        self.selected_provider = provider;
        self.set_ai_status(
            format!(
                "{} selected. Apply to make it active.",
                self.selected_provider.label()
            ),
            !self.provider_ready(provider),
        );
    }

    pub async fn update_brain(&mut self) {
        let provider_config = ProviderConfig {
            provider: self.selected_provider,
            gemini_key: self.gemini_key.clone(),
            openai_key: self.openai_key.clone(),
            anthropic_key: self.anthropic_key.clone(),
            local_endpoint: self.local_endpoint.clone(),
            local_model: self.local_model.clone(),
        };
        if let Some(brain) = build_brain(&provider_config) {
            let mut pilot = self.pilot.lock().await;
            pilot.switch_brain(brain);
            self.pilot_brain_name = pilot.brain_name().to_string();
        }
    }

    pub fn drain_async_results(&mut self) {
        while let Ok(result) = self.async_result_rx.try_recv() {
            if self.pending_async_jobs > 0 {
                self.pending_async_jobs -= 1;
            }
            match result {
                AppEvent::IntentProcessed(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(message) => {
                            self.set_command_summary("Intent completed.", false);
                            self.add_log(&format!("Pilot complete: {}", message));
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Intent failed: {}", e), true);
                            self.add_log(&format!("Pilot failed: {}", e));
                        }
                    }
                }
                AppEvent::AirgapToggled {
                    next,
                    forced_local,
                    snapshot,
                } => {
                    self.airgap.set_status(next.clone());
                    if forced_local {
                        self.selected_provider = AiProvider::Local;
                        self.applied_provider = AiProvider::Local;
                        if self.local_model.trim().is_empty() {
                            self.local_model = "llama-3-8b".to_string();
                        }
                    }
                    self.apply_refresh_snapshot(snapshot);
                    self.add_log(&format!("Air-Gap status changed to: {:?}", next));
                    self.set_command_summary(
                        format!("Air-gap set to {:?}.", next),
                        false,
                    );
                    if forced_local {
                        self.add_log("Network isolated. Switched to local inference.");
                    }
                }
                AppEvent::PrivacyCycled { next, snapshot } => {
                    self.privacy_level = next.clone();
                    self.apply_refresh_snapshot(snapshot);
                    self.add_log(&format!("Neural Privacy Level set to: {:?}", next));
                    self.set_command_summary(
                        format!("Privacy set to {:?}.", next),
                        false,
                    );
                }
                AppEvent::WakeSearch(result) => match result {
                    Ok(results) => {
                        self.set_command_summary(
                            format!("Wake search returned {} result(s).", results.len()),
                            false,
                        );
                        self.wake_search_results = results;
                    }
                    Err(e) => self.add_log(&format!("Wake search failed: {}", e)),
                },
                AppEvent::WakeConsolidated(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(count) => {
                            self.set_command_summary(
                                format!("Wake consolidated. Pruned {} entries.", count),
                                false,
                            );
                            self.add_log(&format!("Wake consolidated. Pruned {} entries.", count))
                        }
                        Err(e) => {
                            self.set_command_summary(
                                format!("Wake consolidation failed: {}", e),
                                true,
                            );
                            self.add_log(&format!("Wake consolidation failed: {}", e));
                        }
                    }
                }
                AppEvent::ProviderSettingsApplied {
                    provider,
                    brain_name,
                } => {
                    if let Some(brain_name) = brain_name {
                        self.applied_provider = provider;
                        self.pilot_brain_name = brain_name;
                        self.set_ai_status(
                            format!(
                                "Applied {} configuration. Provider is now active.",
                                provider.label()
                            ),
                            false,
                        );
                        self.set_command_summary(
                            format!("Applied {} provider.", provider.label()),
                            false,
                        );
                        self.add_log(&format!("AI provider set to {}.", provider.label()));
                    } else {
                        self.set_ai_status(
                            format!(
                                "{} configuration is incomplete. Fill the required fields before applying.",
                                provider.label()
                            ),
                            true,
                        );
                        self.add_log(&format!(
                            "AI provider {} is missing required configuration.",
                            provider.label()
                        ));
                        self.set_command_summary(
                            format!("{} provider config is incomplete.", provider.label()),
                            true,
                        );
                    }
                }
                AppEvent::ProviderSettingsTested { provider, result } => match result {
                    Ok(message) => {
                        self.last_tested_provider = Some(provider);
                        self.last_test_success = true;
                        self.last_test_message = message.clone();
                        self.set_ai_status(
                            format!("{} test succeeded. Response: {}", provider.label(), message),
                            false,
                        );
                        self.set_command_summary(
                            format!("{} provider test passed.", provider.label()),
                            false,
                        );
                        self.add_log(&format!("AI provider {} test succeeded.", provider.label()));
                    }
                    Err(e) => {
                        self.last_tested_provider = Some(provider);
                        self.last_test_success = false;
                        self.last_test_message = e.clone();
                        self.set_ai_status(format!("{} test failed: {}", provider.label(), e), true);
                        self.set_command_summary(
                            format!("{} provider test failed.", provider.label()),
                            true,
                        );
                        self.add_log(&format!("AI provider {} test failed: {}", provider.label(), e));
                    }
                },
                AppEvent::ProviderSettingsSaved(result) => match result {
                    Ok(_) => {
                        self.set_ai_status("Saved AI settings to Vault.", false);
                        self.set_command_summary("Saved AI settings to Vault.", false);
                        self.add_log("API keys encrypted and stored in Vault.");
                    }
                    Err(e) => {
                        self.set_ai_status(format!("Failed to save AI settings: {}", e), true);
                        self.set_command_summary(format!("Save failed: {}", e), true);
                        self.add_log(&format!("Failed to save AI settings: {}", e));
                    }
                },
                AppEvent::ProviderSettingsLoaded {
                    provider,
                    gemini_key,
                    openai_key,
                    anthropic_key,
                    local_endpoint,
                    local_model,
                    result,
                } => match result {
                    Ok(_) => {
                        if let Some(provider) = provider {
                            self.selected_provider = provider;
                        }
                        if let Some(value) = gemini_key {
                            self.gemini_key = value;
                        }
                        if let Some(value) = openai_key {
                            self.openai_key = value;
                        }
                        if let Some(value) = anthropic_key {
                            self.anthropic_key = value;
                        }
                        if let Some(value) = local_endpoint {
                            self.local_endpoint = value;
                        }
                        if let Some(value) = local_model {
                            self.local_model = value;
                        }
                        self.set_ai_status("Loaded AI settings from Vault.", false);
                        self.set_command_summary("Loaded AI settings from Vault.", false);
                        self.add_log("Loaded AI settings from Vault.");
                    }
                    Err(e) => {
                        self.set_ai_status(format!("Failed to load AI settings: {}", e), true);
                        self.set_command_summary(format!("Load failed: {}", e), true);
                        self.add_log(&format!("Failed to load AI settings: {}", e));
                    }
                },
                AppEvent::ConsentAuthorized(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(message) => {
                            self.set_command_summary("Consent authorized.", false);
                            self.add_log(&format!("Action Authorized: {}", message));
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Authorization failed: {}", e), true);
                            self.add_log(&format!("Authorization Failed: {}", e));
                        }
                    }
                }
                AppEvent::ConsentDenied(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(message) => {
                            self.set_command_summary("Consent denied. Plan aborted.", false);
                            self.add_log(&message);
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Consent deny failed: {}", e), true);
                            self.add_log(&format!("Consent deny failed: {}", e));
                        }
                    }
                }
                AppEvent::TabCreated(tab_id, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    self.set_command_summary("Opened a new tab.", false);
                    self.add_log(&format!("Opened new tab {}.", tab_id));
                }
                AppEvent::TabSwitched(tab_id, result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(_) => {
                            self.set_command_summary("Switched active tab.", false);
                            self.add_log(&format!("Switched to tab {}.", tab_id));
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Failed to switch tab: {}", e), true);
                            self.add_log(&format!("Failed to switch tab: {}", e));
                        }
                    }
                }
                AppEvent::ActiveTabClosed(tab_id, result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(_) => {
                            self.set_command_summary("Closed active tab.", false);
                            self.add_log(&format!("Closed tab {}.", tab_id));
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Failed to close tab: {}", e), true);
                            self.add_log(&format!("Failed to close tab: {}", e));
                        }
                    }
                }
            }
        }
    }

    pub fn set_ai_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.ai_status_message = message.into();
        self.ai_status_is_error = is_error;
    }

    pub fn intent_route_message(&self) -> String {
        let mut msg = format!(
            "COMMAND will use active provider {}.",
            self.applied_provider.label()
        );
        if self.selected_provider != self.applied_provider {
            msg.push_str(&format!(
                " {} is selected in the panel but not yet applied.",
                self.selected_provider.label()
            ));
        }
        if self.last_tested_provider == Some(self.applied_provider) {
            if self.last_test_success {
                msg.push_str(" Active provider passed the most recent test.");
            } else {
                msg.push_str(" Active provider failed the most recent test.");
            }
        } else {
            msg.push_str(" Active provider has not been tested since the latest switch.");
        }
        msg
    }
}
