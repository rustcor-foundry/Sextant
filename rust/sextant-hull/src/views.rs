use crate::app_core::{AiProvider, AppCommand};
use crate::poller::async_poller;
use crate::state::SextantState;
use crate::util::app_data_dir;
use sextant_pilot::PilotStatus;
use url::Url;
use xilem::view::{button, flex, label, textbox};
use xilem::{Axis, BoxedMasonryView, Color, MasonryView};

fn section_title(text: impl Into<String>) -> impl MasonryView<SextantState> {
    label(text.into()).color(Color::rgba(0.6, 0.95, 1.0, 1.0))
}

fn muted_text(text: impl Into<String>) -> impl MasonryView<SextantState> {
    label(text.into()).color(Color::rgba(0.55, 0.6, 0.65, 1.0))
}

fn header_view(
    pilot_status: String,
    active_persona: String,
    is_vault_unlocked: bool,
    vault_color: Color,
    is_settings_open: bool,
    is_mesh_open: bool,
) -> impl MasonryView<SextantState> {
    flex((
        label("SEXTANT").color(Color::rgba(0.9, 0.9, 0.95, 1.0)),
        muted_text("SOVEREIGN BROWSER // NATIVE HULL"),
        muted_text(format!("PILOT {}", pilot_status)),
        muted_text(format!("PERSONA {}", active_persona)),
        label(if is_vault_unlocked {
            "VAULT UNLOCKED"
        } else {
            "VAULT LOCKED"
        })
        .color(vault_color),
        button(
            if is_settings_open {
                "CLOSE SETTINGS"
            } else {
                "SETTINGS"
            },
            |state: &mut SextantState| {
                state.is_settings_open = !state.is_settings_open;
            },
        ),
        button(if is_mesh_open { "CLOSE MESH" } else { "MESH" }, |state: &mut SextantState| {
            state.is_mesh_open = !state.is_mesh_open;
            if state.is_mesh_open {
                state.add_log("Mesh panel is a later native pass. Core workflow remains local-first.");
            }
        }),
    ))
    .direction(Axis::Horizontal)
}

fn intent_bar_view(intent: String) -> impl MasonryView<SextantState> {
    flex((
        section_title("INTENT"),
        textbox(intent, |state: &mut SextantState, val| state.intent = val),
        flex((
            button("COMMAND", |state: &mut SextantState| {
                let intent = state.intent.trim().to_string();
                if intent.is_empty() {
                    state.set_command_summary("Enter an intent before running COMMAND.", true);
                    state.add_log("COMMAND ignored: intent was empty.");
                    return;
                }
                if !state.active_provider_ready() {
                    let message = state.active_provider_readiness_message();
                    state.set_command_summary(message.clone(), true);
                    state.add_log(&format!("COMMAND blocked: {}", message));
                    state.is_settings_open = true;
                    return;
                }
                state.set_command_summary(
                    format!(
                        "Submitting intent through {}.",
                        state.applied_provider.label()
                    ),
                    false,
                );
                state.add_log(&format!(
                    "Submitting intent '{}' via {}.",
                    intent,
                    state.applied_provider.label()
                ));
                state.queue_async_command(AppCommand::ProcessIntent(intent));
            }),
            button("TOGGLE AIRGAP", |state: &mut SextantState| {
                state.queue_async_command(AppCommand::ToggleAirgap);
            }),
            button("CYCLE PRIVACY", |state: &mut SextantState| {
                state.queue_async_command(AppCommand::CyclePrivacy);
            }),
        ))
        .direction(Axis::Horizontal),
    ))
}

