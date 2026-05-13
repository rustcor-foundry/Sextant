use crate::app_core::{parse_direct_url_candidate, AiProvider, AppCommand};
use crate::poller::{async_poller, startup_bootstrapper};
use crate::state::SextantState;
use sextant_pilot::PilotStatus;
use xilem::view::{button, flex, label};
use xilem::{Axis, BoxedMasonryView, Color, MasonryView};

fn section_title(text: impl Into<String>) -> impl MasonryView<SextantState> {
    label(text.into()).color(Color::rgba(0.6, 0.95, 1.0, 1.0))
}

fn muted_text(text: impl Into<String>) -> impl MasonryView<SextantState> {
    label(text.into()).color(Color::rgba(0.55, 0.6, 0.65, 1.0))
}

fn validation_badge(text: impl Into<String>, color: Color) -> impl MasonryView<SextantState> {
    label(text.into()).color(color)
}

fn header_view(
    pilot_status: String,
    active_persona: String,
    is_vault_unlocked: bool,
    vault_color: Color,
) -> impl MasonryView<SextantState> {
    flex((
        label("SEXTANT").color(Color::rgba(0.9, 0.9, 0.95, 1.0)),
        muted_text("CORE BROWSER // STABILITY MODE"),
        muted_text(format!("PILOT {}", pilot_status)),
        muted_text(format!("PERSONA {}", active_persona)),
        label(if is_vault_unlocked {
            "VAULT UNLOCKED"
        } else {
            "VAULT LOCKED"
        })
        .color(vault_color),
    ))
    .direction(Axis::Horizontal)
}

fn browser_bar_view(location_input: String) -> impl MasonryView<SextantState> {
    flex((
        section_title("BROWSER"),
        label(location_input.clone()).color(Color::rgba(0.85, 0.9, 0.95, 1.0)),
        flex((
            button("GO", |state: &mut SextantState| {
                let location = state.location_input.trim().to_string();
                if location.is_empty() {
                    state.set_command_summary("Enter a web address before pressing GO.", true);
                    state.add_log("Go ignored because the address field was empty.");
                    return;
                }
                if let Some(url) = parse_direct_url_candidate(&location) {
                    state.set_command_summary(format!("Opening {} in the browser.", url), false);
                    state.add_log(&format!("Browser navigation requested for {}.", url));
                    state.queue_async_command(AppCommand::NavigateDirect(url));
                } else {
                    state.set_command_summary(
                        format!("'{}' is not a valid web address.", location),
                        true,
                    );
                    state.add_log(&format!(
                        "Go ignored because '{}' was not recognized as a web address.",
                        location
                    ));
                }
            }),
            button("EXAMPLE.COM", |state: &mut SextantState| {
                state.location_input = "https://example.com".to_string();
                state.set_command_summary("Address set to https://example.com.", false);
                state.add_log("Address preset set to https://example.com.");
            }),
            button("IANA.ORG", |state: &mut SextantState| {
                state.location_input = "https://www.iana.org".to_string();
                state.set_command_summary("Address set to https://www.iana.org.", false);
                state.add_log("Address preset set to https://www.iana.org.");
            }),
            button("USE ACTIVE URL", |state: &mut SextantState| {
                let active_location = state.current_location_text();
                if active_location.is_empty() {
                    state.set_command_summary("No active tab URL is available yet.", true);
                    state.add_log("Use-active-URL ignored because there was no active tab URL.");
                } else {
                    state.location_input = active_location.clone();
                    state.set_command_summary(
                        format!("Address bar synced to {}.", active_location),
                        false,
                    );
                }
            }),
        ))
        .direction(Axis::Horizontal),
        muted_text("Textbox input is temporarily disabled while we harden the renderer. Use EXAMPLE.COM or IANA.ORG, then GO."),
    ))
}

fn startup_gate_view(detail: String) -> impl MasonryView<SextantState> {
    flex((
        section_title("STARTUP"),
        label("Runtime warm-up is still in progress.").color(Color::rgba(1.0, 0.8, 0.3, 1.0)),
        muted_text(detail),
        muted_text("Browser and intent controls stay offline until startup ownership passes to the live runtime."),
        muted_text("Watch the startup line and system log for lifecycle progress and alerts."),
    ))
}

