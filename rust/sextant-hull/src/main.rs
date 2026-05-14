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
use std::env;
use std::fs;
use std::io::Write;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;
use tokio::sync::Mutex;
use util::{app_data_dir, env_key};
use views::app_logic_native;
use xilem::Xilem;

#[derive(Clone, Copy)]
enum XilemShellMode {
    Safe,
    Minimal,
    InteractiveSmoke,
    Full,
}

impl XilemShellMode {
    fn label(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Minimal => "minimal",
            Self::InteractiveSmoke => "interactive-smoke",
            Self::Full => "full",
        }
    }
}

struct XilemLaunchOptions {
    shell_mode: XilemShellMode,
    smoke_timeout: Option<Duration>,
}

fn main() {
    install_panic_logging();
    tracing_subscriber::fmt::init();

    let launch = match parse_xilem_launch_options() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("[sextant-hull] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    apply_xilem_shell_mode(launch.shell_mode);
    append_xilem_diagnostic(&format!(
        "launch mode={} smoke_timeout={:?}",
        launch.shell_mode.label(),
        launch.smoke_timeout.map(|duration| duration.as_secs())
    ));
    if let Some(timeout) = launch.smoke_timeout {
        install_xilem_smoke_watchdog(launch.shell_mode, timeout);
    }

    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    let state = rt.block_on(async {
        let data_dir = app_data_dir();
        fs::create_dir_all(&data_dir).expect("Failed to create Sextant data directory");

        let vault = Arc::new(Mutex::new(CitadelVault::new()));
        let engine = Arc::new(Mutex::new(sextant_engine::SextantEngine::new()));

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
            location_input: String::new(),
            startup_phase: app_core::StartupPhase::Booting,
            startup_detail: "Constructing hull state before first paint.".to_string(),
            startup_bootstrap_requested: false,
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
        state.lifecycle_log("Hull state constructed. Waiting for first paint to bootstrap runtime.");
        state.add_log(&format!("Wake/Log storage: {}", data_dir.display()));
        state
    });

    append_xilem_diagnostic("state constructed; starting Xilem window");
    Xilem::new(state, app_logic_native)
        .run_windowed(format!(
            "SEXTANT - Sovereign Browser [{}]",
            launch.shell_mode.label()
        ))
        .expect("Failed to run Sextant window");
}

fn parse_xilem_launch_options() -> Result<XilemLaunchOptions, String> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--xilem-modes") {
        println!("safe");
        println!("minimal");
        println!("interactive-smoke");
        println!("full");
        std::process::exit(0);
    }

    let shell_mode = if let Some(mode) = xilem_smoke_mode_arg(&args) {
        parse_xilem_mode(&mode)?
    } else if env_flag("SEXTANT_MINIMAL_SHELL") {
        XilemShellMode::Minimal
    } else if env_flag("SEXTANT_INTERACTIVE_SMOKE") {
        XilemShellMode::InteractiveSmoke
    } else if env_flag("SEXTANT_FULL_SHELL") {
        XilemShellMode::Full
    } else {
        XilemShellMode::Safe
    };

    let smoke_timeout = if args.iter().any(|arg| arg == "--xilem-smoke") {
        Some(parse_duration_arg(
            &args,
            "--xilem-timeout",
            Duration::from_secs(8),
        )?)
    } else {
        None
    };

    Ok(XilemLaunchOptions {
        shell_mode,
        smoke_timeout,
    })
}

fn parse_xilem_mode(mode: &str) -> Result<XilemShellMode, String> {
    match mode {
        "safe" | "passive" => Ok(XilemShellMode::Safe),
        "minimal" => Ok(XilemShellMode::Minimal),
        "interactive" | "interactive-smoke" => Ok(XilemShellMode::InteractiveSmoke),
        "full" => Ok(XilemShellMode::Full),
        other => Err(format!(
            "unknown --xilem-smoke mode '{}'; expected safe, minimal, interactive-smoke, or full",
            other
        )),
    }
}

fn apply_xilem_shell_mode(mode: XilemShellMode) {
    env::remove_var("SEXTANT_MINIMAL_SHELL");
    env::remove_var("SEXTANT_INTERACTIVE_SMOKE");
    env::remove_var("SEXTANT_FULL_SHELL");
    match mode {
        XilemShellMode::Safe => {}
        XilemShellMode::Minimal => env::set_var("SEXTANT_MINIMAL_SHELL", "1"),
        XilemShellMode::InteractiveSmoke => env::set_var("SEXTANT_INTERACTIVE_SMOKE", "1"),
        XilemShellMode::Full => env::set_var("SEXTANT_FULL_SHELL", "1"),
    }
}

fn install_xilem_smoke_watchdog(mode: XilemShellMode, timeout: Duration) {
    thread::spawn(move || {
        thread::sleep(timeout);
        append_xilem_diagnostic(&format!(
            "smoke mode={} survived {}s; exiting 0",
            mode.label(),
            timeout.as_secs()
        ));
        println!(
            "[xilem-smoke] mode={} survived {}s",
            mode.label(),
            timeout.as_secs()
        );
        std::process::exit(0);
    });
}

fn parse_duration_arg(args: &[String], flag: &str, default: Duration) -> Result<Duration, String> {
    let Some(value) = arg_value(args, flag) else {
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

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn xilem_smoke_mode_arg(args: &[String]) -> Option<String> {
    let index = args.iter().position(|arg| arg == "--xilem-smoke")?;
    args.get(index + 1)
        .filter(|value| !value.starts_with("--"))
        .cloned()
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn install_panic_logging() {
    let panic_log = app_data_dir().join("panic.log");
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        if let Some(parent) = panic_log.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&panic_log)
        {
            let _ = writeln!(
                file,
                "[{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                panic_info
            );
        }
        previous_hook(panic_info);
    }));
}

fn append_xilem_diagnostic(message: &str) {
    let diagnostics_log = app_data_dir().join("xilem-diagnostics.log");
    if let Some(parent) = diagnostics_log.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&diagnostics_log)
    {
        let _ = writeln!(
            file,
            "[{}] {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            message
        );
    }
}