fn dashboard_view(
    brain_name: String,
    persona: String,
    did_prefix: String,
    airgap_status: String,
    privacy_status: String,
    command_summary: String,
    command_summary_color: Color,
    pending_jobs: usize,
    tab_views: Vec<impl MasonryView<SextantState>>,
) -> impl MasonryView<SextantState> {
    flex((
        section_title("DASHBOARD"),
        muted_text(format!("BRAIN {}", brain_name)),
        muted_text(format!("PERSONA {}", persona)),
        muted_text(format!("DID {}", did_prefix)),
        muted_text(format!("AIR GAP {}", airgap_status)),
        muted_text(format!("PRIVACY {}", privacy_status)),
        label(command_summary).color(command_summary_color),
        muted_text(format!("ASYNC JOBS {}", pending_jobs)),
        flex((
            button("NEW TAB", |state: &mut SextantState| {
                state.queue_async_command(AppCommand::CreateTab);
            }),
            button("CLOSE ACTIVE", |state: &mut SextantState| {
                state.queue_async_command(AppCommand::CloseActiveTab);
            }),
        ))
        .direction(Axis::Horizontal),
        flex(tab_views),
    ))
}

fn viewport_view(
    engine_status_text: String,
    page_title: String,
    page_url: String,
    page_content: String,
    consent_message: String,
    audit_status_line: String,
    consent_status_color: Color,
) -> impl MasonryView<SextantState> {
    flex((
        section_title("VIEWPORT"),
        muted_text(engine_status_text),
        label(audit_status_line).color(consent_status_color),
        label(page_title).color(Color::rgba(0.9, 0.9, 0.9, 1.0)),
        muted_text(page_url),
        label(page_content).color(Color::rgba(0.8, 0.82, 0.85, 1.0)),
        label(consent_message).color(consent_status_color),
        flex((
            button("AUTHORIZE", |state: &mut SextantState| {
                if let PilotStatus::AwaitingConsent(msg) = state.pilot_status.clone() {
                    state.set_command_summary("Authorizing pending consent request.", false);
                    state.queue_async_command(AppCommand::Authorize(msg));
                } else {
                    state.set_command_summary("No consent request is waiting for authorization.", true);
                    state.add_log("AUTHORIZE ignored: no consent request was pending.");
                }
            }),
            button("DENY", |state: &mut SextantState| {
                if matches!(state.pilot_status, PilotStatus::AwaitingConsent(_)) {
                    state.set_command_summary("Denying pending consent request.", false);
                    state.queue_async_command(AppCommand::DenyConsent);
                } else {
                    state.set_command_summary("No consent request is waiting to be denied.", true);
                    state.add_log("DENY ignored: no consent request was pending.");
                }
            }),
        ))
        .direction(Axis::Horizontal),
    ))
}

fn wake_panel_view(
    wake_query: String,
    wake_views: Vec<impl MasonryView<SextantState>>,
    audit_views: Vec<impl MasonryView<SextantState>>,
    log_views: Vec<impl MasonryView<SextantState>>,
) -> impl MasonryView<SextantState> {
    flex((
        section_title("DIGITAL WAKE"),
        textbox(wake_query, |state: &mut SextantState, val| {
            state.wake_search_query = val
        }),
        flex((
            button("SEARCH", |state: &mut SextantState| {
                state.queue_async_command(AppCommand::SearchWake(state.wake_search_query.clone()));
            }),
            button("CONSOLIDATE", |state: &mut SextantState| {
                state.queue_async_command(AppCommand::ConsolidateWake);
            }),
        ))
        .direction(Axis::Horizontal),
        muted_text("SEARCH RESULTS"),
        flex(wake_views),
        section_title("CAPTAIN'S LOG"),
        flex(audit_views),
        section_title("SYSTEM LOG"),
        flex(log_views),
    ))
}

