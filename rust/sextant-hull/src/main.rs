mod app_core;
mod deferred;
mod poller;
mod state;
mod util;
mod views;

use app_core::AiProvider;
use deferred::DeferredState;
use sextant_airgap::SextantAirGap;
use sextant_log::CaptainsLog;
use sextant_pilot::{AnthropicBrain, GeminiBrain, OpenAIBrain, SextantPilot};
use sextant_privacy::PrivacyLevel;
use sextant_vault::{CitadelVault, KeyType};
use sextant_wake::DigitalWake;
use state::SextantState;
use std::fs;
use std::sync::{mpsc, Arc};
use tokio::sync::Mutex;
use util::{app_data_dir, env_key};
use views::app_logic_native;
use xilem::Xilem;

fn main() {
    tracing_subscriber::fmt::init();

    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    let state = rt.block_on(async {
        let data_dir = app_data_dir();
        fs::create_dir_all(&data_dir).expect("Failed to create Sextant data directory");
        let mut startup_alerts = Vec::new();

        let vault = Arc::new(Mutex::new(CitadelVault::new()));
        let engine = Arc::new(Mutex::new(sextant_engine::SextantEngine::new()));

        {
            let mut engine_guard = engine.lock().await;
            if let Err(e) = engine_guard.initialize_gpu().await {
                startup_alerts.push(format!("GPU initialization failed: {}", e));
                eprintln!("Failed to initialize GPU: {}", e);
            }
        }

        let wake = Arc::new(Mutex::new(
            DigitalWake::open(data_dir.join("wake.db")).expect("Failed to open Wake"),
        ));
        let log_store = Arc::new(Mutex::new(
            CaptainsLog::new(data_dir.join("captains-log.db")).expect("Failed to open Log"),
        ));

        let gemini_key = env_key("GEMINI_API_KEY");
        let openai_key = env_key("OPENAI_API_KEY");
        let anthropic_key = env_key("ANTHROPIC_API_KEY");
        let selected_provider = if !gemini_key.is_empty() {
            AiProvider::Gemini
        } else if !openai_key.is_empty() {
            AiProvider::OpenAI
        } else if !anthropic_key.is_empty() {
            AiProvider::Anthropic
        } else {
            AiProvider::Gemini
        };

        let brain: Box<dyn sextant_pilot::PilotBrain> = if !gemini_key.is_empty() {
            Box::new(GeminiBrain::new(gemini_key.clone(), "gemini-1.5-pro"))
        } else if !openai_key.is_empty() {
            Box::new(OpenAIBrain::new(openai_key.clone(), "gpt-4o"))
        } else if !anthropic_key.is_empty() {
            Box::new(AnthropicBrain::new(
                anthropic_key.clone(),
                "claude-3-5-sonnet-20240620",
            ))
        } else {
            Box::new(GeminiBrain::new(String::new(), "gemini-1.5-pro"))
        };

        let mut pilot =
            SextantPilot::new(vault.clone(), engine.clone(), wake.clone(), log_store.clone(), brain);

        let mut vault_guard = vault.lock().await;
        vault_guard
            .initialize_new("password123")
            .expect("Failed to init vault");
        let persona_personal = vault_guard
            .create_persona("Personal", "Primary browsing persona")
            .expect("Failed to create persona");
        let _persona_work = vault_guard
            .create_persona("Work", "Professional persona")
            .expect("Failed to create work persona");
        let identity = vault_guard
            .derive_identity(
                &persona_personal.id,
                "Main Pilot",
                KeyType::Ed25519,
                "m/44'/0'/0'/0/0",
            )
            .expect("Failed to derive identity");
        let available_personas = vault_guard.get_personas();
        drop(vault_guard);

        pilot.set_persona(Some(persona_personal.id.clone()));
        let pilot_status = pilot.status();
        let pilot_brain_name = pilot.brain_name().to_string();
        let pilot = Arc::new(Mutex::new(pilot));
        let (async_result_tx, async_result_rx) = mpsc::channel();

        let mut state = SextantState {
            runtime: rt.handle().clone(),
            vault,
            engine,
            wake,
            log: log_store,
            pilot,
            intent: String::new(),
            pilot_status,
            pilot_brain_name,
            engine_status: None,
            active_page: None,
            recent_memory: Vec::new(),
            audit_trail: Vec::new(),
            logs: vec!["Hull initialized.".to_string(), "Pilot online.".to_string()],
            startup_alerts: Vec::new(),
            validation_successes: Vec::new(),
            consent_request_exercised: false,
            consent_authorized_exercised: false,
            consent_denied_exercised: false,
            provider_load_exercised: false,
            provider_apply_exercised: false,
            provider_test_exercised: false,
            provider_save_exercised: false,
            tab_open_exercised: false,
            tab_switch_exercised: false,
            tab_close_exercised: false,
            wake_search_exercised: false,
            wake_consolidate_exercised: false,
            airgap_offline_exercised: false,
            airgap_online_exercised: false,
            privacy_cycle_exercised: false,
            audit_entry_exercised: false,
            audit_baseline_timestamp: None,
            last_command_summary: "Awaiting first command.".to_string(),
            last_command_is_error: false,
            is_vault_unlocked: true,
            active_persona: Some(persona_personal),
            airgap: SextantAirGap::new(),
            privacy_level: PrivacyLevel::Standard,
            is_mesh_open: false,
            tabs: Vec::new(),
            active_tab_id: None,
            is_settings_open: false,
            applied_provider: selected_provider,
            applied_provider_config: app_core::ProviderConfig {
                provider: selected_provider,
                gemini_key: gemini_key.clone(),
                openai_key: openai_key.clone(),
                anthropic_key: anthropic_key.clone(),
                local_endpoint: "http://localhost:8080".to_string(),
                local_model: "llama-3-8b".to_string(),
            },
            preferred_online_provider_config: app_core::ProviderConfig {
                provider: selected_provider,
                gemini_key: gemini_key.clone(),
                openai_key: openai_key.clone(),
                anthropic_key: anthropic_key.clone(),
                local_endpoint: "http://localhost:8080".to_string(),
                local_model: "llama-3-8b".to_string(),
            },
            selected_provider,
            gemini_key,
            openai_key,
            anthropic_key,
            local_endpoint: "http://localhost:8080".to_string(),
            local_model: "llama-3-8b".to_string(),
            ai_status_message: "Adjust provider settings and apply when ready.".to_string(),
            ai_status_is_error: false,
            last_tested_provider: None,
            last_test_success: false,
            last_test_message: "No provider test has been run yet.".to_string(),
            wake_search_query: String::new(),
            wake_search_results: Vec::new(),
            active_did: identity.did.clone(),
            deferred: DeferredState::new(
                identity.did.clone(),
                vec![identity.clone()],
                available_personas,
            ),
            async_result_tx,
            async_result_rx,
            pending_async_jobs: 0,
        };

        if state.gemini_key.is_empty()
            && state.openai_key.is_empty()
            && state.anthropic_key.is_empty()
        {
            state.add_log(
                "No cloud inference API key found in environment. Set GEMINI_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY.",
            );
        } else {
            state.update_brain().await;
            state.add_log("Loaded inference credentials from environment.");
        }
        for startup_alert in startup_alerts {
            state.add_startup_alert(startup_alert);
        }
        state.add_log(&format!("Wake/Log storage: {}", data_dir.display()));
        state.sync_tabs().await;
        state.refresh_memory().await;
        state.audit_baseline_timestamp = state.audit_trail.first().map(|entry| entry.timestamp);
        state.update_audit_validation_progress();
        state.refresh_pilot_snapshot().await;
        state
    });

    Xilem::new(state, app_logic_native)
        .run_windowed("SEXTANT — Sovereign Browser".to_string())
        .expect("Failed to run Sextant window");
}