fn intent_bar_view(intent: String, route_message: String) -> impl MasonryView<SextantState> {
    flex((
        section_title("INTENT"),
        label(if intent.trim().is_empty() {
            "PRESET No intent selected".to_string()
        } else {
            format!("PRESET {}", intent)
        })
        .color(Color::rgba(0.85, 0.9, 0.95, 1.0)),
        muted_text(route_message),
        flex((
            button("PRESET SEARCH", |state: &mut SextantState| {
                state.intent = "open example.com and distill the page".to_string();
                state.set_command_summary(
                    "Intent preset selected: open example.com and distill the page.",
                    false,
                );
            }),
            button("PRESET CONSENT", |state: &mut SextantState| {
                state.intent = "buy a test item and request Captain's Key consent".to_string();
                state.set_command_summary(
                    "Intent preset selected: protected action for consent validation.",
                    false,
                );
            }),
            button("COMMAND", |state: &mut SextantState| {
                if !state.startup_allows_interaction() {
                    state.set_command_summary(
                        "Startup is still warming up. Intent command is not ready yet.",
                        true,
                    );
                    return;
                }
                let intent = state.intent.trim().to_string();
                if intent.is_empty() {
                    state.set_command_summary("Enter an intent before pressing COMMAND.", true);
                    state.add_log("Command ignored because the intent field was empty.");
                    return;
                }
                state.set_command_summary(format!("Running intent: {}", intent), false);
                state.queue_async_command(AppCommand::ProcessIntent(intent));
            }),
            button("NEW TAB", |state: &mut SextantState| {
                if !state.startup_allows_interaction() {
                    state.set_command_summary(
                        "Startup is still warming up. New tab is not ready yet.",
                        true,
                    );
                    return;
                }
                state.set_command_summary("Opening a new tab.", false);
                state.queue_async_command(AppCommand::CreateTab);
            }),
            button("AIR GAP", |state: &mut SextantState| {
                if !state.startup_allows_interaction() {
                    state.set_command_summary(
                        "Startup is still warming up. Air-gap control is not ready yet.",
                        true,
                    );
                    return;
                }
                state.set_command_summary("Cycling air-gap mode.", false);
                state.queue_async_command(AppCommand::ToggleAirgap);
            }),
            button("PRIVACY", |state: &mut SextantState| {
                if !state.startup_allows_interaction() {
                    state.set_command_summary(
                        "Startup is still warming up. Privacy control is not ready yet.",
                        true,
                    );
                    return;
                }
                state.set_command_summary("Cycling privacy mode.", false);
                state.queue_async_command(AppCommand::CyclePrivacy);
            }),
        ))
        .direction(Axis::Horizontal),
        muted_text("Text input is disabled in this stability build to avoid the current Masonry text-selection crash. Use presets for validation."),
    ))
}

fn provider_panel_view(
    summary: String,
    next_step: String,
    ai_status: String,
    status_color: Color,
    selected_provider: AiProvider,
    applied_provider: AiProvider,
    gemini_label: String,
    openai_label: String,
    anthropic_label: String,
    local_label: String,
    gemini_configured: bool,
    openai_configured: bool,
    anthropic_configured: bool,
    local_endpoint: String,
    local_model: String,
) -> impl MasonryView<SextantState> {
    let provider_picker = flex((
        button(gemini_label, |state: &mut SextantState| {
            state.select_provider(AiProvider::Gemini)
        }),
        button(openai_label, |state: &mut SextantState| {
            state.select_provider(AiProvider::OpenAI)
        }),
        button(anthropic_label, |state: &mut SextantState| {
            state.select_provider(AiProvider::Anthropic)
        }),
        button(local_label, |state: &mut SextantState| {
            state.select_provider(AiProvider::Local)
        }),
    ))
    .direction(Axis::Horizontal);

    let provider_fields = flex((
        muted_text(format!(
            "KEYS Gemini {}  OpenAI {}  Anthropic {}",
            if gemini_configured { "set" } else { "missing" },
            if openai_configured { "set" } else { "missing" },
            if anthropic_configured { "set" } else { "missing" }
        )),
        muted_text(format!("LOCAL ENDPOINT {}", local_endpoint)),
        muted_text(format!("LOCAL MODEL {}", local_model)),
        muted_text("Credential editing is disabled in this stability build. Use environment keys or Vault load/apply/test/save."),
    ));

    let provider_actions = flex((
        button("LOAD VAULT", |state: &mut SextantState| {
            state.set_command_summary("Loading provider settings from Vault.", false);
            state.queue_async_command(AppCommand::LoadProviderSettings);
        }),
        button("APPLY", |state: &mut SextantState| {
            state.set_command_summary("Applying provider settings.", false);
            state.queue_async_command(AppCommand::ApplyProviderSettings);
        }),
        button("TEST", |state: &mut SextantState| {
            state.set_command_summary("Testing active provider settings.", false);
            state.queue_async_command(AppCommand::TestProviderSettings);
        }),
        button("SAVE VAULT", |state: &mut SextantState| {
            state.set_command_summary("Saving provider settings to Vault.", false);
            state.queue_async_command(AppCommand::SaveProviderSettings);
        }),
    ))
    .direction(Axis::Horizontal);

    flex((
        section_title("AI PROVIDER"),
        muted_text(format!(
            "SELECTED {}  ACTIVE {}",
            selected_provider.label(),
            applied_provider.label()
        )),
        label(ai_status).color(status_color),
        muted_text(summary),
        muted_text(next_step),
        provider_picker,
        provider_fields,
        provider_actions,
    ))
}