pub fn app_logic_native(state: &mut SextantState) -> impl MasonryView<SextantState> {
    state.drain_async_results();
    let data_dir = app_data_dir();
    let pilot_status = match state.pilot_status.clone() {
        PilotStatus::Idle => "IDLE".to_string(),
        PilotStatus::Reasoning => "REASONING".to_string(),
        PilotStatus::ReasoningComplete(_) => "PLAN READY".to_string(),
        PilotStatus::Navigating(url) => format!("NAVIGATING {}", url),
        PilotStatus::Distilling => "DISTILLING".to_string(),
        PilotStatus::AwaitingConsent(_) => "AWAITING CONSENT".to_string(),
        PilotStatus::ExecutingAction(msg) => format!("EXECUTING {}", msg),
    };

    let vault_color = if state.is_vault_unlocked {
        Color::rgba(0.0, 0.95, 0.45, 1.0)
    } else {
        Color::rgba(1.0, 0.25, 0.25, 1.0)
    };

    let log_views: Vec<_> = state
        .logs
        .iter()
        .rev()
        .take(20)
        .map(|log| label(log.clone()).color(Color::rgba(0.6, 0.6, 0.6, 1.0)))
        .collect();

    let audit_views: Vec<_> = state
        .audit_trail
        .iter()
        .map(|entry| {
            let status_text = match &entry.status {
                sextant_log::LogStatus::Success => "SUCCESS".to_string(),
                sextant_log::LogStatus::Failure(err) => format!("FAILURE {}", err),
                sextant_log::LogStatus::Aborted => "ABORTED".to_string(),
                sextant_log::LogStatus::AwaitingConsent => "AWAITING CONSENT".to_string(),
            };
            let status_color = match &entry.status {
                sextant_log::LogStatus::Success => Color::rgba(0.0, 0.95, 0.45, 1.0),
                sextant_log::LogStatus::Failure(_) => Color::rgba(1.0, 0.3, 0.3, 1.0),
                sextant_log::LogStatus::Aborted => Color::rgba(1.0, 0.7, 0.3, 1.0),
                sextant_log::LogStatus::AwaitingConsent => Color::rgba(1.0, 0.8, 0.3, 1.0),
            };
            flex((
                label(entry.intent.clone()).color(Color::rgba(0.9, 0.9, 0.9, 1.0)),
                label(status_text).color(status_color),
                muted_text(format!(
                    "{}",
                    entry.timestamp.with_timezone(&chrono::Local).format("%H:%M:%S")
                )),
            ))
        })
        .collect();

    let wake_views: Vec<_> = state
        .wake_search_results
        .iter()
        .take(8)
        .map(|entry| {
            flex((
                label(entry.title.clone()).color(Color::rgba(0.9, 0.9, 0.9, 1.0)),
                muted_text(entry.url.to_string()),
                muted_text(format!("importance {:.2}", entry.importance)),
            ))
        })
        .collect();

    let engine_status_text = state
        .engine_status
        .as_ref()
        .map(|s| {
            format!(
                "BACKEND {:?}  THREADS {}  LAYOUT {:.1}ms  MEM {}MB",
                s.active_backend, s.parallel_threads, s.layout_time_ms, s.memory_usage_mb
            )
        })
        .unwrap_or_else(|| "ENGINE STANDBY — AWAITING FIRST NAVIGATION".to_string());

    let page_title = state
        .active_page
        .as_ref()
        .map(|p| p.title.clone())
        .unwrap_or_else(|| "Pilot standby. Enter an intent above to begin.".to_string());
    let page_url = state
        .active_page
        .as_ref()
        .map(|p| format!("URL  {}", p.url))
        .unwrap_or_default();
    let page_content = state
        .active_page
        .as_ref()
        .map(|p| p.content.clone())
        .unwrap_or_default();

    let consent_msg = match state.pilot_status.clone() {
        PilotStatus::AwaitingConsent(msg) => Some(msg.clone()),
        _ => None,
    };
    let consent_status_color = if consent_msg.is_some() {
        Color::rgba(1.0, 0.75, 0.3, 1.0)
    } else if state.last_command_is_error {
        Color::rgba(1.0, 0.3, 0.3, 1.0)
    } else {
        Color::rgba(0.7, 0.85, 1.0, 1.0)
    };
    let command_summary_color = if state.last_command_is_error {
        Color::rgba(1.0, 0.3, 0.3, 1.0)
    } else {
        Color::rgba(0.7, 0.85, 1.0, 1.0)
    };

    let tab_views: Vec<_> = state
        .tabs
        .iter()
        .map(|tab| {
            let title = tab
                .url
                .as_ref()
                .map(|u| u.to_string())
                .unwrap_or_else(|| "about:blank".to_string());
            let active = Some(tab.id) == state.active_tab_id;
            let label = if active {
                format!("ACTIVE {}", title)
            } else {
                format!("OPEN {}", title)
            };
            let tab_id = tab.id;
            button(label, move |state: &mut SextantState| {
                state.queue_async_command(AppCommand::SwitchTab(tab_id));
            })
        })
        .collect();

    let settings_panel: BoxedMasonryView<SextantState, ()> = {
        let provider_ready = state.provider_ready(state.selected_provider);
        let provider_status_color = if provider_ready {
            Color::rgba(0.0, 0.95, 0.45, 1.0)
        } else {
            Color::rgba(1.0, 0.3, 0.3, 1.0)
        };
        let action_status_color = if state.ai_status_is_error {
            Color::rgba(1.0, 0.3, 0.3, 1.0)
        } else {
            Color::rgba(0.8, 0.9, 1.0, 1.0)
        };
        let route_status_color = if state.selected_provider == state.applied_provider {
            Color::rgba(0.7, 0.85, 1.0, 1.0)
        } else {
            Color::rgba(1.0, 0.7, 0.3, 1.0)
        };
        let provider_status_text = match state.selected_provider {
            AiProvider::Gemini if provider_ready => "Gemini key configured.".to_string(),
            AiProvider::Gemini => "Gemini key missing.".to_string(),
            AiProvider::OpenAI if provider_ready => "OpenAI key configured.".to_string(),
            AiProvider::OpenAI => "OpenAI key missing.".to_string(),
            AiProvider::Anthropic if provider_ready => "Anthropic key configured.".to_string(),
            AiProvider::Anthropic => "Anthropic key missing.".to_string(),
            AiProvider::Local if provider_ready => format!(
                "Local endpoint ready at {} using model {}.",
                state.local_endpoint, state.local_model
            ),
            AiProvider::Local => "Local endpoint is invalid or missing.".to_string(),
        };
        let gemini_status = if state.gemini_key.trim().is_empty() {
            "missing"
        } else {
            "configured"
        };
        let openai_status = if state.openai_key.trim().is_empty() {
            "missing"
        } else {
            "configured"
        };
        let anthropic_status = if state.anthropic_key.trim().is_empty() {
            "missing"
        } else {
            "configured"
        };
        let local_status = if Url::parse(&state.local_endpoint).is_ok() {
            "configured"
        } else {
            "invalid endpoint"
        };
        let last_test_text = if let Some(provider) = state.last_tested_provider {
            format!(
                "LAST TEST {} {} {}",
                provider.label(),
                if state.last_test_success {
                    "passed"
                } else {
                    "failed"
                },
                state.last_test_message
            )
        } else {
            "LAST TEST none yet".to_string()
        };

        Box::new(flex((
            section_title("SETTINGS // AI & SYSTEM"),
            flex((
                flex((
                    muted_text(format!("AIR GAP {:?}", state.airgap.get_status())),
                    muted_text(format!("PRIVACY {:?}", state.privacy_level)),
                    muted_text(format!("DATA {}", data_dir.display())),
                    muted_text("SECRETS saved via Vault controls below"),
                )),
                flex((
                    section_title("AI CONTROL"),
                    flex((
                        muted_text(format!(
                            "SELECTED PROVIDER {}",
                            state.selected_provider.label()
                        )),
                        muted_text(format!(
                            "ACTIVE PROVIDER {}",
                            state.applied_provider.label()
                        )),
                        muted_text(format!("ACTIVE BRAIN {}", state.pilot_brain_name)),
                        label(if provider_ready {
                            "CONFIG STATUS READY"
                        } else {
                            "CONFIG STATUS INCOMPLETE"
                        })
                        .color(provider_status_color),
                        label(provider_status_text).color(provider_status_color),
                    )),
                    flex((
                        muted_text(format!("Gemini {}", gemini_status)),
                        muted_text(format!("OpenAI {}", openai_status)),
                        muted_text(format!("Anthropic {}", anthropic_status)),
                        muted_text(format!("Local {}", local_status)),
                    )),
                    flex((
                        label(state.intent_route_message()).color(route_status_color),
                        label(last_test_text).color(route_status_color),
                        label(state.ai_status_message.clone()).color(action_status_color),
                    )),
                )),
                flex((
                    section_title("PROVIDERS"),
                    flex((
                        button(
                            state.provider_button_label(AiProvider::Gemini),
                            |state: &mut SextantState| {
                                state.select_provider(AiProvider::Gemini);
                            },
                        ),
                        button(
                            state.provider_button_label(AiProvider::OpenAI),
                            |state: &mut SextantState| {
                                state.select_provider(AiProvider::OpenAI);
                            },
                        ),
                        button(
                            state.provider_button_label(AiProvider::Anthropic),
                            |state: &mut SextantState| {
                                state.select_provider(AiProvider::Anthropic);
                            },
                        ),
                        button(
                            state.provider_button_label(AiProvider::Local),
                            |state: &mut SextantState| {
                                state.select_provider(AiProvider::Local);
                            },
                        ),
                    ))
                    .direction(Axis::Horizontal),
                )),
                flex((
                    section_title("CREDENTIALS"),
                    muted_text("Gemini API Key"),
                    textbox(state.gemini_key.clone(), |state: &mut SextantState, val| {
                        state.gemini_key = val
                    }),
                    muted_text("OpenAI API Key"),
                    textbox(state.openai_key.clone(), |state: &mut SextantState, val| {
                        state.openai_key = val
                    }),
                    muted_text("Anthropic API Key"),
                    textbox(state.anthropic_key.clone(), |state: &mut SextantState, val| {
                        state.anthropic_key = val
                    }),
                    muted_text("Local Endpoint"),
                    textbox(state.local_endpoint.clone(), |state: &mut SextantState, val| {
                        state.local_endpoint = val
                    }),
                    muted_text("Local Model"),
                    textbox(state.local_model.clone(), |state: &mut SextantState, val| {
                        state.local_model = val
                    }),
                )),
                flex((
                    section_title("ACTIONS"),
                    flex((
                        button("APPLY", |state: &mut SextantState| {
                            state.queue_async_command(AppCommand::ApplyProviderSettings);
                        }),
                        button("TEST", |state: &mut SextantState| {
                            state.queue_async_command(AppCommand::TestProviderSettings);
                        }),
                        button("SAVE VAULT", |state: &mut SextantState| {
                            state.queue_async_command(AppCommand::SaveProviderSettings);
                        }),
                        button("LOAD VAULT", |state: &mut SextantState| {
                            state.queue_async_command(AppCommand::LoadProviderSettings);
                        }),
                    ))
                    .direction(Axis::Horizontal),
                )),
            )),
        )))
    };

    let header = header_view(
        pilot_status,
        state
            .active_persona
            .as_ref()
            .map(|p| p.label.clone())
            .unwrap_or_else(|| "NONE".to_string()),
        state.is_vault_unlocked,
        vault_color,
        state.is_settings_open,
        state.is_mesh_open,
    );

    let intent_bar = intent_bar_view(state.intent.clone());

    let dashboard = dashboard_view(
        state.pilot_brain_name.clone(),
        state
            .active_persona
            .as_ref()
            .map(|p| p.label.clone())
            .unwrap_or_else(|| "UNBOUND".to_string()),
        state.active_did[..state.active_did.len().min(20)].to_string(),
        format!("{:?}", state.airgap.get_status()),
        format!("{:?}", state.privacy_level),
        state.last_command_summary.clone(),
        command_summary_color,
        state.pending_async_jobs,
        tab_views,
    );

    let viewport = viewport_view(
        engine_status_text,
        page_title,
        page_url,
        page_content,
        state.latest_audit_status_line(),
        consent_msg
            .as_ref()
            .map(|m| format!("CAPTAIN'S KEY REQUIRED — {}", m))
            .unwrap_or_else(|| "No consent request is pending.".to_string()),
        consent_status_color,
    );

    let wake_panel =
        wake_panel_view(state.wake_search_query.clone(), wake_views, audit_views, log_views);

    let main_content = flex((dashboard, viewport, wake_panel));

    let body: BoxedMasonryView<SextantState, ()> = if state.is_settings_open {
        Box::new(flex((settings_panel, intent_bar, main_content)))
    } else {
        Box::new(flex((intent_bar, main_content)))
    };

    flex((async_poller(state.pending_async_jobs > 0), header, body))
}
