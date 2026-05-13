use sextant_airgap::AirGapStatus;
use sextant_engine::{DistilledPage, EngineStatus, SextantEngine, Tab};
use sextant_inference::InferenceBackend;
use sextant_log::{CaptainsLog, LogEntry};
use sextant_pilot::{
    AnthropicBrain, GeminiBrain, LocalBrain, OpenAIBrain, PilotBrain, PilotStatus, SextantPilot,
};
use sextant_privacy::PrivacyLevel;
use sextant_vault::CitadelVault;
use sextant_wake::{DigitalWake, WakeEntry};
use std::sync::{mpsc, Arc};
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AiProvider {
    Gemini,
    OpenAI,
    Anthropic,
    Local,
}

impl AiProvider {
    pub fn label(self) -> &'static str {
        match self {
            AiProvider::Gemini => "Gemini",
            AiProvider::OpenAI => "OpenAI",
            AiProvider::Anthropic => "Anthropic",
            AiProvider::Local => "Local",
        }
    }

    pub fn from_secret(value: &str) -> Self {
        match value {
            "openai" => AiProvider::OpenAI,
            "anthropic" => AiProvider::Anthropic,
            "local" => AiProvider::Local,
            _ => AiProvider::Gemini,
        }
    }

    pub fn as_secret(self) -> &'static str {
        match self {
            AiProvider::Gemini => "gemini",
            AiProvider::OpenAI => "openai",
            AiProvider::Anthropic => "anthropic",
            AiProvider::Local => "local",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupPhase {
    Booting,
    WarmingUp,
    Ready,
    Degraded,
}

#[derive(Clone)]
pub struct PilotSnapshot {
    pub status: PilotStatus,
    pub brain_name: String,
}

#[derive(Clone)]
pub struct RefreshSnapshot {
    pub tabs: Vec<Tab>,
    pub active_tab_id: Option<Uuid>,
    pub active_page: Option<DistilledPage>,
    pub engine_status: Option<EngineStatus>,
    pub recent_memory: Vec<WakeEntry>,
    pub audit_trail: Vec<LogEntry>,
    pub pilot: PilotSnapshot,
}

pub enum AppCommand {
    BootstrapRuntime,
    ProcessIntent(String),
    NavigateDirect(Url),
    ToggleAirgap,
    CyclePrivacy,
    SearchWake(String),
    ConsolidateWake,
    ApplyProviderSettings,
    TestProviderSettings,
    SaveProviderSettings,
    LoadProviderSettings,
    Authorize(String),
    DenyConsent,
    CreateTab,
    SwitchTab(Uuid),
    CloseActiveTab,
    ReloadActiveTab,
    GoBackActiveTab,
    GoForwardActiveTab,
}

impl AppCommand {
    pub fn label(&self) -> &'static str {
        match self {
            AppCommand::BootstrapRuntime => "bootstrap runtime",
            AppCommand::ProcessIntent(_) => "process intent",
            AppCommand::NavigateDirect(_) => "direct navigation",
            AppCommand::ToggleAirgap => "toggle air-gap",
            AppCommand::CyclePrivacy => "cycle privacy",
            AppCommand::SearchWake(_) => "search Wake",
            AppCommand::ConsolidateWake => "consolidate Wake",
            AppCommand::ApplyProviderSettings => "apply provider settings",
            AppCommand::TestProviderSettings => "test provider settings",
            AppCommand::SaveProviderSettings => "save provider settings",
            AppCommand::LoadProviderSettings => "load provider settings",
            AppCommand::Authorize(_) => "authorize consent",
            AppCommand::DenyConsent => "deny consent",
            AppCommand::CreateTab => "create tab",
            AppCommand::SwitchTab(_) => "switch tab",
            AppCommand::CloseActiveTab => "close active tab",
            AppCommand::ReloadActiveTab => "reload active tab",
            AppCommand::GoBackActiveTab => "back navigation",
            AppCommand::GoForwardActiveTab => "forward navigation",
        }
    }
}

