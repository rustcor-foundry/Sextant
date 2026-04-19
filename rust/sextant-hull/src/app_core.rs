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
    ProcessIntent(String),
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
}

pub enum AppEvent {
    IntentProcessed(Result<String, String>, RefreshSnapshot),
    AirgapToggled {
        next: AirGapStatus,
        forced_local: bool,
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
    ProviderSettingsSaved(Result<(), String>),
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
}

#[derive(Clone)]
pub struct ProviderConfig {
    pub provider: AiProvider,
    pub gemini_key: String,
    pub openai_key: String,
    pub anthropic_key: String,
    pub local_endpoint: String,
    pub local_model: String,
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

pub fn spawn_command(
    runtime: &Handle,
    result_tx: mpsc::Sender<AppEvent>,
    services: AppServices,
    session: AppSession,
    provider_config: ProviderConfig,
    command: AppCommand,
) {
    runtime.spawn(async move {
        let send_event = |event| {
            let _ = result_tx.send(event);
        };

        match command {
            AppCommand::ProcessIntent(intent) => {
                let result = {
                    let mut pilot = services.pilot.lock().await;
                    pilot.process_intent(&intent).await
                };
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::IntentProcessed(result, snapshot));
            }
            AppCommand::ToggleAirgap => {
                let next = match session.current_airgap {
                    AirGapStatus::Online => AirGapStatus::Isolated,
                    AirGapStatus::Isolated => AirGapStatus::Hardened,
                    AirGapStatus::Hardened => AirGapStatus::Online,
                };
                let forced_local = next != AirGapStatus::Online;
                if forced_local {
                    let local_config = ProviderConfig {
                        provider: AiProvider::Local,
                        ..provider_config
                    };
                    if let Some(brain) = build_brain(&local_config) {
                        let mut pilot = services.pilot.lock().await;
                        pilot.switch_brain(brain);
                    }
                }
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::AirgapToggled {
                    next,
                    forced_local,
                    snapshot,
                });
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
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::PrivacyCycled { next, snapshot });
            }
            AppCommand::SearchWake(query) => {
                let results = {
                    let wake = services.wake.lock().await;
                    wake.search(&session.persona_id, &query)
                        .map_err(|e| e.to_string())
                };
                send_event(AppEvent::WakeSearch(results));
            }
            AppCommand::ConsolidateWake => {
                let result = {
                    let wake = services.wake.lock().await;
                    wake.consolidate(&session.persona_id)
                        .map_err(|e| e.to_string())
                };
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::WakeConsolidated(result, snapshot));
            }
            AppCommand::ApplyProviderSettings => {
                let brain = build_brain(&provider_config);
                match brain {
                    Some(brain) => {
                        let brain_name = {
                            let mut pilot = services.pilot.lock().await;
                            pilot.switch_brain(brain);
                            Some(pilot.brain_name().to_string())
                        };
                        send_event(AppEvent::ProviderSettingsApplied {
                            provider: provider_config.provider,
                            brain_name,
                        });
                    }
                    None => {
                        send_event(AppEvent::ProviderSettingsApplied {
                            provider: provider_config.provider,
                            brain_name: None,
                        });
                    }
                }
            }
            AppCommand::TestProviderSettings => {
                let result = if let Some(brain) = build_brain(&provider_config) {
                    let result = match brain.reason("Reply with exactly OK.", &[]).await {
                        Ok(_) => Ok("OK".to_string()),
                        Err(e) => Err(e),
                    };
                    result
                } else {
                    Err(format!(
                        "{} configuration is incomplete.",
                        provider_config.provider.label()
                    ))
                };
                send_event(AppEvent::ProviderSettingsTested {
                    provider: provider_config.provider,
                    result,
                });
            }
            AppCommand::SaveProviderSettings => {
                let result = {
                    let mut vault = services.vault.lock().await;
                    vault.store_secret("ai_provider", provider_config.provider.as_secret())
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
                send_event(AppEvent::ProviderSettingsSaved(result));
            }
            AppCommand::LoadProviderSettings => {
                let result = {
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
                };
                send_event(result);
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
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::ConsentAuthorized(result, snapshot));
            }
            AppCommand::DenyConsent => {
                let result = {
                    let mut pilot = services.pilot.lock().await;
                    pilot.deny_consent().await
                };
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::ConsentDenied(result, snapshot));
            }
            AppCommand::CreateTab => {
                let tab_id = {
                    let mut engine = services.engine.lock().await;
                    let tab_id = engine.open_tab();
                    let _ = engine.switch_to_tab(tab_id);
                    tab_id
                };
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::TabCreated(tab_id, snapshot));
            }
            AppCommand::SwitchTab(tab_id) => {
                let result = {
                    let mut engine = services.engine.lock().await;
                    engine.switch_to_tab(tab_id).map_err(|e| e.to_string())
                };
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::TabSwitched(tab_id, result.map(|_| ()), snapshot));
            }
            AppCommand::CloseActiveTab => {
                let tab_id = session.active_tab_id.unwrap_or_else(Uuid::nil);
                let result = if session.active_tab_id.is_some() {
                    let mut engine = services.engine.lock().await;
                    engine.close_tab(&tab_id).map_err(|e| e.to_string())
                } else {
                    Err("No active tab to close.".to_string())
                };
                let snapshot = collect_refresh_snapshot(&services, session.persona_id).await;
                send_event(AppEvent::ActiveTabClosed(
                    tab_id,
                    result.map(|_| ()),
                    snapshot,
                ));
            }
        }
    });
}
