use crate::app_core::{
    build_brain, spawn_command, AiProvider, AppCommand, AppEvent, AppServices, AppSession,
    ProviderConfig, RefreshSnapshot, StartupPhase,
};
use crate::deferred::DeferredState;
use chrono::{DateTime, Utc};
use sextant_airgap::{AirGapStatus, SextantAirGap};
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
    pub location_input: String,
    pub startup_phase: StartupPhase,
    pub startup_detail: String,
    pub startup_bootstrap_requested: bool,
    pub pilot_status: PilotStatus,
    pub pilot_brain_name: String,
    pub engine_status: Option<EngineStatus>,
    pub active_page: Option<DistilledPage>,
    pub recent_memory: Vec<WakeEntry>,
    pub audit_trail: Vec<LogEntry>,
    pub logs: Vec<String>,
    pub startup_alerts: Vec<String>,
    pub validation_successes: Vec<String>,
    pub consent_request_exercised: bool,
    pub consent_authorized_exercised: bool,
    pub consent_denied_exercised: bool,
    pub provider_load_exercised: bool,
    pub provider_apply_exercised: bool,
    pub provider_test_exercised: bool,
    pub provider_save_exercised: bool,
    pub tab_open_exercised: bool,
    pub tab_switch_exercised: bool,
    pub tab_close_exercised: bool,
    pub wake_search_exercised: bool,
    pub wake_consolidate_exercised: bool,
    pub airgap_offline_exercised: bool,
    pub airgap_online_exercised: bool,
    pub privacy_cycle_exercised: bool,
    pub audit_entry_exercised: bool,
    pub audit_baseline_timestamp: Option<DateTime<Utc>>,
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
    pub applied_provider_config: ProviderConfig,
    pub preferred_online_provider_config: ProviderConfig,
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

    pub fn tab_label(&self, tab_id: Uuid) -> String {
        self.tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .and_then(|tab| tab.url.as_ref().map(|url| url.to_string()))
            .unwrap_or_else(|| "about:blank".to_string())
    }

    pub fn active_tab_can_go_back(&self) -> bool {
        self.active_tab_id
            .and_then(|tab_id| self.tabs.iter().find(|tab| tab.id == tab_id))
            .map(|tab| tab.can_go_back)
            .unwrap_or(false)
    }

    pub fn active_tab_can_go_forward(&self) -> bool {
        self.active_tab_id
            .and_then(|tab_id| self.tabs.iter().find(|tab| tab.id == tab_id))
            .map(|tab| tab.can_go_forward)
            .unwrap_or(false)
    }

    pub fn current_location_text(&self) -> String {
        self.active_tab_id
            .and_then(|tab_id| self.tabs.iter().find(|tab| tab.id == tab_id))
            .and_then(|tab| tab.url.as_ref().map(|url| url.to_string()))
            .or_else(|| self.active_page.as_ref().map(|page| page.url.to_string()))
            .unwrap_or_default()
    }

    pub fn sync_location_input_from_active_tab(&mut self) {
        self.location_input = self.current_location_text();
    }

    pub fn startup_allows_interaction(&self) -> bool {
        matches!(
            self.startup_phase,
            StartupPhase::Ready | StartupPhase::Degraded
        )
    }

    pub fn set_startup_phase(&mut self, phase: StartupPhase, detail: impl Into<String>) {
        self.startup_phase = phase;
        self.startup_detail = detail.into();
    }

    pub fn lifecycle_log(&mut self, message: impl Into<String>) {
        self.add_log(&format!("LIFECYCLE: {}", message.into()));
    }

    pub fn ensure_startup_bootstrap(&mut self) {
        if self.startup_bootstrap_requested {
            return;
        }
        self.startup_bootstrap_requested = true;
        self.set_startup_phase(
            StartupPhase::WarmingUp,
            "Window painted. Bringing up the shell first; engine work is deferred until browser use.",
        );
        self.lifecycle_log("Queued runtime bootstrap after first paint.");
        self.queue_async_command(AppCommand::BootstrapRuntime);
    }

    pub fn add_startup_alert(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.startup_alerts.push(message.clone());
        self.add_log(&format!("STARTUP ALERT: {}", message));
        if self.last_command_summary == "Awaiting first command." {
            self.set_command_summary(format!("Startup alert: {}", message), true);
        }
    }

    pub fn record_validation_step(&mut self, workflow: &str, step: impl Into<String>) {
        let step = step.into();
        let entry = format!(
            "[{}] [{}] {}",
            chrono::Local::now().format("%H:%M:%S"),
            workflow,
            step
        );
        self.validation_successes.push(entry);
        if self.validation_successes.len() > 8 {
            let drop_count = self.validation_successes.len() - 8;
            self.validation_successes.drain(0..drop_count);
        }
    }

    pub fn latest_validation_step_for(&self, workflow: &str) -> String {
        let needle = format!("[{}]", workflow);
        self.validation_successes
            .iter()
            .rev()
            .find(|entry| entry.contains(&needle))
            .cloned()
            .unwrap_or_else(|| format!("[--:--:--] [{}] no completed step yet", workflow))
    }

    pub fn reset_validation_session(&mut self) {
        self.validation_successes.clear();
        self.consent_request_exercised = false;
        self.consent_authorized_exercised = false;
        self.consent_denied_exercised = false;
        self.provider_load_exercised = false;
        self.provider_apply_exercised = false;
        self.provider_test_exercised = false;
        self.provider_save_exercised = false;
        self.tab_open_exercised = false;
        self.tab_switch_exercised = false;
        self.tab_close_exercised = false;
        self.wake_search_exercised = false;
        self.wake_consolidate_exercised = false;
        self.airgap_offline_exercised = false;
        self.airgap_online_exercised = false;
        self.privacy_cycle_exercised = false;
        self.audit_entry_exercised = false;
        self.audit_baseline_timestamp = self.audit_trail.first().map(|entry| entry.timestamp);
    }

    pub fn consent_validation_steps_completed(&self) -> usize {
        usize::from(self.consent_request_exercised)
            + usize::from(self.consent_authorized_exercised)
            + usize::from(self.consent_denied_exercised)
    }

    pub fn consent_validation_status_message(&self) -> String {
        let mut completed = Vec::new();
        if self.consent_request_exercised {
            completed.push("request seen");
        }
        if self.consent_authorized_exercised {
            completed.push("authorize exercised");
        }
        if self.consent_denied_exercised {
            completed.push("deny exercised");
        }

        let detail = if completed.is_empty() {
            "no Captain's Key steps exercised yet".to_string()
        } else {
            completed.join(", ")
        };

        format!(
            "Captain's Key coverage {}/3: {}.",
            self.consent_validation_steps_completed(),
            detail
        )
    }

    pub fn provider_validation_steps_completed(&self) -> usize {
        usize::from(self.provider_load_exercised)
            + usize::from(self.provider_apply_exercised)
            + usize::from(self.provider_test_exercised)
            + usize::from(self.provider_save_exercised)
    }

    pub fn provider_validation_status_message(&self) -> String {
        let mut completed = Vec::new();
        if self.provider_load_exercised {
            completed.push("load exercised");
        }
        if self.provider_apply_exercised {
            completed.push("apply exercised");
        }
        if self.provider_test_exercised {
            completed.push("test exercised");
        }
        if self.provider_save_exercised {
            completed.push("save exercised");
        }

        let detail = if completed.is_empty() {
            "no provider actions exercised yet".to_string()
        } else {
            completed.join(", ")
        };

        format!(
            "Provider coverage {}/4: {}.",
            self.provider_validation_steps_completed(),
            detail
        )
    }

    pub fn tab_validation_steps_completed(&self) -> usize {
        usize::from(self.tab_open_exercised)
            + usize::from(self.tab_switch_exercised)
            + usize::from(self.tab_close_exercised)
    }

    pub fn tab_validation_status_message(&self) -> String {
        let mut completed = Vec::new();
        if self.tab_open_exercised {
            completed.push("open exercised");
        }
        if self.tab_switch_exercised {
            completed.push("switch exercised");
        }
        if self.tab_close_exercised {
            completed.push("close exercised");
        }

        let detail = if completed.is_empty() {
            "no tab actions exercised yet".to_string()
        } else {
            completed.join(", ")
        };

        format!(
            "Tab coverage {}/3: {}.",
            self.tab_validation_steps_completed(),
            detail
        )
    }

    pub fn wake_validation_steps_completed(&self) -> usize {
        usize::from(self.wake_search_exercised) + usize::from(self.wake_consolidate_exercised)
    }

    pub fn wake_validation_status_message(&self) -> String {
        let mut completed = Vec::new();
        if self.wake_search_exercised {
            completed.push("search exercised");
        }
        if self.wake_consolidate_exercised {
            completed.push("consolidate exercised");
        }

        let detail = if completed.is_empty() {
            "no Wake actions exercised yet".to_string()
        } else {
            completed.join(", ")
        };

        format!(
            "Wake coverage {}/2: {}.",
            self.wake_validation_steps_completed(),
            detail
        )
    }

    pub fn control_validation_steps_completed(&self) -> usize {
        usize::from(self.airgap_offline_exercised)
            + usize::from(self.airgap_online_exercised)
            + usize::from(self.privacy_cycle_exercised)
    }

    pub fn control_validation_status_message(&self) -> String {
        let mut completed = Vec::new();
        if self.airgap_offline_exercised {
            completed.push("offline air-gap exercised");
        }
        if self.airgap_online_exercised {
            completed.push("online air-gap exercised");
        }
        if self.privacy_cycle_exercised {
            completed.push("privacy cycle exercised");
        }

        let detail = if completed.is_empty() {
            "no control toggles exercised yet".to_string()
        } else {
            completed.join(", ")
        };

        format!(
            "Control coverage {}/3: {}.",
            self.control_validation_steps_completed(),
            detail
        )
    }

    pub fn audit_validation_steps_completed(&self) -> usize {
        usize::from(self.audit_entry_exercised)
    }

    pub fn audit_validation_status_message(&self) -> String {
        if self.audit_entry_exercised {
            "Audit coverage 1/1: fresh Captain's Log entry recorded during this validation session."
                .to_string()
        } else if self.audit_baseline_timestamp.is_some() {
            "Audit coverage 0/1: no newer Captain's Log entry has been recorded since the last validation reset."
                .to_string()
        } else {
            "Audit coverage 0/1: no Captain's Log baseline exists yet for this validation session."
                .to_string()
        }
    }

    pub fn total_validation_workflow_steps(&self) -> usize {
        16
    }

    pub fn completed_validation_workflow_steps(&self) -> usize {
        self.provider_validation_steps_completed()
            + self.consent_validation_steps_completed()
            + self.tab_validation_steps_completed()
            + self.wake_validation_steps_completed()
            + self.control_validation_steps_completed()
            + self.audit_validation_steps_completed()
    }

    pub fn validation_progress_percent(&self) -> usize {
        (self.completed_validation_workflow_steps() * 100) / self.total_validation_workflow_steps()
    }

    pub fn validation_progress_line(&self) -> String {
        format!(
            "VALIDATION COVERAGE {}% ({}/{} workflow steps exercised).",
            self.validation_progress_percent(),
            self.completed_validation_workflow_steps(),
            self.total_validation_workflow_steps()
        )
    }

    pub fn validation_workflow_rollup_line(&self) -> String {
        format!(
            "WORKFLOWS provider {}/4  consent {}/3  tab {}/3  wake {}/2  controls {}/3  audit {}/1",
            self.provider_validation_steps_completed(),
            self.consent_validation_steps_completed(),
            self.tab_validation_steps_completed(),
            self.wake_validation_steps_completed(),
            self.control_validation_steps_completed(),
            self.audit_validation_steps_completed()
        )
    }

    pub fn update_audit_validation_progress(&mut self) {
        let latest_timestamp = self.audit_trail.first().map(|entry| entry.timestamp);
        self.audit_entry_exercised = match (self.audit_baseline_timestamp, latest_timestamp) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(baseline), Some(latest)) => latest > baseline,
        };
    }

    pub async fn refresh_memory(&mut self) {
        let persona_id = self
            .active_persona
            .as_ref()
            .map(|p| p.id.as_str())
            .unwrap_or("default")
            .to_string();
        {
            let wake = self.wake.lock().await;
            if let Ok(entries) = wake.search(&persona_id, "") {
                self.recent_memory = entries;
            }
        }

        {
            let log = self.log.lock().await;
            if let Ok(entries) = log.get_entries(&persona_id, 10) {
                self.audit_trail = entries;
            }
        }
        self.update_audit_validation_progress();
    }

    #[allow(dead_code)]
    pub async fn sync_tabs(&mut self) {
        let (tabs, active_tab_id, active_page, engine_status) = {
            let engine = self.engine.lock().await;
            let active_tab = engine.get_active_tab().cloned();
            (
                engine.get_tabs(),
                active_tab.as_ref().map(|tab| tab.id),
                active_tab
                    .as_ref()
                    .and_then(|tab| tab.distilled_page.clone()),
                active_tab.map(|tab| tab.status),
            )
        };
        self.tabs = tabs;
        self.active_tab_id = active_tab_id;
        if let Some(page) = active_page {
            self.active_page = Some(page);
            self.engine_status = engine_status;
        } else {
            self.active_page = None;
            self.engine_status = None;
        }
        self.sync_location_input_from_active_tab();
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
        self.sync_location_input_from_active_tab();
        self.update_audit_validation_progress();
    }

    pub fn queue_async_command(&mut self, command: AppCommand) {
        let command_label = command.label();
        let session = AppSession {
            persona_id: self
                .active_persona
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_else(|| "default".to_string()),
            current_airgap: self.airgap.get_status(),
            current_privacy: self.privacy_level.clone(),
            active_tab_id: self.active_tab_id,
            preferred_online_provider_config: self.preferred_online_provider_config.clone(),
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
        let was_idle = self.pending_async_jobs == 0;
        self.pending_async_jobs += 1;
        if was_idle {
            self.lifecycle_log(format!(
                "Async queue activated by {}. {} job in flight.",
                command_label, self.pending_async_jobs
            ));
        } else {
            self.add_log(&format!(
                "Queued {}. {} async job(s) in flight.",
                command_label, self.pending_async_jobs
            ));
        }
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

    pub fn current_provider_config(&self) -> ProviderConfig {
        ProviderConfig {
            provider: self.selected_provider,
            gemini_key: self.gemini_key.clone(),
            openai_key: self.openai_key.clone(),
            anthropic_key: self.anthropic_key.clone(),
            local_endpoint: self.local_endpoint.clone(),
            local_model: self.local_model.clone(),
        }
    }

    pub fn has_unapplied_provider_changes(&self) -> bool {
        self.current_provider_config() != self.applied_provider_config
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

    pub fn remaining_validation_checks(&self) -> Vec<String> {
        let mut blockers = Vec::new();
        let mut active_flow = Vec::new();
        let mut workflow_gaps = Vec::new();
        let mut evidence_gaps = Vec::new();

        if self.has_unapplied_provider_changes() {
            blockers.push("Apply the selected provider settings.".to_string());
        } else if !self.active_provider_ready() {
            blockers.push(self.active_provider_readiness_message());
        }

        if self.last_tested_provider != Some(self.applied_provider) {
            active_flow.push(format!(
                "Run TEST for the active {} route.",
                self.applied_provider.label()
            ));
        } else if !self.last_test_success {
            blockers.push(format!(
                "Fix the failing {} provider test before relying on this route.",
                self.applied_provider.label()
            ));
        }

        if self.airgap.get_status() != AirGapStatus::Online
            && !self.provider_ready(AiProvider::Local)
        {
            blockers.push("Configure the local endpoint before using offline mode.".to_string());
        }

        if !self.provider_load_exercised {
            workflow_gaps
                .push("LOAD VAULT once to validate provider settings restore.".to_string());
        }
        if !self.provider_apply_exercised {
            workflow_gaps
                .push("APPLY a provider configuration to validate route activation.".to_string());
        }
        if !self.provider_test_exercised {
            workflow_gaps.push("TEST the active provider to validate route health.".to_string());
        }
        if !self.provider_save_exercised {
            workflow_gaps
                .push("SAVE VAULT once to validate provider settings persistence.".to_string());
        }

        if matches!(self.pilot_status, PilotStatus::AwaitingConsent(_)) {
            active_flow.push(
                "Resolve the pending Captain's Key request with AUTHORIZE or DENY.".to_string(),
            );
        } else if !self.consent_request_exercised {
            workflow_gaps.push("Run a protected intent to open Captain's Key flow.".to_string());
        } else {
            if !self.consent_authorized_exercised {
                workflow_gaps.push(
                    "AUTHORIZE one Captain's Key request to validate plan resume.".to_string(),
                );
            }
            if !self.consent_denied_exercised {
                workflow_gaps
                    .push("DENY one Captain's Key request to validate plan abort.".to_string());
            }
        }

        if !self.tab_open_exercised {
            workflow_gaps.push("Open a tab to validate tab creation.".to_string());
        }
        if !self.tab_switch_exercised {
            workflow_gaps.push("Switch tabs once to validate active-tab changes.".to_string());
        }
        if !self.tab_close_exercised {
            workflow_gaps.push("Close an active tab once to validate tab teardown.".to_string());
        }

        if !self.wake_search_exercised {
            workflow_gaps.push("Run SEARCH to validate Wake query flow.".to_string());
        }
        if !self.wake_consolidate_exercised {
            workflow_gaps.push("Run CONSOLIDATE to validate Wake pruning flow.".to_string());
        }
        if self.wake_search_results.is_empty() && self.recent_memory.is_empty() {
            evidence_gaps
                .push("Run SEARCH or complete an intent to confirm Wake visibility.".to_string());
        }

        if !self.airgap_offline_exercised {
            workflow_gaps.push(
                "Toggle air-gap into an offline mode to validate local-route enforcement."
                    .to_string(),
            );
        }
        if !self.airgap_online_exercised {
            workflow_gaps
                .push("Return air-gap to Online to validate route restoration.".to_string());
        }
        if !self.privacy_cycle_exercised {
            workflow_gaps
                .push("Cycle privacy once to validate runtime privacy controls.".to_string());
        }

        if !self.audit_entry_exercised {
            evidence_gaps.push(
                "Run a command that produces a new Captain's Log entry after reset.".to_string(),
            );
        }

        if self.validation_successes.is_empty() {
            evidence_gaps.push(
                "Complete one successful workflow step to seed the validation trail.".to_string(),
            );
        }

        let mut checks = Vec::new();
        checks.extend(
            blockers
                .into_iter()
                .map(|check| format!("BLOCKER {}", check)),
        );
        checks.extend(
            active_flow
                .into_iter()
                .map(|check| format!("ACTIVE {}", check)),
        );
        checks.extend(
            workflow_gaps
                .into_iter()
                .map(|check| format!("WORKFLOW {}", check)),
        );
        checks.extend(
            evidence_gaps
                .into_iter()
                .map(|check| format!("EVIDENCE {}", check)),
        );
        checks
    }

    pub fn validation_focus_line(&self) -> String {
        let remaining = self.remaining_validation_checks();
        if let Some(top) = remaining.first() {
            format!("CURRENT FOCUS {}", top)
        } else {
            "CURRENT FOCUS no validation blockers or gaps remain.".to_string()
        }
    }

    pub fn validation_summary_line(&self) -> String {
        let remaining = self.remaining_validation_checks();
        if remaining.is_empty() {
            "VALIDATION STATUS all current checklist items are satisfied.".to_string()
        } else {
            format!(
                "VALIDATION STATUS {} remaining check{}.",
                remaining.len(),
                if remaining.len() == 1 { "" } else { "s" }
            )
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
        if self.selected_provider != provider {
            self.invalidate_provider_test("Provider selection changed.");
        }
        self.selected_provider = provider;
        let airgap_status = self.airgap.get_status();
        let provider_ready = self.provider_ready(provider);
        let (summary, is_error) = if airgap_status != AirGapStatus::Online
            && provider != AiProvider::Local
        {
            (
                format!(
                    "{} selected in settings. Air-gap is {:?}, so commands still require local inference until you return online and apply.",
                    self.selected_provider.label(),
                    airgap_status
                ),
                true,
            )
        } else {
            (
                format!(
                    "{} selected in settings. Apply to make it active.",
                    self.selected_provider.label()
                ),
                !provider_ready,
            )
        };
        self.set_command_summary(summary.clone(), is_error);
        self.set_ai_status(
            if airgap_status != AirGapStatus::Online && provider != AiProvider::Local {
                format!(
                    "{} selected, but air-gap {:?} still enforces local inference.",
                    self.selected_provider.label(),
                    airgap_status
                )
            } else {
                format!(
                    "{} selected. Apply to make it active.",
                    self.selected_provider.label()
                )
            },
            is_error,
        );
    }

    #[allow(dead_code)]
    pub fn set_gemini_key(&mut self, value: String) {
        if self.gemini_key != value {
            self.gemini_key = value;
            self.invalidate_provider_test("Gemini settings changed.");
        }
    }

    #[allow(dead_code)]
    pub fn set_openai_key(&mut self, value: String) {
        if self.openai_key != value {
            self.openai_key = value;
            self.invalidate_provider_test("OpenAI settings changed.");
        }
    }

    #[allow(dead_code)]
    pub fn set_anthropic_key(&mut self, value: String) {
        if self.anthropic_key != value {
            self.anthropic_key = value;
            self.invalidate_provider_test("Anthropic settings changed.");
        }
    }

    #[allow(dead_code)]
    pub fn set_local_endpoint(&mut self, value: String) {
        if self.local_endpoint != value {
            self.local_endpoint = value;
            self.invalidate_provider_test("Local endpoint changed.");
        }
    }

    #[allow(dead_code)]
    pub fn set_local_model(&mut self, value: String) {
        if self.local_model != value {
            self.local_model = value;
            self.invalidate_provider_test("Local model changed.");
        }
    }

    pub fn invalidate_provider_test(&mut self, reason: &str) {
        self.last_tested_provider = None;
        self.last_test_success = false;
        self.last_test_message = "No provider test matches the current settings yet.".to_string();
        self.add_log(&format!("Provider test invalidated: {}", reason));
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
        let mut processed = 0usize;
        while let Ok(result) = self.async_result_rx.try_recv() {
            processed += 1;
            if self.pending_async_jobs > 0 {
                self.pending_async_jobs -= 1;
            }
            match result {
                AppEvent::RuntimeBootstrapped {
                    phase,
                    alerts,
                    notes,
                    snapshot,
                } => {
                    self.apply_refresh_snapshot(snapshot);
                    self.startup_alerts.clear();
                    for alert in alerts {
                        self.add_startup_alert(alert);
                    }
                    for note in notes {
                        self.lifecycle_log(note);
                    }
                    match phase {
                        StartupPhase::Ready => {
                            self.set_startup_phase(
                                StartupPhase::Ready,
                                "Runtime ready. Browser controls are live.",
                            );
                            self.set_command_summary("Startup finished. Hull is ready.", false);
                        }
                        StartupPhase::Degraded => {
                            self.set_startup_phase(
                                StartupPhase::Degraded,
                                "Runtime is usable with alerts. Check startup status before manual testing.",
                            );
                            self.set_command_summary(
                                "Startup finished with alerts. Hull is usable but degraded.",
                                true,
                            );
                        }
                        StartupPhase::Booting | StartupPhase::WarmingUp => {
                            self.set_startup_phase(
                                StartupPhase::WarmingUp,
                                "Runtime bootstrap is still in progress.",
                            );
                        }
                    }
                }
                AppEvent::IntentProcessed(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(message) => {
                            if let PilotStatus::AwaitingConsent(request) = self.pilot_status.clone()
                            {
                                self.consent_request_exercised = true;
                                self.set_command_summary(
                                    format!("Captain's Key required: {}", request),
                                    false,
                                );
                                self.add_log(&format!("Pilot paused for consent: {}", request));
                                self.record_validation_step(
                                    "CONSENT",
                                    "Protected intent reached Captain's Key consent gate.",
                                );
                            } else {
                                self.set_command_summary("Intent completed.", false);
                                self.add_log(&format!("Pilot complete: {}", message));
                                self.record_validation_step(
                                    "INTENT",
                                    "Intent completed through the active route.",
                                );
                            }
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Intent failed: {}", e), true);
                            self.add_log(&format!("Pilot failed: {}", e));
                        }
                    }
                }
                AppEvent::DirectNavigation(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(message) => {
                            self.set_command_summary("Direct navigation completed.", false);
                            self.add_log(&message);
                            self.record_validation_step(
                                "TAB",
                                format!("Direct browser navigation completed: {}", message),
                            );
                        }
                        Err(e) => {
                            self.set_command_summary(
                                format!("Direct navigation failed: {}", e),
                                true,
                            );
                            self.add_log(&format!("Direct navigation failed: {}", e));
                        }
                    }
                }
                AppEvent::AirgapToggled {
                    next,
                    forced_local,
                    active_provider,
                    restored_online_provider,
                    snapshot,
                } => {
                    let previous_airgap = self.airgap.get_status();
                    self.airgap.set_status(next.clone());
                    if forced_local {
                        if previous_airgap == AirGapStatus::Online {
                            self.preferred_online_provider_config =
                                self.applied_provider_config.clone();
                        }
                        self.selected_provider = AiProvider::Local;
                        self.applied_provider = AiProvider::Local;
                        if self.local_model.trim().is_empty() {
                            self.local_model = "llama-3-8b".to_string();
                        }
                        self.applied_provider_config = ProviderConfig {
                            provider: AiProvider::Local,
                            gemini_key: self.gemini_key.clone(),
                            openai_key: self.openai_key.clone(),
                            anthropic_key: self.anthropic_key.clone(),
                            local_endpoint: self.local_endpoint.clone(),
                            local_model: self.local_model.clone(),
                        };
                        self.invalidate_provider_test("Air-gap forced local routing.");
                    }
                    if !forced_local {
                        self.applied_provider = active_provider;
                        if restored_online_provider {
                            self.selected_provider = self.preferred_online_provider_config.provider;
                            self.applied_provider_config =
                                self.preferred_online_provider_config.clone();
                        } else {
                            self.applied_provider_config = self.current_provider_config();
                        }
                    }
                    self.apply_refresh_snapshot(snapshot);
                    self.add_log(&format!("Air-gap changed to {:?}.", next));
                    let local_ready = self.provider_ready(AiProvider::Local);
                    if forced_local {
                        self.airgap_offline_exercised = true;
                        self.set_command_summary(
                            match local_ready {
                                true => format!(
                                    "Air-gap set to {:?}. Commands now require local inference.",
                                    next
                                ),
                                false => format!(
                                    "Air-gap set to {:?}. Local inference is required but not fully configured.",
                                    next
                                ),
                            },
                            !local_ready,
                        );
                        if local_ready {
                            self.set_ai_status(
                                "Network isolated. Local inference is now the active route.",
                                false,
                            );
                            self.add_log("Network isolated. Switched to local inference.");
                            self.record_validation_step(
                                "CONTROLS",
                                "Air-gap forced local inference successfully.",
                            );
                        } else {
                            self.set_ai_status(
                                "Network isolated, but local inference still needs a valid endpoint before commands can run.",
                                true,
                            );
                            self.add_log("Network isolated. Local inference is required, but the local endpoint is not ready.");
                        }
                    } else if restored_online_provider {
                        self.airgap_online_exercised = true;
                        self.set_ai_status(
                            format!(
                                "Air-gap returned online. Restored {} as the active provider.",
                                self.applied_provider.label()
                            ),
                            false,
                        );
                        self.set_command_summary(
                            format!(
                                "Air-gap set to {:?}. Restored {} provider routing.",
                                next,
                                self.applied_provider.label()
                            ),
                            false,
                        );
                        self.add_log(&format!(
                            "Air-gap returned online. Restored {} provider routing.",
                            self.applied_provider.label()
                        ));
                        self.record_validation_step(
                            "CONTROLS",
                            format!(
                                "Air-gap returned online and restored {} routing.",
                                self.applied_provider.label()
                            ),
                        );
                    } else {
                        self.airgap_online_exercised = true;
                        self.set_ai_status(
                            format!(
                                "Air-gap returned online. {} remains the active provider.",
                                self.applied_provider.label()
                            ),
                            false,
                        );
                        self.set_command_summary(
                            format!(
                                "Air-gap set to {:?}. {} remains the active provider.",
                                next,
                                self.applied_provider.label()
                            ),
                            false,
                        );
                        self.add_log(&format!(
                            "Air-gap returned online. {} remained active.",
                            self.applied_provider.label()
                        ));
                        self.record_validation_step(
                            "CONTROLS",
                            format!(
                                "Air-gap returned online with {} still active.",
                                self.applied_provider.label()
                            ),
                        );
                    }
                }
                AppEvent::PrivacyCycled { next, snapshot } => {
                    self.privacy_level = next.clone();
                    self.apply_refresh_snapshot(snapshot);
                    self.privacy_cycle_exercised = true;
                    self.add_log(&format!("Privacy changed to {:?}.", next));
                    self.set_command_summary(format!("Privacy set to {:?}.", next), false);
                    self.record_validation_step(
                        "CONTROLS",
                        format!("Privacy cycled to {:?}.", next),
                    );
                }
                AppEvent::WakeSearch(result) => match result {
                    Ok(results) => {
                        self.wake_search_exercised = true;
                        self.set_command_summary(
                            format!("Wake search returned {} result(s).", results.len()),
                            false,
                        );
                        self.wake_search_results = results;
                        self.record_validation_step(
                            "WAKE",
                            format!(
                                "Wake search returned {} result(s).",
                                self.wake_search_results.len()
                            ),
                        );
                    }
                    Err(e) => {
                        self.wake_search_results.clear();
                        self.set_command_summary(format!("Wake search failed: {}", e), true);
                        self.add_log(&format!("Wake search failed: {}", e));
                    }
                },
                AppEvent::WakeConsolidated(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(count) => {
                            self.wake_consolidate_exercised = true;
                            self.set_command_summary(
                                format!("Wake consolidated. Pruned {} entries.", count),
                                false,
                            );
                            self.add_log(&format!("Wake consolidated. Pruned {} entries.", count));
                            self.record_validation_step(
                                "WAKE",
                                format!("Wake consolidation pruned {} entries.", count),
                            );
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
                        self.provider_apply_exercised = true;
                        self.applied_provider = provider;
                        self.applied_provider_config = self.current_provider_config();
                        if self.airgap.get_status() == AirGapStatus::Online {
                            self.preferred_online_provider_config =
                                self.applied_provider_config.clone();
                        }
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
                        self.add_log(&format!(
                            "Applied {} as the active provider.",
                            provider.label()
                        ));
                        self.record_validation_step(
                            "PROVIDER",
                            format!("Applied {} as the active provider.", provider.label()),
                        );
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
                        self.provider_test_exercised = true;
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
                        self.add_log(&format!("{} provider test passed.", provider.label()));
                        self.record_validation_step(
                            "PROVIDER",
                            format!("{} provider test passed.", provider.label()),
                        );
                    }
                    Err(e) => {
                        self.last_tested_provider = Some(provider);
                        self.last_test_success = false;
                        self.last_test_message = e.clone();
                        self.set_ai_status(
                            format!("{} test failed: {}", provider.label(), e),
                            true,
                        );
                        self.set_command_summary(
                            format!("{} provider test failed.", provider.label()),
                            true,
                        );
                        self.add_log(&format!("{} provider test failed: {}", provider.label(), e));
                    }
                },
                AppEvent::ProviderSettingsSaved { provider, result } => match result {
                    Ok(_) => {
                        self.provider_save_exercised = true;
                        self.set_ai_status(
                            format!("Saved {} settings to Vault.", provider.label()),
                            false,
                        );
                        self.set_command_summary(
                            format!("Saved {} settings to Vault.", provider.label()),
                            false,
                        );
                        self.add_log(&format!(
                            "{} settings and credentials were stored in Vault.",
                            provider.label()
                        ));
                        self.record_validation_step(
                            "PROVIDER",
                            format!("Saved {} settings to Vault.", provider.label()),
                        );
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
                        self.provider_load_exercised = true;
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
                        self.invalidate_provider_test("Provider settings loaded from Vault.");
                        let loaded_provider_label = provider
                            .map(|provider| provider.label().to_string())
                            .unwrap_or_else(|| "stored".to_string());
                        self.set_ai_status(
                            format!(
                                "Loaded {} settings into the panel. Apply to make the selected provider active.",
                                loaded_provider_label
                            ),
                            false,
                        );
                        self.set_command_summary(
                            format!(
                                "Loaded {} settings into the panel. Apply to activate them.",
                                loaded_provider_label
                            ),
                            false,
                        );
                        self.add_log(&format!(
                            "Loaded {} settings from Vault into the panel.",
                            loaded_provider_label
                        ));
                        self.record_validation_step(
                            "PROVIDER",
                            format!(
                                "Loaded {} settings from Vault into the panel.",
                                loaded_provider_label
                            ),
                        );
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
                            self.consent_authorized_exercised = true;
                            self.set_command_summary(
                                "Consent authorized. Pilot resumed the plan.",
                                false,
                            );
                            self.add_log(&format!("Consent authorized: {}", message));
                            self.record_validation_step(
                                "CONSENT",
                                "Captain's Key authorized and plan resumed.",
                            );
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
                            self.consent_denied_exercised = true;
                            self.set_command_summary("Consent denied. Plan aborted.", false);
                            self.add_log(&format!("Consent denied: {}", message));
                            self.record_validation_step(
                                "CONSENT",
                                "Captain's Key request denied and plan aborted.",
                            );
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Consent deny failed: {}", e), true);
                            self.add_log(&format!("Consent deny failed: {}", e));
                        }
                    }
                }
                AppEvent::TabCreated(tab_id, snapshot) => {
                    let opened_tab_label = snapshot
                        .tabs
                        .iter()
                        .find(|tab| tab.id == tab_id)
                        .and_then(|tab| tab.url.as_ref().map(|url| url.to_string()))
                        .unwrap_or_else(|| "about:blank".to_string());
                    self.apply_refresh_snapshot(snapshot);
                    self.tab_open_exercised = true;
                    self.set_command_summary("Opened a new tab.", false);
                    self.add_log(&format!("Opened tab {}.", opened_tab_label));
                    self.record_validation_step("TAB", format!("Opened tab {}.", opened_tab_label));
                }
                AppEvent::TabSwitched(tab_id, result, snapshot) => {
                    let tab_label = snapshot
                        .tabs
                        .iter()
                        .find(|tab| tab.id == tab_id)
                        .and_then(|tab| tab.url.as_ref().map(|url| url.to_string()))
                        .unwrap_or_else(|| self.tab_label(tab_id));
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(_) => {
                            self.tab_switch_exercised = true;
                            self.set_command_summary("Switched active tab.", false);
                            self.add_log(&format!("Switched to tab {}.", tab_label));
                            self.record_validation_step(
                                "TAB",
                                format!("Switched to tab {}.", tab_label),
                            );
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Failed to switch tab: {}", e), true);
                            self.add_log(&format!("Failed to switch tab: {}", e));
                        }
                    }
                }
                AppEvent::ActiveTabClosed(tab_id, result, snapshot) => {
                    let tab_label = self.tab_label(tab_id);
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(_) => {
                            self.tab_close_exercised = true;
                            self.set_command_summary("Closed active tab.", false);
                            self.add_log(&format!("Closed tab {}.", tab_label));
                            self.record_validation_step(
                                "TAB",
                                format!("Closed tab {}.", tab_label),
                            );
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Failed to close tab: {}", e), true);
                            self.add_log(&format!("Failed to close tab: {}", e));
                        }
                    }
                }
                AppEvent::ActiveTabReloaded(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(message) => {
                            self.set_command_summary("Reloaded active tab.", false);
                            self.add_log(&message);
                            self.record_validation_step(
                                "TAB",
                                format!("Reloaded active tab: {}", message),
                            );
                        }
                        Err(e) => {
                            self.set_command_summary(format!("Reload failed: {}", e), true);
                            self.add_log(&format!("Reload failed: {}", e));
                        }
                    }
                }
                AppEvent::ActiveTabWentBack(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(message) => {
                            self.set_command_summary("Moved back in active tab history.", false);
                            self.add_log(&message);
                            self.record_validation_step(
                                "TAB",
                                format!("Moved back in active tab history: {}", message),
                            );
                        }
                        Err(e) => {
                            self.set_command_summary(
                                format!("Back navigation failed: {}", e),
                                true,
                            );
                            self.add_log(&format!("Back navigation failed: {}", e));
                        }
                    }
                }
                AppEvent::ActiveTabWentForward(result, snapshot) => {
                    self.apply_refresh_snapshot(snapshot);
                    match result {
                        Ok(message) => {
                            self.set_command_summary("Moved forward in active tab history.", false);
                            self.add_log(&message);
                            self.record_validation_step(
                                "TAB",
                                format!("Moved forward in active tab history: {}", message),
                            );
                        }
                        Err(e) => {
                            self.set_command_summary(
                                format!("Forward navigation failed: {}", e),
                                true,
                            );
                            self.add_log(&format!("Forward navigation failed: {}", e));
                        }
                    }
                }
            }
        }
        if processed > 0 {
            if self.pending_async_jobs == 0 {
                self.lifecycle_log(format!(
                    "Async queue drained after processing {} event(s).",
                    processed
                ));
            } else {
                self.add_log(&format!(
                    "Processed {} async event(s). {} job(s) still in flight.",
                    processed, self.pending_async_jobs
                ));
            }
        }
    }

    pub fn set_ai_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.ai_status_message = message.into();
        self.ai_status_is_error = is_error;
    }

    pub fn intent_route_message(&self) -> String {
        let mut msg = if self.airgap.get_status() != AirGapStatus::Online {
            if self.provider_ready(AiProvider::Local) {
                format!(
                    "COMMAND is constrained by {:?} air-gap and will use local inference.",
                    self.airgap.get_status()
                )
            } else {
                format!(
                    "COMMAND is constrained by {:?} air-gap, but local inference is not ready yet.",
                    self.airgap.get_status()
                )
            }
        } else {
            format!(
                "COMMAND will use active provider {}.",
                self.applied_provider.label()
            )
        };
        if self.has_unapplied_provider_changes() {
            msg.push_str(&format!(
                " Settings panel has unapplied changes; apply before expecting the route to change."
            ));
        } else if self.selected_provider != self.applied_provider {
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

    pub fn runtime_status_message(&self) -> String {
        if !self.startup_allows_interaction() {
            return format!(
                "RUNTIME startup phase {:?}: {}",
                self.startup_phase, self.startup_detail
            );
        }
        match &self.pilot_status {
            PilotStatus::AwaitingConsent(message) => {
                format!("RUNTIME waiting for Captain's Key: {}", message)
            }
            PilotStatus::Reasoning => {
                "RUNTIME pilot is reasoning about the current intent.".to_string()
            }
            PilotStatus::Navigating(url) => format!("RUNTIME pilot is navigating to {}.", url),
            PilotStatus::Distilling => {
                "RUNTIME engine is distilling the active page into the Digital Wake.".to_string()
            }
            PilotStatus::ExecutingAction(message) => {
                format!("RUNTIME pilot is executing: {}", message)
            }
            PilotStatus::ReasoningComplete(_) if self.pending_async_jobs > 0 => {
                "RUNTIME plan is staged and async work is still settling.".to_string()
            }
            _ if self.pending_async_jobs > 0 => {
                format!(
                    "RUNTIME {} async job(s) in flight.",
                    self.pending_async_jobs
                )
            }
            _ => "RUNTIME idle and ready for the next command.".to_string(),
        }
    }

    pub fn startup_status_line(&self) -> String {
        let phase = match self.startup_phase {
            StartupPhase::Booting => "BOOTING",
            StartupPhase::WarmingUp => "WARMING UP",
            StartupPhase::Ready => "READY",
            StartupPhase::Degraded => "DEGRADED",
        };

        if self.startup_alerts.is_empty() {
            format!("STARTUP {} {}", phase, self.startup_detail)
        } else if self.startup_alerts.len() == 1 {
            format!(
                "STARTUP {} {} ALERT {}",
                phase, self.startup_detail, self.startup_alerts[0]
            )
        } else {
            format!(
                "STARTUP {} {} ALERTS {} issues",
                phase,
                self.startup_detail,
                self.startup_alerts.len()
            )
        }
    }

    pub fn provider_next_step_message(&self) -> String {
        if self.airgap.get_status() != AirGapStatus::Online
            && !self.provider_ready(AiProvider::Local)
        {
            return "NEXT STEP configure the local endpoint before testing commands in offline mode."
                .to_string();
        }
        if self.has_unapplied_provider_changes() {
            return format!(
                "NEXT STEP apply the {} settings currently shown in the panel.",
                self.selected_provider.label()
            );
        }
        if self.last_tested_provider != Some(self.applied_provider) {
            return format!(
                "NEXT STEP test the active {} provider with the current settings.",
                self.applied_provider.label()
            );
        }
        if !self.last_test_success {
            return format!(
                "NEXT STEP fix the {} provider settings, then test again.",
                self.applied_provider.label()
            );
        }
        "NEXT STEP provider routing looks ready for a real command.".to_string()
    }

    pub fn consent_next_step_message(&self) -> String {
        match &self.pilot_status {
            PilotStatus::AwaitingConsent(_) => {
                "NEXT STEP review the request, then AUTHORIZE to resume or DENY to abort."
                    .to_string()
            }
            _ => "NEXT STEP run a protected intent when you want to exercise Captain's Key flow."
                .to_string(),
        }
    }
}