fn operator_toggles_view(settings_open: bool, mesh_open: bool) -> impl MasonryView<SextantState> {
    flex((
        section_title("PANELS"),
        muted_text(format!(
            "SETTINGS {}  MESH {}",
            if settings_open { "open" } else { "closed" },
            if mesh_open { "open" } else { "closed" }
        )),
        flex((
            button(
                if settings_open {
                    "HIDE SETTINGS"
                } else {
                    "SHOW SETTINGS"
                },
                |state: &mut SextantState| {
                    state.is_settings_open = !state.is_settings_open;
                    state.set_command_summary(
                        if state.is_settings_open {
                            "Provider settings panel opened."
                        } else {
                            "Provider settings panel closed."
                        },
                        false,
                    );
                },
            ),
            button(
                if mesh_open { "HIDE MESH" } else { "SHOW MESH" },
                |state: &mut SextantState| {
                    state.is_mesh_open = !state.is_mesh_open;
                    state.set_command_summary(
                        if state.is_mesh_open {
                            "Mesh placeholder panel opened."
                        } else {
                            "Mesh placeholder panel closed."
                        },
                        false,
                    );
                },
            ),
        ))
        .direction(Axis::Horizontal),
    ))
}

fn safe_validation_shell_view(
    startup_status: String,
    command_summary: String,
    pending_jobs: usize,
) -> impl MasonryView<SextantState> {
    flex((
        startup_bootstrapper(),
        async_poller(pending_jobs > 0),
        label("SEXTANT").color(Color::rgba(0.9, 0.9, 0.95, 1.0)),
        muted_text("PASSIVE SAFE SHELL"),
        muted_text(startup_status),
        muted_text(format!("ASYNC JOBS {}", pending_jobs)),
        label(command_summary).color(Color::rgba(0.7, 0.85, 1.0, 1.0)),
        muted_text("Interactive widgets are disabled in the default shell after repeat native access violations in the Xilem/Masonry/Vello path."),
        muted_text("Core engine/Pilot/Wake behavior is currently verified through automated workspace tests while the native widget crash is isolated."),
        muted_text("Set SEXTANT_INTERACTIVE_SMOKE=1 for the one-button crash reproduction shell, or SEXTANT_FULL_SHELL=1 for the full native shell."),
    ))
}

fn interactive_smoke_shell_view(
    startup_status: String,
    command_summary: String,
    pending_jobs: usize,
) -> impl MasonryView<SextantState> {
    flex((
        startup_bootstrapper(),
        async_poller(pending_jobs > 0),
        label("SEXTANT").color(Color::rgba(0.9, 0.9, 0.95, 1.0)),
        muted_text("INTERACTIVE SMOKE SHELL"),
        muted_text(startup_status),
        muted_text(format!("ASYNC JOBS {}", pending_jobs)),
        label(command_summary).color(Color::rgba(0.7, 0.85, 1.0, 1.0)),
        button("OPEN EXAMPLE", |state: &mut SextantState| {
            if !state.startup_allows_interaction() {
                state.set_command_summary("Startup is still warming up.", true);
                return;
            }
            let url =
                parse_direct_url_candidate("https://example.com").expect("static URL should parse");
            state.location_input = url.to_string();
            state.set_command_summary("Opening https://example.com.", false);
            state.queue_async_command(AppCommand::NavigateDirect(url));
        }),
        muted_text("This mode intentionally reproduces the native interactive-widget crash path."),
    ))
}