pub enum AppEvent {
    RuntimeBootstrapped {
        phase: StartupPhase,
        alerts: Vec<String>,
        notes: Vec<String>,
        snapshot: RefreshSnapshot,
    },
    IntentProcessed(Result<String, String>, RefreshSnapshot),
    DirectNavigation(Result<String, String>, RefreshSnapshot),
    AirgapToggled {
        next: AirGapStatus,
        forced_local: bool,
        active_provider: AiProvider,
        restored_online_provider: bool,
        snapshot: RefreshSnapshot,
    },
    PrivacyCycled {
        next: PrivacyLevel,
        snapshot: RefreshSnapshot,
    },
    WakeSearch(Result<Vec<WakeEntry>, String>),
    WakeConsolidated(Result<usize, String>, RefreshSnapshot),
    ProviderSettingsApplied {
        provider: AiProvider,
        brain_name: Option<String>,
    },
    ProviderSettingsTested {
        provider: AiProvider,
        result: Result<String, String>,
    },
    ProviderSettingsSaved {
        provider: AiProvider,
        result: Result<(), String>,
    },
    ProviderSettingsLoaded {
        provider: Option<AiProvider>,
        gemini_key: Option<String>,
        openai_key: Option<String>,
        anthropic_key: Option<String>,
        local_endpoint: Option<String>,
        local_model: Option<String>,
        result: Result<(), String>,
    },
    ConsentAuthorized(Result<String, String>, RefreshSnapshot),
    ConsentDenied(Result<String, String>, RefreshSnapshot),
    TabCreated(Uuid, RefreshSnapshot),
    TabSwitched(Uuid, Result<(), String>, RefreshSnapshot),
    ActiveTabClosed(Uuid, Result<(), String>, RefreshSnapshot),
    ActiveTabReloaded(Result<String, String>, RefreshSnapshot),
    ActiveTabWentBack(Result<String, String>, RefreshSnapshot),
    ActiveTabWentForward(Result<String, String>, RefreshSnapshot),
}

#[derive(Clone)]
pub struct AppServices {
    pub vault: Arc<Mutex<CitadelVault>>,
    pub pilot: Arc<Mutex<SextantPilot>>,
    pub engine: Arc<Mutex<SextantEngine>>,
    pub wake: Arc<Mutex<DigitalWake>>,
    pub log: Arc<Mutex<CaptainsLog>>,
}

#[derive(Clone)]
pub struct AppSession {
    pub persona_id: String,
    pub current_airgap: AirGapStatus,
    pub current_privacy: PrivacyLevel,
    pub active_tab_id: Option<Uuid>,
    pub preferred_online_provider_config: ProviderConfig,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub provider: AiProvider,
    pub gemini_key: String,
    pub openai_key: String,
    pub anthropic_key: String,
    pub local_endpoint: String,
    pub local_model: String,
}

