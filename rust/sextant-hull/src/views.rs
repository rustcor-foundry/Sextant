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
                state.set_command_summary(
                    if state.is_settings_open {
                        "Settings panel opened."
                    } else {
                        "Settings panel closed."
                    },
                    false,
                );
            },
        ),
        button(if is_mesh_open { "CLOSE MESH" } else { "MESH" }, |state: &mut SextantState| {
            state.is_mesh_open = !state.is_mesh_open;
            if state.is_mesh_open {
                state.set_command_summary("Mesh panel opened as a native placeholder.", false);
                state.add_log("Mesh panel is a later native pass. Core workflow remains local-first.");
            } else {
                state.set_command_summary("Mesh panel closed.", false);
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
                    state.add_log("Command ignored because the intent field was empty.");
                    return;
                }
                if !state.active_provider_ready() {
                    let message = state.active_provider_readiness_message();
                    state.set_command_summary(message.clone(), true);
                    state.add_log(&format!("Command blocked: {}", message));
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
                    "Submitted intent '{}' via {}.",
                    intent,
                    state.applied_provider.label()
                ));
                state.queue_async_command(AppCommand::ProcessIntent(intent));
            }),
            button("TOGGLE AIRGAP", |state: &mut SextantState| {
                state.set_command_summary("Cycling air-gap state.", false);
                state.queue_async_command(AppCommand::ToggleAirgap);
            }),
            button("CYCLE PRIVACY", |state: &mut SextantState| {
                state.set_command_summary("Cycling privacy level.", false);
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
    runtime_status: String,
    runtime_status_color: Color,
    startup_status: String,
    startup_status_color: Color,
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
        label(runtime_status).color(runtime_status_color),
        label(startup_status).color(startup_status_color),
        label(command_summary).color(command_summary_color),
        muted_text(format!("ASYNC JOBS {}", pending_jobs)),
        flex((
            button("NEW TAB", |state: &mut SextantState| {
                state.set_command_summary("Opening a new tab.", false);
                state.queue_async_command(AppCommand::CreateTab);
            }),
            button("CLOSE ACTIVE", |state: &mut SextantState| {
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
                    state.set_command_summary("No consent request is waiting for authorization.", true);
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
        textbox(wake_query, |state: &mut SextantState, val| {
            state.wake_search_query = val
        }),
        flex((
            button("SEARCH", |state: &mut SextantState| {
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
        label("Mesh is not wired into the native hull yet.").color(Color::rgba(1.0, 0.72, 0.3, 1.0)),
        muted_text("Keep Sextant local-first for tonight's validation pass."),
        muted_text("Future work here will cover peer discovery, chat, and shared artifacts."),
    ))
}

fn validation_panel_view(
    summary_line: String,
    summary_color: Color,
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
            .map(|check| Box::new(muted_text(format!("NEXT {}", check))) as BoxedMasonryView<SextantState, ()>)
            .collect()
    };

    flex((
        section_title("VALIDATION CHECKLIST"),
        muted_text("Use this panel as the current click-through guide for native-hull testing."),
        label(summary_line).color(summary_color),
        button("RESET VALIDATION", |state: &mut SextantState| {
            state.reset_validation_session();
            state.set_command_summary("Validation session reset. Checklist coverage cleared.", false);
            state.add_log("Validation session reset. Coverage markers cleared for a fresh pass.");
        }),
        flex(remaining_views),
        flex(item_views),
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

    let wake_views: Vec<BoxedMasonryView<SextantState, ()>> = if state.wake_search_results.is_empty() {
        vec![Box::new(flex((
            muted_text("No Wake results are currently loaded."),
            muted_text("Run SEARCH to query memory or submit an intent to record new pages."),
        )))]
    } else {
        state
            .wake_search_results
            .iter()
            .take(8)
            .map(|entry| {
                Box::new(flex((
                    label(entry.title.clone()).color(Color::rgba(0.9, 0.9, 0.9, 1.0)),
                    muted_text(entry.url.to_string()),
                    muted_text(format!("importance {:.2}", entry.importance)),
                ))) as BoxedMasonryView<SextantState, ()>
            })
            .collect()
    };

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
    let startup_status = if state.startup_alerts.is_empty() {
        "STARTUP STATUS nominal".to_string()
    } else if state.startup_alerts.len() == 1 {
        format!("STARTUP ALERT {}", state.startup_alerts[0])
    } else {
        format!(
            "STARTUP ALERTS {} active. Latest: {}",
            state.startup_alerts.len(),
            state.startup_alerts.last().cloned().unwrap_or_default()
        )
    };
    let startup_status_color = if state.startup_alerts.is_empty() {
        Color::rgba(0.0, 0.95, 0.45, 1.0)
    } else {
        Color::rgba(1.0, 0.72, 0.3, 1.0)
    };
    let checklist_pass = Color::rgba(0.0, 0.95, 0.45, 1.0);
    let checklist_warn = Color::rgba(1.0, 0.8, 0.3, 1.0);
    let checklist_fail = Color::rgba(1.0, 0.3, 0.3, 1.0);

    let tab_views: Vec<BoxedMasonryView<SextantState, ()>> = if state.tabs.is_empty() {
        vec![Box::new(flex((
            muted_text("No tabs are open."),
            muted_text("Use NEW TAB or submit an intent that opens a page."),
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
                        state.add_log(&format!("Tab switch ignored because {} was already active.", tab_title));
                    } else {
                        state.set_command_summary(
                            format!("Switching to tab {}.", tab_title),
                            false,
                        );
                        state.queue_async_command(AppCommand::SwitchTab(tab_id));
                    }
                })) as BoxedMasonryView<SextantState, ()>
            })
            .collect()
    };

    let provider_route_check: BoxedMasonryView<SextantState, ()> = if state.has_unapplied_provider_changes() {
        Box::new(flex((
            label("CHECK provider route pending").color(checklist_warn),
            muted_text("Apply the selected provider settings before expecting command routing to change."),
            muted_text(state.provider_validation_status_message()),
        )))
    } else if state.active_provider_ready() {
        Box::new(flex((
            label("CHECK provider route ready").color(checklist_pass),
            muted_text(format!(
                "Active route is {} and the current configuration is usable.",
                state.applied_provider.label()
            )),
            muted_text(state.provider_validation_status_message()),
        )))
    } else {
        Box::new(flex((
            label("CHECK provider route blocked").color(checklist_fail),
            muted_text(state.active_provider_readiness_message()),
            muted_text(state.provider_validation_status_message()),
        )))
    };

    let provider_test_check: BoxedMasonryView<SextantState, ()> =
        if state.provider_load_exercised
            && state.provider_apply_exercised
            && state.provider_test_exercised
            && state.provider_save_exercised
            && state.last_tested_provider == Some(state.applied_provider)
            && state.last_test_success
        {
            Box::new(flex((
                label("CHECK provider workflow complete").color(checklist_pass),
                muted_text(format!(
                    "{} load/apply/test/save coverage is complete for this session.",
                    state.applied_provider.label()
                )),
                muted_text(state.provider_validation_status_message()),
            )))
        } else if state.last_tested_provider == Some(state.applied_provider) && state.last_test_success {
            Box::new(flex((
                label("CHECK provider test passed").color(checklist_pass),
                muted_text(format!(
                    "{} was tested successfully with the current active settings.",
                    state.applied_provider.label()
                )),
                muted_text(state.provider_validation_status_message()),
            )))
        } else if state.last_tested_provider == Some(state.applied_provider) {
            Box::new(flex((
                label("CHECK provider test failed").color(checklist_fail),
                muted_text(format!(
                    "{} needs attention before relying on this route.",
                    state.applied_provider.label()
                )),
                muted_text(state.provider_validation_status_message()),
            )))
        } else {
            Box::new(flex((
                label("CHECK provider test pending").color(checklist_warn),
                muted_text(format!(
                    "Run TEST for the active {} route before deeper validation.",
                    state.applied_provider.label()
                )),
                muted_text(state.provider_validation_status_message()),
            )))
        };

    let airgap_check: BoxedMasonryView<SextantState, ()> =
        if state.airgap_offline_exercised
            && state.airgap_online_exercised
            && state.privacy_cycle_exercised
        {
            Box::new(flex((
                label("CHECK control workflow complete").color(checklist_pass),
                muted_text("Air-gap offline/online transitions and privacy cycling have all been exercised in this session."),
                muted_text(state.control_validation_status_message()),
            )))
        } else if state.airgap.get_status() == sextant_airgap::AirGapStatus::Online {
            Box::new(flex((
                label("CHECK air-gap online").color(checklist_pass),
                muted_text("Online mode is active; the configured provider route can be exercised directly."),
                muted_text(state.control_validation_status_message()),
            )))
        } else if state.provider_ready(AiProvider::Local) {
            Box::new(flex((
                label("CHECK offline local ready").color(checklist_pass),
                muted_text("Air-gap is enforcing local inference and the local route is ready."),
                muted_text(state.control_validation_status_message()),
            )))
        } else {
            Box::new(flex((
                label("CHECK offline local blocked").color(checklist_fail),
                muted_text("Air-gap is active, but the local endpoint still needs configuration."),
                muted_text(state.control_validation_status_message()),
            )))
        };

    let consent_check: BoxedMasonryView<SextantState, ()> =
        if matches!(state.pilot_status, PilotStatus::AwaitingConsent(_)) {
            Box::new(flex((
                label("CHECK consent pending").color(checklist_warn),
                muted_text("A Captain's Key request is waiting. AUTHORIZE or DENY to continue."),
                muted_text(state.consent_validation_status_message()),
            )))
        } else if state.consent_request_exercised
            && state.consent_authorized_exercised
            && state.consent_denied_exercised
        {
            Box::new(flex((
                label("CHECK consent coverage complete").color(checklist_pass),
                muted_text("Captain's Key request, authorize, and deny paths have all been exercised in this session."),
                muted_text(state.consent_validation_status_message()),
            )))
        } else {
            Box::new(flex((
                label("CHECK consent coverage pending").color(checklist_warn),
                muted_text("Run a protected intent, then exercise AUTHORIZE and DENY to complete Captain's Key coverage."),
                muted_text(state.consent_validation_status_message()),
            )))
        };

    let tab_check: BoxedMasonryView<SextantState, ()> =
        if state.tab_open_exercised && state.tab_switch_exercised && state.tab_close_exercised {
            Box::new(flex((
                label("CHECK tab workflow complete").color(checklist_pass),
                muted_text("Open, switch, and close tab paths have all been exercised in this session."),
                muted_text(state.tab_validation_status_message()),
            )))
        } else if state.active_tab_id.is_some() {
            Box::new(flex((
                label("CHECK active tab ready").color(checklist_pass),
                muted_text("A tab is active, so switch/close/navigation validation can proceed."),
                muted_text(state.tab_validation_status_message()),
            )))
        } else {
            Box::new(flex((
                label("CHECK active tab missing").color(checklist_warn),
                muted_text("Open a tab or submit an intent before testing tab and viewport behavior."),
                muted_text(state.tab_validation_status_message()),
            )))
        };

    let wake_check: BoxedMasonryView<SextantState, ()> =
        if state.wake_search_exercised && state.wake_consolidate_exercised {
            Box::new(flex((
                label("CHECK wake workflow complete").color(checklist_pass),
                muted_text("Wake search and consolidation have both been exercised in this session."),
                muted_text(state.wake_validation_status_message()),
            )))
        } else if !state.wake_search_results.is_empty() || !state.recent_memory.is_empty() {
            Box::new(flex((
                label("CHECK wake visibility ready").color(checklist_pass),
                muted_text("Wake results or recent memory entries are visible for inspection."),
                muted_text(state.wake_validation_status_message()),
            )))
        } else {
            Box::new(flex((
                label("CHECK wake visibility pending").color(checklist_warn),
                muted_text("Run SEARCH or complete an intent to confirm Wake persistence and display."),
                muted_text(state.wake_validation_status_message()),
            )))
        };

    let audit_check: BoxedMasonryView<SextantState, ()> = if !state.audit_trail.is_empty() {
        Box::new(flex((
            label("CHECK audit trail ready").color(checklist_pass),
            muted_text("Captain's Log contains entries that can be compared against the current workflow."),
        )))
    } else {
        Box::new(flex((
            label("CHECK audit trail pending").color(checklist_warn),
            muted_text("Run a command or protected flow to generate audit entries for validation."),
        )))
    };

    let validation_trail_one = state
        .validation_successes
        .iter()
        .rev()
        .nth(0)
        .cloned()
        .unwrap_or_else(|| "No successful validation step recorded yet.".to_string());
    let validation_trail_two = state
        .validation_successes
        .iter()
        .rev()
        .nth(1)
        .cloned()
        .unwrap_or_else(|| "Run a successful step to populate the trail.".to_string());
    let validation_trail_three = state
        .validation_successes
        .iter()
        .rev()
        .nth(2)
        .cloned()
        .unwrap_or_else(|| "The latest successes will stay visible here.".to_string());

    let validation_trail_check: BoxedMasonryView<SextantState, ()> = Box::new(flex((
        label("CHECK recent validation trail").color(if state.validation_successes.is_empty() {
            checklist_warn
        } else {
            checklist_pass
        }),
        muted_text(validation_trail_one),
        muted_text(validation_trail_two),
        muted_text(validation_trail_three),
    )));

    let remaining_validation_checks = state.remaining_validation_checks();
    let validation_summary_line = state.validation_summary_line();
    let validation_summary_color = if remaining_validation_checks.is_empty() {
        checklist_pass
    } else {
        checklist_warn
    };

    let validation_panel = validation_panel_view(
        validation_summary_line,
        validation_summary_color,
        remaining_validation_checks,
        vec![
            provider_route_check,
            provider_test_check,
            airgap_check,
            consent_check,
            tab_check,
            wake_check,
            audit_check,
            validation_trail_check,
        ],
    );

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
        let route_status_color = if state.has_unapplied_provider_changes() {
            Color::rgba(1.0, 0.7, 0.3, 1.0)
        } else if state.selected_provider == state.applied_provider {
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
        let next_step_color = if state.has_unapplied_provider_changes()
            || (state.airgap.get_status() != sextant_airgap::AirGapStatus::Online
                && !state.provider_ready(AiProvider::Local))
            || (state.last_tested_provider == Some(state.applied_provider)
                && !state.last_test_success)
        {
            Color::rgba(1.0, 0.8, 0.3, 1.0)
        } else {
            Color::rgba(0.0, 0.95, 0.45, 1.0)
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
                        muted_text(if state.has_unapplied_provider_changes() {
                            "PENDING CHANGES yes".to_string()
                        } else {
                            "PENDING CHANGES no".to_string()
                        }),
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
                        label(state.provider_next_step_message()).color(next_step_color),
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
                        state.set_gemini_key(val)
                    }),
                    muted_text("OpenAI API Key"),
                    textbox(state.openai_key.clone(), |state: &mut SextantState, val| {
                        state.set_openai_key(val)
                    }),
                    muted_text("Anthropic API Key"),
                    textbox(state.anthropic_key.clone(), |state: &mut SextantState, val| {
                        state.set_anthropic_key(val)
                    }),
                    muted_text("Local Endpoint"),
                    textbox(state.local_endpoint.clone(), |state: &mut SextantState, val| {
                        state.set_local_endpoint(val)
                    }),
                    muted_text("Local Model"),
                    textbox(state.local_model.clone(), |state: &mut SextantState, val| {
                        state.set_local_model(val)
                    }),
                )),
                flex((
                    section_title("ACTIONS"),
                    flex((
                        button("APPLY", |state: &mut SextantState| {
                            if !state.provider_ready(state.selected_provider) {
                                let message = format!(
                                    "{} configuration is incomplete. Fill the required fields before applying.",
                                    state.selected_provider.label()
                                );
                                state.set_command_summary(message.clone(), true);
                                state.set_ai_status(message.clone(), true);
                                state.add_log(&format!("Apply blocked: {}", message));
                                return;
                            }
                            state.set_command_summary(
                                format!(
                                    "Applying {} provider settings.",
                                    state.selected_provider.label()
                                ),
                                false,
                            );
                            state.queue_async_command(AppCommand::ApplyProviderSettings);
                        }),
                        button("TEST", |state: &mut SextantState| {
                            if !state.provider_ready(state.selected_provider) {
                                let message = format!(
                                    "{} configuration is incomplete. Fill the required fields before testing.",
                                    state.selected_provider.label()
                                );
                                state.set_command_summary(message.clone(), true);
                                state.set_ai_status(message.clone(), true);
                                state.add_log(&format!("Test blocked: {}", message));
                                return;
                            }
                            state.set_command_summary(
                                format!(
                                    "Testing {} provider settings.",
                                    state.selected_provider.label()
                                ),
                                false,
                            );
                            state.queue_async_command(AppCommand::TestProviderSettings);
                        }),
                        button("SAVE VAULT", |state: &mut SextantState| {
                            state.set_command_summary("Saving AI settings to Vault.", false);
                            state.queue_async_command(AppCommand::SaveProviderSettings);
                        }),
                        button("LOAD VAULT", |state: &mut SextantState| {
                            state.set_command_summary("Loading AI settings from Vault.", false);
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
        runtime_status,
        runtime_status_color,
        startup_status,
        startup_status_color,
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
        consent_msg
            .as_ref()
            .map(|m| format!("CAPTAIN'S KEY REQUIRED — {}", m))
            .unwrap_or_else(|| "No consent request is pending.".to_string()),
        state.consent_next_step_message(),
        state.latest_audit_status_line(),
        consent_status_color,
        consent_next_step_color,
    );

    let wake_panel =
        wake_panel_view(state.wake_search_query.clone(), wake_views, audit_views, log_views);

    let main_content: BoxedMasonryView<SextantState, ()> = if state.is_mesh_open {
        Box::new(flex((
            dashboard,
            viewport,
            wake_panel,
            validation_panel,
            mesh_panel_view(),
        )))
    } else {
        Box::new(flex((dashboard, viewport, wake_panel, validation_panel)))
    };

    let body: BoxedMasonryView<SextantState, ()> = if state.is_settings_open {
        Box::new(flex((settings_panel, intent_bar, main_content)))
    } else {
        Box::new(flex((intent_bar, main_content)))
    };

    flex((async_poller(state.pending_async_jobs > 0), header, body))
}