fn system_log_panel_view(
    startup_alerts: &[String],
    log_views: Vec<impl MasonryView<SextantState>>,
) -> impl MasonryView<SextantState> {
    let startup_views: Vec<BoxedMasonryView<SextantState, ()>> = if startup_alerts.is_empty() {
        vec![Box::new(muted_text(
            "No startup alerts are currently recorded.",
        ))]
    } else {
        startup_alerts
            .iter()
            .rev()
            .take(4)
            .map(|alert| {
                Box::new(label(format!("ALERT {}", alert)).color(Color::rgba(1.0, 0.72, 0.3, 1.0)))
                    as BoxedMasonryView<SextantState, ()>
            })
            .collect()
    };

    flex((
        section_title("SYSTEM"),
        flex(startup_views),
        section_title("SYSTEM LOG"),
        flex(log_views),
    ))
}

fn dashboard_view(
    brain_name: String,
    persona: String,
    did_prefix: String,
    airgap_status: String,
    privacy_status: String,
    runtime_status: String,
    runtime_status_color: Color,
    startup_status: String,
    startup_status_color: Color,
    command_summary: String,
    command_summary_color: Color,
    pending_jobs: usize,
    can_go_back: bool,
    can_go_forward: bool,
    tab_views: Vec<impl MasonryView<SextantState>>,
) -> impl MasonryView<SextantState> {
    flex((
        section_title("DASHBOARD"),
        muted_text(format!("BRAIN {}", brain_name)),
        muted_text(format!("PERSONA {}", persona)),
        muted_text(format!("DID {}", did_prefix)),
        muted_text(format!("AIR GAP {}", airgap_status)),
        muted_text(format!("PRIVACY {}", privacy_status)),
        label(runtime_status).color(runtime_status_color),
        label(startup_status).color(startup_status_color),
        label(command_summary).color(command_summary_color),
        muted_text(format!("ASYNC JOBS {}", pending_jobs)),
        muted_text(format!(
            "HISTORY back {}  forward {}",
            if can_go_back { "ready" } else { "empty" },
            if can_go_forward { "ready" } else { "empty" }
        )),
        flex((
            button("BACK", |state: &mut SextantState| {
                if !state.startup_allows_interaction() {
                    state.set_command_summary("Startup is still warming up. Back is not ready yet.", true);
                    return;
                }
                if state.active_tab_id.is_none() {
                    state.set_command_summary("No active tab is available for back navigation.", true);
                    state.add_log("Back ignored because no tab was active.");
                } else if !state.active_tab_can_go_back() {
                    state.set_command_summary("Active tab has no back history.", true);
                    state.add_log("Back ignored because the active tab has no back history.");
                } else {
                    state.set_command_summary("Moving back in active tab history.", false);
                    state.queue_async_command(AppCommand::GoBackActiveTab);
                }
            }),
            button("FORWARD", |state: &mut SextantState| {
                if !state.startup_allows_interaction() {
                    state.set_command_summary("Startup is still warming up. Forward is not ready yet.", true);
                    return;
                }
                if state.active_tab_id.is_none() {
                    state.set_command_summary("No active tab is available for forward navigation.", true);
                    state.add_log("Forward ignored because no tab was active.");
                } else if !state.active_tab_can_go_forward() {
                    state.set_command_summary("Active tab has no forward history.", true);
                    state.add_log("Forward ignored because the active tab has no forward history.");
                } else {
                    state.set_command_summary("Moving forward in active tab history.", false);
                    state.queue_async_command(AppCommand::GoForwardActiveTab);
                }
            }),
            button("RELOAD", |state: &mut SextantState| {
                if !state.startup_allows_interaction() {
                    state.set_command_summary("Startup is still warming up. Reload is not ready yet.", true);
                    return;
                }
                if state.active_tab_id.is_none() {
                    state.set_command_summary("No active tab is available to reload.", true);
                    state.add_log("Reload ignored because no tab was active.");
                } else {
                    state.set_command_summary("Reloading the active tab.", false);
                    state.queue_async_command(AppCommand::ReloadActiveTab);
                }
            }),
            button("CLOSE ACTIVE", |state: &mut SextantState| {
                if !state.startup_allows_interaction() {
                    state.set_command_summary("Startup is still warming up. Close tab is not ready yet.", true);
                    return;
                }
                if state.active_tab_id.is_some() {
                    state.queue_async_command(AppCommand::CloseActiveTab);
                } else {
                    state.set_command_summary("No active tab is available to close.", true);
                    state.add_log("Close-active ignored because no tab was active.");
                }
            }),
        ))
        .direction(Axis::Horizontal),
        flex(tab_views),
        muted_text("Stability mode keeps the browser path narrow: use GO to create the first tab and navigate."),
    ))
}