pub fn parse_direct_url_candidate(candidate: &str) -> Option<Url> {
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

pub fn build_brain(config: &ProviderConfig) -> Option<Box<dyn PilotBrain>> {
    match config.provider {
        AiProvider::Gemini => Some(Box::new(GeminiBrain::new(
            config.gemini_key.clone(),
            "gemini-1.5-pro",
        ))),
        AiProvider::OpenAI => Some(Box::new(OpenAIBrain::new(
            config.openai_key.clone(),
            "gpt-4o",
        ))),
        AiProvider::Anthropic => Some(Box::new(AnthropicBrain::new(
            config.anthropic_key.clone(),
            "claude-3-5-sonnet-20240620",
        ))),
        AiProvider::Local => Url::parse(&config.local_endpoint).ok().map(|endpoint| {
            Box::new(LocalBrain::new(
                InferenceBackend::LlamaCpp,
                endpoint,
                &config.local_model,
            )) as Box<dyn PilotBrain>
        }),
    }
}

pub async fn collect_refresh_snapshot(
    services: &AppServices,
    persona_id: String,
) -> RefreshSnapshot {
    let (tabs, active_tab_id, active_page, engine_status) = {
        let engine = services.engine.lock().await;
        let tabs = engine.get_tabs();
        let active_tab = engine.get_active_tab().cloned();
        (
            tabs,
            active_tab.as_ref().map(|t| t.id),
            active_tab.as_ref().and_then(|t| t.distilled_page.clone()),
            active_tab.map(|t| t.status),
        )
    };

    let recent_memory = {
        let wake = services.wake.lock().await;
        wake.search(&persona_id, "").unwrap_or_default()
    };

    let audit_trail = {
        let log = services.log.lock().await;
        log.get_entries(&persona_id, 10).unwrap_or_default()
    };

    let pilot = services.pilot.lock().await;
    let pilot_snapshot = PilotSnapshot {
        status: pilot.status(),
        brain_name: pilot.brain_name().to_string(),
    };
    drop(pilot);

    RefreshSnapshot {
        tabs,
        active_tab_id,
        active_page,
        engine_status,
        recent_memory,
        audit_trail,
        pilot: pilot_snapshot,
    }
}

pub async fn run_command(
    services: &AppServices,
    session: &AppSession,
    provider_config: &ProviderConfig,
    command: AppCommand,
) -> AppEvent {
    match command {
        AppCommand::BootstrapRuntime => {
            let alerts = Vec::new();
            let mut notes = Vec::new();
            notes.push("Bootstrap started.".to_string());
            notes.push(
                "Stability mode defers engine warm-up until the first explicit browser action."
                    .to_string(),
            );
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            let phase = if alerts.is_empty() {
                StartupPhase::Ready
            } else {
                StartupPhase::Degraded
            };
            AppEvent::RuntimeBootstrapped {
                phase,
                alerts,
                notes,
                snapshot,
            }
        }
        AppCommand::ProcessIntent(intent) => {
            let result = {
                let mut pilot = services.pilot.lock().await;
                pilot.process_intent(&intent).await
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::IntentProcessed(result, snapshot)
        }
        AppCommand::NavigateDirect(url) => {
            let result: Result<String, String> = {
                let mut engine = services.engine.lock().await;
                if engine.get_active_tab().is_none() {
                    engine.open_tab();
                }
                match engine.navigate_with_fallback(url.clone(), &session.persona_id) {
                    Ok(_) => match engine.distill_current_page() {
                        Ok(page) => Ok(format!("Navigated directly to {}.", page.url)),
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(e.to_string()),
                }
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::DirectNavigation(result, snapshot)
        }
        AppCommand::ToggleAirgap => {
            let next = match session.current_airgap {
                AirGapStatus::Online => AirGapStatus::Isolated,
                AirGapStatus::Isolated => AirGapStatus::Hardened,
                AirGapStatus::Hardened => AirGapStatus::Online,
            };
            let forced_local = next != AirGapStatus::Online;
            let mut active_provider = provider_config.provider;
            let mut restored_online_provider = false;
            if forced_local {
                let local_config = ProviderConfig {
                    provider: AiProvider::Local,
                    ..provider_config.clone()
                };
                active_provider = AiProvider::Local;
                if let Some(brain) = build_brain(&local_config) {
                    let mut pilot = services.pilot.lock().await;
                    pilot.switch_brain(brain);
                }
            } else {
                let preferred_online = session.preferred_online_provider_config.clone();
                if let Some(brain) = build_brain(&preferred_online) {
                    let mut pilot = services.pilot.lock().await;
                    pilot.switch_brain(brain);
                    active_provider = preferred_online.provider;
                    restored_online_provider = true;
                }
            }
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::AirgapToggled {
                next,
                forced_local,
                active_provider,
                restored_online_provider,
                snapshot,
            }
        }
        AppCommand::CyclePrivacy => {
            let next = match session.current_privacy {
                PrivacyLevel::None => PrivacyLevel::Standard,
                PrivacyLevel::Standard => PrivacyLevel::Strict,
                PrivacyLevel::Strict => PrivacyLevel::None,
            };
            {
                let engine = services.engine.lock().await;
                engine.set_privacy_level(next.clone());
            }
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::PrivacyCycled { next, snapshot }
        }
        AppCommand::SearchWake(query) => {
            let results = {
                let wake = services.wake.lock().await;
                wake.search(&session.persona_id, &query)
                    .map_err(|e| e.to_string())
            };
            AppEvent::WakeSearch(results)
        }
        AppCommand::ConsolidateWake => {
            let result = {
                let wake = services.wake.lock().await;
                wake.consolidate(&session.persona_id)
                    .map_err(|e| e.to_string())
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::WakeConsolidated(result, snapshot)
        }
        AppCommand::ApplyProviderSettings => {
            let brain = build_brain(provider_config);
            match brain {
                Some(brain) => {
                    let brain_name = {
                        let mut pilot = services.pilot.lock().await;
                        pilot.switch_brain(brain);
                        Some(pilot.brain_name().to_string())
                    };
                    AppEvent::ProviderSettingsApplied {
                        provider: provider_config.provider,
                        brain_name,
                    }
                }
                None => AppEvent::ProviderSettingsApplied {
                    provider: provider_config.provider,
                    brain_name: None,
                },
            }
        }
        AppCommand::TestProviderSettings => {
            let result = if let Some(brain) = build_brain(provider_config) {
                match brain.reason("Reply with exactly OK.", &[]).await {
                    Ok(_) => Ok("OK".to_string()),
                    Err(e) => Err(e),
                }
            } else {
                Err(format!(
                    "{} configuration is incomplete.",
                    provider_config.provider.label()
                ))
            };
            AppEvent::ProviderSettingsTested {
                provider: provider_config.provider,
                result,
            }
        }
        AppCommand::SaveProviderSettings => {
            let result = {
                let mut vault = services.vault.lock().await;
                vault
                    .store_secret("ai_provider", provider_config.provider.as_secret())
                    .and_then(|_| vault.store_secret("gemini_key", &provider_config.gemini_key))
                    .and_then(|_| vault.store_secret("openai_key", &provider_config.openai_key))
                    .and_then(|_| {
                        vault.store_secret("anthropic_key", &provider_config.anthropic_key)
                    })
                    .and_then(|_| {
                        vault.store_secret("local_endpoint", &provider_config.local_endpoint)
                    })
                    .and_then(|_| vault.store_secret("local_model", &provider_config.local_model))
            };
            AppEvent::ProviderSettingsSaved {
                provider: provider_config.provider,
                result,
            }
        }
        AppCommand::LoadProviderSettings => {
            let vault = services.vault.lock().await;
            let provider = vault
                .get_secret("ai_provider")
                .ok()
                .map(|value| AiProvider::from_secret(&value));
            let gemini_key = vault.get_secret("gemini_key").ok();
            let openai_key = vault.get_secret("openai_key").ok();
            let anthropic_key = vault.get_secret("anthropic_key").ok();
            let local_endpoint = vault.get_secret("local_endpoint").ok();
            let local_model = vault.get_secret("local_model").ok();
            AppEvent::ProviderSettingsLoaded {
                provider,
                gemini_key,
                openai_key,
                anthropic_key,
                local_endpoint,
                local_model,
                result: Ok(()),
            }
        }
        AppCommand::Authorize(message) => {
            let result = {
                let signature = {
                    let vault = services.vault.lock().await;
                    vault.sign_consent(&message)
                };
                match signature {
                    Ok(signature) => {
                        let mut pilot = services.pilot.lock().await;
                        pilot.provide_consent(&signature).await
                    }
                    Err(e) => Err(e),
                }
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::ConsentAuthorized(result, snapshot)
        }
        AppCommand::DenyConsent => {
            let result = {
                let mut pilot = services.pilot.lock().await;
                pilot.deny_consent().await
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::ConsentDenied(result, snapshot)
        }
        AppCommand::CreateTab => {
            let tab_id = {
                let mut engine = services.engine.lock().await;
                let tab_id = engine.open_tab();
                let _ = engine.switch_to_tab(tab_id);
                tab_id
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::TabCreated(tab_id, snapshot)
        }
        AppCommand::SwitchTab(tab_id) => {
            let result = {
                let mut engine = services.engine.lock().await;
                engine.switch_to_tab(tab_id).map_err(|e| e.to_string())
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::TabSwitched(tab_id, result.map(|_| ()), snapshot)
        }
        AppCommand::CloseActiveTab => {
            let tab_id = session.active_tab_id.unwrap_or_else(Uuid::nil);
            let result = if session.active_tab_id.is_some() {
                let mut engine = services.engine.lock().await;
                engine.close_tab(&tab_id).map_err(|e| e.to_string())
            } else {
                Err("No active tab to close.".to_string())
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::ActiveTabClosed(tab_id, result.map(|_| ()), snapshot)
        }
        AppCommand::ReloadActiveTab => {
            let result: Result<String, String> = {
                let mut engine = services.engine.lock().await;
                match engine.reload_active_tab() {
                    Ok(_) => match engine.distill_current_page() {
                        Ok(page) => Ok(format!("Reloaded {}.", page.url)),
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(e.to_string()),
                }
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::ActiveTabReloaded(result, snapshot)
        }
        AppCommand::GoBackActiveTab => {
            let result: Result<String, String> = {
                let mut engine = services.engine.lock().await;
                match engine.go_back_active_tab() {
                    Ok(_) => match engine.distill_current_page() {
                        Ok(page) => Ok(format!("Moved back to {}.", page.url)),
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(e.to_string()),
                }
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::ActiveTabWentBack(result, snapshot)
        }
        AppCommand::GoForwardActiveTab => {
            let result: Result<String, String> = {
                let mut engine = services.engine.lock().await;
                match engine.go_forward_active_tab() {
                    Ok(_) => match engine.distill_current_page() {
                        Ok(page) => Ok(format!("Moved forward to {}.", page.url)),
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(e.to_string()),
                }
            };
            let snapshot = collect_refresh_snapshot(services, session.persona_id.clone()).await;
            AppEvent::ActiveTabWentForward(result, snapshot)
        }
    }
}

pub fn spawn_command(
    runtime: &Handle,
    result_tx: mpsc::Sender<AppEvent>,
    services: AppServices,
    session: AppSession,
    provider_config: ProviderConfig,
    command: AppCommand,
) {
    runtime.spawn(async move {
        let event = run_command(&services, &session, &provider_config, command).await;
        let _ = result_tx.send(event);
    });
}