fn viewport_view(
    engine_status_text: String,
    page_title: String,
    page_url: String,
    page_content: String,
    consent_message: String,
    consent_next_step: String,
    audit_status_line: String,
    consent_status_color: Color,
    consent_next_step_color: Color,
) -> impl MasonryView<SextantState> {
    flex((
        section_title("VIEWPORT"),
        muted_text(engine_status_text),
        label(audit_status_line).color(consent_status_color),
        label(page_title).color(Color::rgba(0.9, 0.9, 0.9, 1.0)),
        muted_text(page_url),
        label(page_content).color(Color::rgba(0.8, 0.82, 0.85, 1.0)),
        label(consent_message).color(consent_status_color),
        label(consent_next_step).color(consent_next_step_color),
        flex((
            button("AUTHORIZE", |state: &mut SextantState| {
                if let PilotStatus::AwaitingConsent(msg) = state.pilot_status.clone() {
                    state.set_command_summary("Authorizing pending consent request.", false);
                    state.queue_async_command(AppCommand::Authorize(msg));
                } else {
                    state.set_command_summary(
                        "No consent request is waiting for authorization.",
                        true,
                    );
                    state.add_log("Authorize ignored because no consent request was pending.");
                }
            }),
            button("DENY", |state: &mut SextantState| {
                if matches!(state.pilot_status, PilotStatus::AwaitingConsent(_)) {
                    state.set_command_summary("Denying pending consent request.", false);
                    state.queue_async_command(AppCommand::DenyConsent);
                } else {
                    state.set_command_summary("No consent request is waiting to be denied.", true);
                    state.add_log("Deny ignored because no consent request was pending.");
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
        label(format!(
            "SEARCH QUERY {}",
            if wake_query.trim().is_empty() {
                "digital wake"
            } else {
                wake_query.as_str()
            }
        ))
        .color(Color::rgba(0.85, 0.9, 0.95, 1.0)),
        flex((
            button("QUERY WAKE", |state: &mut SextantState| {
                state.wake_search_query = "digital wake".to_string();
                state.set_command_summary("Wake search preset selected: digital wake.", false);
            }),
            button("QUERY EXAMPLE", |state: &mut SextantState| {
                state.wake_search_query = "example".to_string();
                state.set_command_summary("Wake search preset selected: example.", false);
            }),
            button("SEARCH", |state: &mut SextantState| {
                if state.wake_search_query.trim().is_empty() {
                    state.wake_search_query = "digital wake".to_string();
                }
                state.set_command_summary("Running Wake search.", false);
                state.queue_async_command(AppCommand::SearchWake(state.wake_search_query.clone()));
            }),
            button("CONSOLIDATE", |state: &mut SextantState| {
                state.set_command_summary("Consolidating Wake entries.", false);
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

fn mesh_panel_view() -> impl MasonryView<SextantState> {
    flex((
        section_title("MESH"),
        label("Mesh is not wired into the native hull yet.")
            .color(Color::rgba(1.0, 0.72, 0.3, 1.0)),
        muted_text("Keep Sextant local-first for tonight's validation pass."),
        muted_text("Future work here will cover peer discovery, chat, and shared artifacts."),
    ))
}

fn validation_panel_view(
    summary_line: String,
    summary_color: Color,
    focus_line: String,
    focus_color: Color,
    progress_line: String,
    progress_color: Color,
    workflow_rollup_line: String,
    remaining_checks: Vec<String>,
    item_views: Vec<BoxedMasonryView<SextantState, ()>>,
) -> impl MasonryView<SextantState> {
    let remaining_views: Vec<_> = if remaining_checks.is_empty() {
        vec![Box::new(muted_text(
            "No current checklist gaps are detected in the native hull state.",
        )) as BoxedMasonryView<SextantState, ()>]
    } else {
        remaining_checks
            .into_iter()
            .map(|check| Box::new(muted_text(check)) as BoxedMasonryView<SextantState, ()>)
            .collect()
    };

    flex((
        section_title("VALIDATION CHECKLIST"),
        muted_text("Use this panel as the current click-through guide for native-hull testing."),
        label(summary_line).color(summary_color),
        label(focus_line).color(focus_color),
        label(progress_line).color(progress_color),
        muted_text(workflow_rollup_line),
        button("RESET VALIDATION", |state: &mut SextantState| {
            state.reset_validation_session();
            state.set_command_summary(
                "Validation session reset. Checklist coverage cleared.",
                false,
            );
            state.add_log("Validation session reset. Coverage markers cleared for a fresh pass.");
        }),
        flex(remaining_views),
        flex(item_views),
    ))
}

pub fn app_logic_native(state: &mut SextantState) -> BoxedMasonryView<SextantState, ()> {
    if std::env::var("SEXTANT_MINIMAL_SHELL")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
    {
        return Box::new(flex((
            startup_bootstrapper(),
            async_poller(state.pending_async_jobs > 0),
            label("SEXTANT").color(Color::rgba(0.9, 0.9, 0.95, 1.0)),
            muted_text("MINIMAL SHELL MODE"),
            muted_text(state.startup_status_line()),
            muted_text(format!("ASYNC JOBS {}", state.pending_async_jobs)),
            muted_text(state.last_command_summary.clone()),
        )));
    }

    let full_shell_enabled = std::env::var("SEXTANT_FULL_SHELL")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    let interactive_smoke_enabled = std::env::var("SEXTANT_INTERACTIVE_SMOKE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    if interactive_smoke_enabled {
        return Box::new(interactive_smoke_shell_view(
            state.startup_status_line(),
            state.last_command_summary.clone(),
            state.pending_async_jobs,
        ));
    }
    if !full_shell_enabled {
        return Box::new(safe_validation_shell_view(
            state.startup_status_line(),
            state.last_command_summary.clone(),
            state.pending_async_jobs,
        ));
    }

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
    let wake_log_views: Vec<_> = state
        .logs
        .iter()
        .rev()
        .take(8)
        .map(|log| label(log.clone()).color(Color::rgba(0.6, 0.6, 0.6, 1.0)))
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
        .unwrap_or_else(|| {
            "Browser standby. Enter a web address or an intent above to begin.".to_string()
        });
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
    let consent_next_step_color = if matches!(state.pilot_status, PilotStatus::AwaitingConsent(_)) {
        Color::rgba(1.0, 0.8, 0.3, 1.0)
    } else {
        Color::rgba(0.7, 0.85, 1.0, 1.0)
    };
    let runtime_status = state.runtime_status_message();
    let runtime_status_color = match state.pilot_status {
        PilotStatus::AwaitingConsent(_) => Color::rgba(1.0, 0.8, 0.3, 1.0),
        PilotStatus::Reasoning
        | PilotStatus::Navigating(_)
        | PilotStatus::Distilling
        | PilotStatus::ExecutingAction(_)
        | PilotStatus::ReasoningComplete(_) => Color::rgba(0.7, 0.85, 1.0, 1.0),
        PilotStatus::Idle if state.pending_async_jobs > 0 => Color::rgba(0.7, 0.85, 1.0, 1.0),
        PilotStatus::Idle => Color::rgba(0.0, 0.95, 0.45, 1.0),
    };
    let startup_status = state.startup_status_line();
    let startup_status_color = match state.startup_phase {
        crate::app_core::StartupPhase::Ready => Color::rgba(0.0, 0.95, 0.45, 1.0),
        crate::app_core::StartupPhase::Degraded => Color::rgba(1.0, 0.72, 0.3, 1.0),
        crate::app_core::StartupPhase::Booting | crate::app_core::StartupPhase::WarmingUp => {
            Color::rgba(1.0, 0.8, 0.3, 1.0)
        }
    };
    let checklist_pass = Color::rgba(0.0, 0.95, 0.45, 1.0);
    let checklist_warn = Color::rgba(1.0, 0.8, 0.3, 1.0);
    let checklist_fail = Color::rgba(1.0, 0.3, 0.3, 1.0);
    let validation_summary_color =
        if state.completed_validation_workflow_steps() == state.total_validation_workflow_steps() {
            checklist_pass
        } else {
            checklist_warn
        };
    let validation_focus_color = if state.remaining_validation_checks().is_empty() {
        checklist_pass
    } else {
        checklist_warn
    };
    let validation_progress_color = if state.validation_progress_percent() >= 100 {
        checklist_pass
    } else if state.validation_progress_percent() == 0 {
        checklist_fail
    } else {
        checklist_warn
    };

    let validation_item_views: Vec<BoxedMasonryView<SextantState, ()>> = vec![
        Box::new(validation_badge(
            format!("PROVIDER {}", state.provider_validation_status_message()),
            if state.provider_validation_steps_completed() >= 4 {
                checklist_pass
            } else {
                checklist_warn
            },
        )),
        Box::new(validation_badge(
            format!("CONSENT {}", state.consent_validation_status_message()),
            if state.consent_validation_steps_completed() >= 3 {
                checklist_pass
            } else {
                checklist_warn
            },
        )),
        Box::new(validation_badge(
            format!("TABS {}", state.tab_validation_status_message()),
            if state.tab_validation_steps_completed() >= 3 {
                checklist_pass
            } else {
                checklist_warn
            },
        )),
        Box::new(validation_badge(
            format!("WAKE {}", state.wake_validation_status_message()),
            if state.wake_validation_steps_completed() >= 2 {
                checklist_pass
            } else {
                checklist_warn
            },
        )),
        Box::new(validation_badge(
            format!("CONTROLS {}", state.control_validation_status_message()),
            if state.control_validation_steps_completed() >= 3 {
                checklist_pass
            } else {
                checklist_warn
            },
        )),
        Box::new(validation_badge(
            format!("AUDIT {}", state.audit_validation_status_message()),
            if state.audit_validation_steps_completed() >= 1 {
                checklist_pass
            } else {
                checklist_warn
            },
        )),
        Box::new(muted_text(format!(
            "LATEST PROVIDER {}",
            state.latest_validation_step_for("PROVIDER")
        ))),
        Box::new(muted_text(format!(
            "LATEST WAKE {}",
            state.latest_validation_step_for("WAKE")
        ))),
    ];

    let wake_views: Vec<BoxedMasonryView<SextantState, ()>> =
        if state.wake_search_results.is_empty() {
            state
                .recent_memory
                .iter()
                .take(8)
                .map(|entry| {
                    Box::new(muted_text(format!("{} :: {}", entry.url, entry.title)))
                        as BoxedMasonryView<SextantState, ()>
                })
                .collect()
        } else {
            state
                .wake_search_results
                .iter()
                .take(8)
                .map(|entry| {
                    Box::new(muted_text(format!("{} :: {}", entry.url, entry.title)))
                        as BoxedMasonryView<SextantState, ()>
                })
                .collect()
        };
    let wake_views = if wake_views.is_empty() {
        vec![Box::new(muted_text(
            "No Wake entries are visible yet. Complete an intent or run a search.",
        )) as BoxedMasonryView<SextantState, ()>]
    } else {
        wake_views
    };
    let audit_views: Vec<BoxedMasonryView<SextantState, ()>> = if state.audit_trail.is_empty() {
        vec![
            Box::new(muted_text("No Captain's Log entries are visible yet."))
                as BoxedMasonryView<SextantState, ()>,
        ]
    } else {
        state
            .audit_trail
            .iter()
            .rev()
            .take(8)
            .map(|entry| {
                Box::new(muted_text(format!("{:?} {}", entry.status, entry.intent)))
                    as BoxedMasonryView<SextantState, ()>
            })
            .collect()
    };

    let tab_views: Vec<BoxedMasonryView<SextantState, ()>> = if state.tabs.is_empty() {
        vec![Box::new(flex((
            muted_text("No tabs are open."),
            muted_text("Use NEW TAB or GO to open the first page."),
        )))]
    } else {
        state
            .tabs
            .iter()
            .map(|tab| {
                let title = tab
                    .url
                    .as_ref()
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| "about:blank".to_string());
                let active = Some(tab.id) == state.active_tab_id;
                let tab_label = if active {
                    format!("ACTIVE {}", title)
                } else {
                    format!("OPEN {}", title)
                };
                let tab_id = tab.id;
                let tab_title = title.clone();
                Box::new(button(tab_label, move |state: &mut SextantState| {
                    if Some(tab_id) == state.active_tab_id {
                        state.set_command_summary("Selected tab is already active.", false);
                        state.add_log(&format!(
                            "Tab switch ignored because {} was already active.",
                            tab_title
                        ));
                    } else {
                        state
                            .set_command_summary(format!("Switching to tab {}.", tab_title), false);
                        state.queue_async_command(AppCommand::SwitchTab(tab_id));
                    }
                })) as BoxedMasonryView<SextantState, ()>
            })
            .collect()
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
    );

    let browser_bar: BoxedMasonryView<SextantState, ()> = if state.startup_allows_interaction() {
        Box::new(browser_bar_view(state.location_input.clone()))
    } else {
        Box::new(startup_gate_view(state.startup_detail.clone()))
    };
    let intent_bar: BoxedMasonryView<SextantState, ()> = if state.startup_allows_interaction() {
        Box::new(intent_bar_view(
            state.intent.clone(),
            state.intent_route_message(),
        ))
    } else {
        Box::new(muted_text(
            "Intent controls will appear after startup warm-up completes.",
        ))
    };

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
        runtime_status,
        runtime_status_color,
        startup_status,
        startup_status_color,
        state.last_command_summary.clone(),
        command_summary_color,
        state.pending_async_jobs,
        state.active_tab_can_go_back(),
        state.active_tab_can_go_forward(),
        tab_views,
    );

    let viewport = viewport_view(
        engine_status_text,
        page_title,
        page_url,
        page_content,
        consent_msg
            .as_ref()
            .map(|m| format!("CAPTAIN'S KEY REQUIRED — {}", m))
            .unwrap_or_else(|| "No consent request is pending.".to_string()),
        state.consent_next_step_message(),
        state.latest_audit_status_line(),
        consent_status_color,
        consent_next_step_color,
    );

    let operator_toggles = operator_toggles_view(state.is_settings_open, state.is_mesh_open);

    let provider_panel: BoxedMasonryView<SextantState, ()> = if state.is_settings_open {
        Box::new(provider_panel_view(
            state.active_provider_readiness_message(),
            state.provider_next_step_message(),
            state.ai_status_message.clone(),
            if state.ai_status_is_error {
                Color::rgba(1.0, 0.3, 0.3, 1.0)
            } else if state.active_provider_ready() {
                Color::rgba(0.0, 0.95, 0.45, 1.0)
            } else {
                Color::rgba(1.0, 0.8, 0.3, 1.0)
            },
            state.selected_provider,
            state.applied_provider,
            state.provider_button_label(AiProvider::Gemini),
            state.provider_button_label(AiProvider::OpenAI),
            state.provider_button_label(AiProvider::Anthropic),
            state.provider_button_label(AiProvider::Local),
            !state.gemini_key.trim().is_empty(),
            !state.openai_key.trim().is_empty(),
            !state.anthropic_key.trim().is_empty(),
            state.local_endpoint.clone(),
            state.local_model.clone(),
        ))
    } else {
        Box::new(muted_text(
            "Provider settings are collapsed. Open them from PANELS when validating inference routes.",
        ))
    };

    let mesh_panel: BoxedMasonryView<SextantState, ()> = if state.is_mesh_open {
        Box::new(mesh_panel_view())
    } else {
        Box::new(muted_text(
            "Mesh panel is collapsed. The native mesh surface is still intentionally deferred.",
        ))
    };

    let wake_panel = wake_panel_view(
        state.wake_search_query.clone(),
        wake_views,
        audit_views,
        wake_log_views,
    );
    let validation_panel = validation_panel_view(
        state.validation_summary_line(),
        validation_summary_color,
        state.validation_focus_line(),
        validation_focus_color,
        state.validation_progress_line(),
        validation_progress_color,
        state.validation_workflow_rollup_line(),
        state.remaining_validation_checks(),
        validation_item_views,
    );

    let system_panel = system_log_panel_view(&state.startup_alerts, log_views);

    return Box::new(flex((
        startup_bootstrapper(),
        async_poller(state.pending_async_jobs > 0),
        header,
        browser_bar,
        intent_bar,
        dashboard,
        viewport,
        operator_toggles,
        provider_panel,
        mesh_panel,
        wake_panel,
        validation_panel,
        system_panel,
    )));
}
