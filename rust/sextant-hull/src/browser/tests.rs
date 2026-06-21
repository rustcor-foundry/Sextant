//! Unit tests for the sextant-browser shell, split out of browser.rs to keep
//! the implementation file navigable. Attached via `#[cfg(test)] #[path] mod
//! tests;`; `use super::*` gives the tests the crate root's items (the moved
//! impl-partition methods are pub, so they remain reachable here).

use super::*;

#[test]
fn detects_explicit_native_intents() {
    assert_eq!(
        native_intent_body("intent: open example.com and distill").as_deref(),
        Some("open example.com and distill")
    );
    assert_eq!(
        native_intent_body("research Rust ownership docs").as_deref(),
        Some("research Rust ownership docs")
    );
    assert!(native_intent_body("plain web search").is_none());
}

#[test]
fn plans_domain_intent_without_losing_memory_step() {
    let plan = plan_native_intent("open example.com and distill").unwrap();
    assert_eq!(plan.target.as_str(), "https://example.com/");
    assert!(plan.should_distill);
    assert!(plan.steps.iter().any(|step| step.contains("Distill")));
}

#[test]
fn strips_intent_verbs_for_search_targets() {
    assert_eq!(
        intent_navigation_text("research sovereign browser architecture"),
        "sovereign browser architecture"
    );
    let target =
        parse_navigation_target(&intent_navigation_text("summarize https://example.com now"))
            .unwrap();
    assert_eq!(target.as_str(), "https://example.com/");
}

#[test]
fn summarizes_hosted_direct_timing_logs() {
    let summary = parse_hosted_direct_log_summary_text(
        r#"
[window-direct] first direct present in 103ms
[window-direct] active load status HeadParsed after 193ms url https://example.test/
[window-direct] active load status Complete after 4.5s url https://example.test/
[window-direct] retained navigation swap to https://example.test/next after 415ms (head parsed)
[window-direct] verified input direct frame in 368ms
[window-direct] verified repeat input direct frame in 114ms
[window-direct] slow direct frame 7 total=238ms spin=4ms paint=210ms present=24ms
[window-direct] slow direct frame 8 total=1.3s spin=7ms paint=1.2s present=20ms
[window-direct] chrome status tab 2/3 back true forward false title Example Domain url https://example.test/next
[window-direct] certificate fingerprint sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
[window-direct] host command certificate-go-back
[window-direct] host command trust-once
[window-direct] host command trust-this-appliance
[window-direct] resource audit {"imageCount":12,"brokenImages":["missing.png"],"inlineSvgCount":9,"zeroSizeSvgCount":2,"stylesheetCount":4,"canvasCount":1}
"#,
    );

    assert_eq!(summary.first_present.as_deref(), Some("103ms"));
    assert_eq!(summary.latest_load.as_deref(), Some("Complete 4.5s"));
    assert!(summary.latest_load_complete);
    assert_eq!(summary.load_progress(), 1.0);
    assert_eq!(summary.latest_swap.as_deref(), Some("415ms"));
    assert_eq!(summary.latest_input.as_deref(), Some("repeat 114ms"));
    assert_eq!(
        summary.latest_url.as_deref(),
        Some("https://example.test/next")
    );
    assert_eq!(summary.active_title.as_deref(), Some("Example Domain"));
    assert_eq!(summary.active_tab_index, Some(2));
    assert_eq!(summary.tab_count, Some(3));
    assert!(summary.can_switch_tabs());
    assert!(summary.can_go_back);
    assert!(!summary.can_go_forward);
    assert_eq!(
        summary.certificate_fingerprint_sha256.as_deref(),
        Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    );
    assert!(summary.certificate_warning_active());
    assert_eq!(
        summary.compact_certificate_status().as_deref(),
        Some("cert 01234567..cdef")
    );
    assert!(summary.certificate_back_requested);
    assert!(summary.certificate_trust_once_requested);
    assert!(summary.certificate_trust_this_appliance_requested);
    assert_eq!(summary.slow_frames, 2);
    assert_eq!(summary.max_frame_label.as_deref(), Some("1.3s"));
    assert_eq!(summary.max_paint_label.as_deref(), Some("1.2s"));
    assert!(summary.compact_perf_status().contains("slow 2 paint 1.2s"));
    assert_eq!(
        summary.latest_resource_audit.as_deref(),
        Some("audit img 12 broken 1 svg 9/2 css 4 canvas 1")
    );

    let default_summary = HostedDirectLogSummary::default();
    assert!(!default_summary.can_go_back);
    assert!(!default_summary.can_go_forward);
    assert!(!default_summary.can_switch_tabs());
    assert!(!default_summary.certificate_warning_active());
}

#[test]
fn parses_hosted_direct_certificate_smoke_actions() -> Result<(), String> {
    assert_eq!(HostedDirectSmokeAction::parse_arg(None)?, None);
    assert_eq!(
        HostedDirectSmokeAction::parse_arg(Some("back".to_string()))?,
        Some(HostedDirectSmokeAction::Back)
    );
    assert_eq!(
        HostedDirectSmokeAction::parse_arg(Some("trust-once".to_string()))?,
        Some(HostedDirectSmokeAction::TrustOnce)
    );
    assert_eq!(
        HostedDirectSmokeAction::parse_arg(Some("trust".to_string()))?,
        Some(HostedDirectSmokeAction::TrustThisAppliance)
    );
    assert!(HostedDirectSmokeAction::parse_arg(Some("ignore".to_string())).is_err());
    Ok(())
}

#[test]
fn hosted_direct_child_bounds_keep_chrome_reserved() {
    assert_eq!(
        hosted_direct_child_bounds(PhysicalSize::new(1180, 760)),
        (0, HOSTED_DIRECT_CHROME_H as i32, 1180, 688)
    );
    assert_eq!(
        hosted_direct_child_bounds(PhysicalSize::new(100, 80)),
        (0, HOSTED_DIRECT_CHROME_H as i32, 320, 240)
    );
}

#[test]
fn hosted_direct_chrome_rects_leave_room_for_address() {
    let rects = hosted_direct_chrome_rects(PhysicalSize::new(1180, 760));
    assert!(rects.back.x < rects.forward.x);
    assert!(rects.forward.x < rects.reload.x);
    assert!(rects.reload.x < rects.new_tab.x);
    assert!(rects.new_tab.x < rects.previous_tab.x);
    assert!(rects.previous_tab.x < rects.next_tab.x);
    assert!(rects.next_tab.x < rects.close_tab.x);
    assert!(rects.close_tab.x < rects.address.x);
    assert!(rects.certificate_back.x < rects.trust_once.x);
    assert!(rects.trust_once.x < rects.trust_appliance.x);
    assert!(rects.address.w >= 160);
    assert!(rects.address.y < HOSTED_DIRECT_CHROME_H);
}

#[test]
fn hosted_direct_address_input_tracks_cursor() {
    let mut shell = HostedDirectShellState::new("https://example.test".to_string());
    shell.address_focused = true;
    shell.address_replace_on_text = true;
    shell.push_address_text("abcd");
    assert_eq!(shell.address_input, "abcd");
    assert_eq!(shell.address_cursor, 4);

    shell.address_cursor = 2;
    shell.push_address_text("Z");
    assert_eq!(shell.address_input, "abZcd");
    assert_eq!(shell.address_cursor, 3);
    assert_eq!(shell.address_display_text(12), "abZ_cd");

    assert_eq!(previous_char_boundary("abZcd", 3), 2);
    assert_eq!(next_char_boundary("abZcd", 3), 4);
}

#[test]
fn pauses_sensitive_intents_for_consent() {
    assert!(intent_needs_consent("buy this item and checkout"));
    assert!(!intent_needs_consent("summarize example.com"));
}

#[cfg(feature = "xilem-shell")]
#[test]
fn pilot_action_plan_pauses_sensitive_intents() {
    let actions = pilot_action_plan("buy this item and checkout").unwrap();
    assert!(matches!(actions.first(), Some(PilotAction::Analyze(_))));
    assert!(matches!(
        actions.get(1),
        Some(PilotAction::RequestConsent(message)) if message.contains("Authorize")
    ));
    assert!(
        actions
            .iter()
            .skip(2)
            .any(|action| matches!(action, PilotAction::Navigate(_))),
        "sensitive intents should carry a resumable browser continuation"
    );
    assert!(
        actions
            .iter()
            .skip(2)
            .any(|action| matches!(action, PilotAction::Perceive)),
        "resumed sensitive intents should perceive the authorized page"
    );
}

#[cfg(feature = "xilem-shell")]
#[test]
fn pilot_action_plan_maps_safe_intents_to_browser_actions() {
    let actions = pilot_action_plan("open example.com and distill").unwrap();
    assert!(matches!(
        actions.first(),
        Some(PilotAction::Navigate(url)) if url.as_str() == "https://example.com/"
    ));
    assert!(actions
        .iter()
        .any(|action| matches!(action, PilotAction::Distill)));
    assert!(actions
        .iter()
        .any(|action| matches!(action, PilotAction::Analyze(_))));
}

#[test]
fn browser_shell_can_deny_pending_pilot_consent() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-deny-consent-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    app.run_native_intent_sync("intent: buy example.com and checkout");
    assert_eq!(app.pilot_status, "AWAITING CONSENT");
    assert!(app.pending_consent.is_some());

    app.deny_pilot_consent();
    assert_eq!(app.pilot_status, "CONSENT DENIED");
    assert!(app.pending_consent.is_none());
    app.refresh_logs();
    assert!(app
        .recent_logs
        .iter()
        .any(|entry| entry.intent.starts_with("pilot consent deny ")));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

/// End-to-end check that the live agent path plans with the local model brain
/// and records a structured run. Ignored by default: needs a model server on
/// SEXTANT_LOCAL_ENDPOINT (default http://127.0.0.1:8101).
#[cfg(feature = "xilem-shell")]
#[test]
#[ignore = "requires a local model server (SEXTANT_LOCAL_ENDPOINT, default :8101)"]
fn brain_intent_produces_structured_plan() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-brain-intent-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.set_browser_mode(BrowserMode::Agent);

    app.run_native_intent("intent: open example.com and summarize the page");
    assert!(
        app.pending_pilot_plan.is_some(),
        "intent should dispatch to the brain lane (status: {})",
        app.last_status
    );

    let deadline = Instant::now() + Duration::from_secs(90);
    while app.pending_pilot_plan.is_some() && Instant::now() < deadline {
        let _ = app.collect_pending_pilot_plan();
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        app.pending_pilot_plan.is_none(),
        "brain plan should resolve before the deadline"
    );

    let run = app
        .last_pilot_run
        .as_ref()
        .expect("a structured pilot run artifact should be recorded");
    assert_eq!(
        run.planner, "local-model",
        "plan should come from the brain"
    );
    assert!(
        run.plan.iter().any(|step| step.kind == "navigate"),
        "brain plan should include a navigate step; got {:?}",
        run.plan
    );

    let _ = std::fs::remove_dir_all(&data_dir);
    Ok(())
}

#[cfg(feature = "xilem-shell")]
#[test]
fn settings_backend_selection_persists_and_rebuilds() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-ai-settings-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    // Selecting Ollama applies its default endpoint/model deterministically and
    // rebuilds the brain lane without error.
    app.apply_ai_backend("ollama");
    assert!(
        app.last_ok,
        "backend switch should succeed: {}",
        app.last_status
    );
    assert_eq!(app.ai_config.backend, "ollama");
    assert_eq!(app.ai_config.endpoint, "http://127.0.0.1:11434");

    // Persisted to <profile>/ai-provider.json (read directly to avoid env overrides).
    let raw = std::fs::read_to_string(AiLocalConfig::config_path(&app.profile_data_dir))
        .map_err(|error| error.to_string())?;
    let saved: AiLocalConfig = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    assert_eq!(saved.backend, "ollama");
    assert_eq!(saved.model, "qwen2.5:3b");

    // Switching back to llama.cpp restores the on-box 32B defaults.
    app.apply_ai_backend("llamacpp");
    assert_eq!(app.ai_config.endpoint, "http://127.0.0.1:8101");
    assert_eq!(app.ai_config.model, "qwen2.5-coder-32b");

    let _ = std::fs::remove_dir_all(&data_dir);
    Ok(())
}

#[test]
fn pilot_status_color_maps_states() {
    assert_eq!(pilot_status_color("IDLE"), STATUS_OK);
    assert_eq!(pilot_status_color("PILOT COMPLETE"), STATUS_OK);
    assert_eq!(pilot_status_color("CONSENT AUTHORIZED"), STATUS_OK);
    assert_eq!(pilot_status_color("FAILED"), STATUS_ERROR);
    assert_eq!(pilot_status_color("BLOCKED"), STATUS_WARN);
    assert_eq!(pilot_status_color("AWAITING CONSENT"), STATUS_WARN);
    assert_eq!(pilot_status_color("PILOT REASONING"), STATUS_INFO);
    assert_eq!(pilot_status_color("PILOT NAVIGATING"), STATUS_INFO);
}

#[cfg(feature = "xilem-shell")]
#[test]
fn preset_intent_mirrors_address_and_shows_browser() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-preset-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.set_browser_mode(BrowserMode::Agent);

    app.run_preset_intent(AI_RAIL_PRESETS[0].1);
    assert_eq!(app.address_input.as_str(), AI_RAIL_PRESETS[0].1);
    assert!(app.main_view == MainView::Browser);

    let _ = std::fs::remove_dir_all(&data_dir);
    Ok(())
}

#[test]
fn browser_shell_can_authorize_pending_pilot_consent() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-authorize-consent-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    app.pending_consent = Some(PendingConsent {
        intent: "buy example.com and checkout".to_string(),
        message: "Authorize this browser action before execution.".to_string(),
        #[cfg(feature = "xilem-shell")]
        remaining_actions: Vec::new(),
    });
    app.pilot_status = "AWAITING CONSENT".to_string();
    assert_eq!(app.pilot_status, "AWAITING CONSENT");

    app.authorize_pilot_consent();
    assert_eq!(app.pilot_status, "CONSENT AUTHORIZED");
    assert!(app.pending_consent.is_none());
    app.refresh_logs();
    assert!(app.recent_logs.iter().any(|entry| {
        entry.intent.starts_with("pilot consent authorize ") && entry.consent_signature.is_some()
    }));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_consent_controls_authorize_pending_request() -> Result<(), String> {
    let data_dir =
        env::temp_dir().join(format!("sextant-browser-click-consent-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));

    app.pending_consent = Some(PendingConsent {
        intent: "buy example.com and checkout".to_string(),
        message: "Authorize this browser action before execution.".to_string(),
        #[cfg(feature = "xilem-shell")]
        remaining_actions: Vec::new(),
    });
    app.pilot_status = "AWAITING CONSENT".to_string();
    let authorize_rect = app.consent_authorize_rect;
    click_rect(&mut app, authorize_rect, "authorize consent")?;

    assert_eq!(app.pilot_status, "CONSENT AUTHORIZED");
    assert!(app.pending_consent.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[cfg(feature = "xilem-shell")]
#[test]
fn authorizing_consent_resumes_pending_pilot_actions() -> Result<(), String> {
    let data_dir =
        env::temp_dir().join(format!("sextant-browser-resume-consent-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.pending_consent = Some(PendingConsent {
        intent: "buy example.com and checkout".to_string(),
        message: "Authorize this browser action before execution.".to_string(),
        remaining_actions: vec![PilotAction::Analyze(
            "Resumed gated browser plan.".to_string(),
        )],
    });
    app.pilot_status = "AWAITING CONSENT".to_string();

    app.authorize_pilot_consent();

    assert_eq!(app.pilot_status, "PILOT COMPLETE");
    assert!(app.pending_consent.is_none());
    assert_eq!(app.pilot_result, "Resumed gated browser plan.");
    app.refresh_logs();
    assert!(app
        .recent_logs
        .iter()
        .any(|entry| entry.intent.starts_with("pilot consent resume ")
            && entry.consent_signature.is_some()));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn maps_browser_chrome_shortcuts() {
    let control = ModifiersState::CONTROL;
    let alt = ModifiersState::ALT;

    assert_eq!(
        browser_shortcut_for_key(control, &Key::Character("l".into())),
        Some(BrowserShortcut::FocusAddress)
    );
    assert_eq!(
        browser_shortcut_for_key(control, &Key::Character("T".into())),
        Some(BrowserShortcut::NewTab)
    );
    assert_eq!(
        browser_shortcut_for_key(control, &Key::Character("w".into())),
        Some(BrowserShortcut::CloseTab)
    );
    assert_eq!(
        browser_shortcut_for_key(control, &Key::Character("r".into())),
        Some(BrowserShortcut::Reload)
    );
    assert_eq!(
        browser_shortcut_for_key(alt, &Key::Named(NamedKey::ArrowLeft)),
        Some(BrowserShortcut::Back)
    );
    assert_eq!(
        browser_shortcut_for_key(alt, &Key::Named(NamedKey::ArrowRight)),
        Some(BrowserShortcut::Forward)
    );
    assert_eq!(
        browser_shortcut_for_key(control | alt, &Key::Character("l".into())),
        None
    );
}

#[test]
fn builds_window_icon_for_visible_browser() {
    assert!(sextant_window_icon().is_some());
}

#[test]
fn wake_search_label_and_field_focus_input() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-wake-focus-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    app.wake_query.clear();

    let hit = app.wake_input_hit_rect();
    app.click((hit.x + 8) as f64, (hit.y + 8) as f64);
    assert!(matches!(app.focus, FocusTarget::Wake));
    app.focused_text_mut().push_str("local ai");
    assert_eq!(app.wake_query, "local ai");
    assert!(app.last_status.contains("Wake search focused"));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn page_tab_strip_keeps_active_overflow_tab_visible() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-tab-overflow-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let first_tab = app.active_tab().expect("initial tab").id;

    for _ in 0..MAX_VISIBLE_PAGE_TABS {
        app.new_tab();
    }
    app.layout(PhysicalSize::new(1180, 760));

    let active_tab = app.active_tab().expect("active tab").id;
    assert!(
        app.page_tab_rects
            .iter()
            .any(|region| region.tab_id == active_tab),
        "new active tab should stay visible when the strip overflows"
    );
    assert!(
        !app.page_tab_rects
            .iter()
            .any(|region| region.tab_id == first_tab),
        "overflowed strip should window around the active tab"
    );

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn page_tab_strip_pager_reaches_hidden_tabs() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-tab-pager-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let first_tab = app.active_tab().expect("initial tab").id;

    for _ in 0..(MAX_VISIBLE_PAGE_TABS + 2) {
        app.new_tab();
    }
    app.layout(PhysicalSize::new(1180, 760));
    assert!(
        !app.page_tab_rects
            .iter()
            .any(|region| region.tab_id == first_tab),
        "first tab should start outside the active overflow window"
    );

    while app.can_page_tabs_previous() {
        let previous_rect = app.page_tab_prev_rect;
        click_rect(&mut app, previous_rect, "previous tab pager")?;
    }

    assert!(
        app.page_tab_rects
            .iter()
            .any(|region| region.tab_id == first_tab),
        "previous pager should make the hidden first tab visible"
    );

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn page_tab_range_label_does_not_overlap_controls() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-tab-label-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    for _ in 0..(MAX_VISIBLE_PAGE_TABS + 2) {
        app.new_tab();
    }
    app.layout(PhysicalSize::new(2400, 900));
    let (label_rect, _) = page_tab_range_label_rect(&app)
        .ok_or_else(|| "wide overflow strip should have room for a range label".to_string())?;

    assert!(
        !label_rect.intersects(app.page_tab_next_rect),
        "range label should not overlap the next pager"
    );
    for region in &app.page_tab_rects {
        assert!(
            !label_rect.intersects(region.rect),
            "range label should not overlap visible tab labels"
        );
    }

    app.layout(PhysicalSize::new(900, 620));
    if let Some((narrow_label_rect, _)) = page_tab_range_label_rect(&app) {
        assert!(
            !narrow_label_rect.intersects(app.page_tab_next_rect),
            "narrow range label should not overlap the next pager"
        );
        for region in &app.page_tab_rects {
            assert!(
                !narrow_label_rect.intersects(region.rect),
                "narrow range label should not overlap visible tabs"
            );
        }
    }

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn page_load_bar_sits_between_tabs_and_metrics() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-load-bar-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    let track = page_load_bar_rect(app.window_size.width);
    let tab_strip_bottom = CHROME_H + STRIP_H + TAB_H + PAGE_TAB_H;

    assert!(track.y >= tab_strip_bottom);
    assert!(track.y + track.h <= METRIC_Y);
    assert!(page_load_bar_active_rect(&app, track).is_none());

    let (_result_tx, result_rx) = mpsc::channel();
    let url = Url::parse("https://example.com").map_err(|error| error.to_string())?;
    app.pending_navigation = Some(PendingNavigation {
        url,
        kind: PendingNavigationKind::Open,
        result_rx,
        started: Instant::now() - Duration::from_millis(80),
        viewport_size: None,
    });
    let active = page_load_bar_active_rect(&app, track)
        .ok_or_else(|| "pending navigation should expose a load gauge segment".to_string())?;
    assert!(active.x >= track.x);
    assert!(active.x + active.w <= track.x + track.w);
    assert_eq!(active.y, track.y);
    assert_eq!(active.h, track.h);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn records_perf_events_and_caps_history() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-perf-history-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    for millis in 0..(PERF_HISTORY_LIMIT + 2) {
        app.record_perf("frame", Duration::from_millis(millis as u64), "capture");
    }

    assert_eq!(app.perf_events.len(), PERF_HISTORY_LIMIT);
    assert_eq!(
        app.perf_events.first().map(|event| event.duration),
        Some(Duration::from_millis(2))
    );
    assert_eq!(
        app.perf.frame,
        Some(Duration::from_millis((PERF_HISTORY_LIMIT + 1) as u64))
    );

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn navigation_phase_timings_are_recorded_without_overwriting_summary() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-nav-phase-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.record_perf("navigation", Duration::from_millis(120), "open async");

    app.record_navigation_timings(
        &AsyncNavigationTimings {
            firewall: Some(Duration::from_millis(2)),
            servo_navigation: Some(Duration::from_millis(95)),
            servo_url_wait: Some(Duration::from_millis(55)),
            servo_load_wait: Some(Duration::from_millis(35)),
            servo_inspect: Some(Duration::from_millis(7)),
            reader_fallback: None,
        },
        PendingNavigationKind::Open,
    );

    assert_eq!(app.perf.navigation, Some(Duration::from_millis(120)));
    assert_eq!(app.perf.resize, None);
    assert!(app
        .perf_events
        .iter()
        .any(|event| event.phase == "nav-phase"
            && event.label == "open servo navigation"
            && event.duration == Duration::from_millis(95)));
    assert!(app
        .perf_events
        .iter()
        .any(|event| event.phase == "nav-phase"
            && event.label == "open servo url wait"
            && event.duration == Duration::from_millis(55)));
    assert!(app
        .perf_events
        .iter()
        .any(|event| event.phase == "nav-phase"
            && event.label == "open servo load wait"
            && event.duration == Duration::from_millis(35)));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn slowest_perf_event_tracks_largest_duration() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-perf-slowest-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    app.record_perf("navigation", Duration::from_millis(100), "open");
    app.record_perf("frame", Duration::from_millis(20), "capture");
    app.record_perf("distill", Duration::from_millis(300), "active tab");

    let slowest = app
        .slowest_perf_event()
        .ok_or_else(|| "missing slowest perf event".to_string())?;
    assert_eq!(slowest.phase, "distill");
    assert_eq!(slowest.duration, Duration::from_millis(300));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn slowest_perf_events_are_sorted_and_limited() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-perf-top-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    app.record_perf("navigation", Duration::from_millis(100), "open");
    app.record_perf(
        "nav-phase",
        Duration::from_millis(95),
        "open servo navigation",
    );
    app.record_perf(
        "resize",
        Duration::from_millis(430),
        "viewport render bridge async",
    );
    app.record_perf("frame", Duration::from_millis(44), "capture render bridge");

    let labels = app
        .slowest_perf_events(3)
        .iter()
        .map(|event| event.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        vec![
            "viewport render bridge async",
            "open",
            "open servo navigation"
        ]
    );

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_distillation_can_complete_from_worker() -> Result<(), String> {
    let data_dir =
        env::temp_dir().join(format!("sextant-browser-async-distill-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let url = Url::parse(
        "data:text/html,<title>Async Distill</title><main><h1>Async Distill Ready</h1></main>",
    )
    .map_err(|error| error.to_string())?;

    app.navigate_to(url);
    app.distill_active();
    assert!(
        app.last_ok,
        "sync setup distill failed: {}",
        app.last_status
    );
    app.last_status.clear();
    assert!(app.start_visible_distillation());
    assert!(app.pending_distillation.is_some());

    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_distillation.is_some() && Instant::now() < deadline {
        let _ = app.collect_pending_distillation();
        thread::sleep(Duration::from_millis(10));
    }
    while app.pending_persistence.is_some() && Instant::now() < deadline {
        let _ = app.collect_pending_persistence();
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        app.pending_distillation.is_none(),
        "async distillation did not complete"
    );
    assert!(
        app.pending_persistence.is_none(),
        "persistence lane did not complete"
    );
    assert!(app.last_ok, "async distill failed: {}", app.last_status);
    assert!(app.perf.distill.is_some());
    assert!(app.perf.wake.is_some());
    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.as_ref())
        .ok_or_else(|| "async distill did not attach page".to_string())?;
    assert!(page.content.contains("Async Distill Ready"));
    assert!(!app.wake_results.is_empty());
    assert!(!app.recent_logs.is_empty());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_wake_search_can_complete_from_persistence_lane() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-async-wake-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let url = Url::parse(
        "data:text/html,<title>Async Wake</title><main><h1>Async Wake Needle</h1></main>",
    )
    .map_err(|error| error.to_string())?;

    app.navigate_to(url);
    app.distill_active();
    assert!(
        app.last_ok,
        "sync setup distill failed: {}",
        app.last_status
    );
    app.wake_results.clear();
    app.recent_logs.clear();
    app.wake_query = "Needle".to_string();

    assert!(app.start_visible_wake_search());
    assert!(app.pending_wake_search.is_some());
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_wake_search.is_some() && Instant::now() < deadline {
        let _ = app.collect_pending_wake_search();
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        app.pending_wake_search.is_none(),
        "Wake search lane did not complete"
    );
    assert!(app.last_ok, "Wake search failed: {}", app.last_status);
    assert!(app.perf.wake.is_some());
    assert!(!app.wake_results.is_empty());
    assert!(app
        .recent_logs
        .iter()
        .any(|entry| entry.intent.contains("search Wake 'Needle'")));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_log_write_can_complete_from_persistence_lane() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-async-log-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.defer_user_navigation = true;

    app.record_log("visible log lane", LogStatus::Success)?;
    assert_eq!(app.pending_log_writes.len(), 1);
    assert!(app.recent_logs.is_empty());

    let deadline = Instant::now() + Duration::from_secs(5);
    while !app.pending_log_writes.is_empty() && Instant::now() < deadline {
        let _ = app.collect_pending_log_writes();
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        app.pending_log_writes.is_empty(),
        "log lane did not complete"
    );
    assert!(app.validation.log_seen);
    assert!(app
        .recent_logs
        .iter()
        .any(|entry| entry.intent == "visible log lane"));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_log_refresh_can_complete_from_persistence_lane() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-async-log-refresh-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    app.record_log("visible log refresh lane", LogStatus::Success)?;
    app.defer_user_navigation = true;
    app.recent_logs.clear();
    app.validation.log_seen = false;

    app.refresh_logs();
    assert!(app.pending_log_refresh.is_some());
    assert!(app.recent_logs.is_empty());

    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_log_refresh.is_some() && Instant::now() < deadline {
        let _ = app.collect_pending_log_refresh();
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        app.pending_log_refresh.is_none(),
        "log refresh lane did not complete"
    );
    assert!(app.validation.log_seen);
    assert!(app
        .recent_logs
        .iter()
        .any(|entry| entry.intent == "visible log refresh lane"));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn perf_tab_switches_to_performance_view() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-perf-tab-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));

    let perf_rect = app.perf_tab_rect;
    click_rect(&mut app, perf_rect, "Perf tab")?;
    assert!(app.main_view == MainView::Perf);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_navigation_can_defer_user_open() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-deferred-navigation-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.defer_user_navigation = true;
    app.address_input = "https://example.com".to_string();

    app.navigate_input();

    assert_eq!(
        app.pending_user_navigation.as_deref(),
        Some("https://example.com")
    );
    assert!(app.last_status.contains("Opening https://example.com"));
    assert!(app.last_ok);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_navigation_can_defer_user_reload() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-deferred-reload-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.defer_user_navigation = true;

    app.run_action(Action::Reload);

    assert!(matches!(app.pending_user_action, Some(Action::Reload)));
    assert_eq!(app.last_status, "Reloading active tab...");
    assert!(app.last_ok);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_navigation_control_requires_tab_url() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-control-url-required-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    assert!(!app.start_visible_navigation_control(PendingNavigationKind::Reload));
    assert!(app.pending_navigation.is_none());
    assert!(app.last_status.contains("active tab has no URL"));
    assert!(!app.last_ok);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[cfg(feature = "servo-backend")]
#[test]
fn visible_reload_can_complete_from_navigation_worker() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-async-reload-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    app.navigate_to(
        Url::parse("data:text/html,<title>Async Reload</title><main>ready</main>")
            .map_err(|error| error.to_string())?,
    );
    assert!(app.last_ok, "setup navigation failed: {}", app.last_status);

    assert!(app.start_visible_navigation_control(PendingNavigationKind::Reload));
    assert!(matches!(
        app.pending_navigation.as_ref().map(|pending| pending.kind),
        Some(PendingNavigationKind::Reload)
    ));

    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_navigation.is_some() && Instant::now() < deadline {
        let _ = app.collect_pending_navigation();
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        app.pending_navigation.is_none(),
        "async reload did not complete"
    );
    assert!(app.last_ok, "async reload failed: {}", app.last_status);
    assert!(app.last_status.contains("Reloaded active tab"));
    assert!(app.perf.navigation.is_some());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn completed_visible_navigation_queues_first_frame_capture() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-nav-first-frame-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.defer_user_navigation = true;
    app.layout(PhysicalSize::new(1180, 760));
    let tab_id = app
        .active_tab()
        .map(|tab| tab.id)
        .ok_or_else(|| "missing active tab".to_string())?;
    let url = Url::parse("https://example.com").map_err(|error| error.to_string())?;
    let (result_tx, result_rx) = mpsc::channel();
    result_tx
        .send(Ok(AsyncNavigationResult {
            tab_id,
            requested_url: url.clone(),
            final_url: url.clone(),
            status: EngineStatus {
                active_backend: EngineBackend::Servo,
                is_sandboxed: true,
                memory_usage_mb: 180,
                gpu_accelerated: false,
                sandbox_profile: None,
                firewall_status: None,
                layout_time_ms: 0.0,
                parallel_threads: 8,
            },
            distilled_page: None,
            can_go_back: false,
            can_go_forward: false,
            timings: AsyncNavigationTimings {
                firewall: Some(Duration::from_millis(1)),
                servo_navigation: Some(Duration::from_millis(12)),
                servo_url_wait: Some(Duration::from_millis(8)),
                servo_load_wait: Some(Duration::from_millis(4)),
                servo_inspect: Some(Duration::from_millis(2)),
                reader_fallback: None,
            },
            initial_frame: None,
        }))
        .map_err(|error| error.to_string())?;
    app.pending_navigation = Some(PendingNavigation {
        url: url.clone(),
        kind: PendingNavigationKind::Open,
        result_rx,
        started: Instant::now(),
        viewport_size: None,
    });

    assert!(app.collect_pending_navigation().is_some());
    assert!(app.pending_navigation.is_none());
    assert!(app.pending_frame_capture.is_some());
    assert_eq!(app.frame_refresh_budget, FRAME_WARMUP_BUDGET);
    assert!(app
        .perf_events
        .iter()
        .any(|event| event.phase == "nav-phase" && event.label == "open servo navigation"));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn completed_visible_navigation_applies_initial_frame() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-nav-initial-frame-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.defer_user_navigation = true;
    app.layout(PhysicalSize::new(1180, 760));
    let viewport = app.browser_viewport_rect;
    let viewport_size = (viewport.w.max(1), viewport.h.max(1));
    let tab_id = app
        .active_tab()
        .map(|tab| tab.id)
        .ok_or_else(|| "missing active tab".to_string())?;
    let url = Url::parse("https://example.com").map_err(|error| error.to_string())?;
    let (result_tx, result_rx) = mpsc::channel();
    result_tx
        .send(Ok(AsyncNavigationResult {
            tab_id,
            requested_url: url.clone(),
            final_url: url.clone(),
            status: EngineStatus {
                active_backend: EngineBackend::Servo,
                is_sandboxed: true,
                memory_usage_mb: 180,
                gpu_accelerated: false,
                sandbox_profile: None,
                firewall_status: None,
                layout_time_ms: 0.0,
                parallel_threads: 8,
            },
            distilled_page: None,
            can_go_back: false,
            can_go_forward: false,
            timings: AsyncNavigationTimings::default(),
            initial_frame: Some(Ok(AsyncFrameCapture {
                queue: Duration::from_millis(3),
                resize: None,
                capture: Duration::from_millis(8),
                frame: RenderedFrame {
                    width: viewport_size.0,
                    height: viewport_size.1,
                    pixels: vec![0x00ff00; (viewport_size.0 * viewport_size.1) as usize],
                },
            })),
        }))
        .map_err(|error| error.to_string())?;
    app.pending_navigation = Some(PendingNavigation {
        url,
        kind: PendingNavigationKind::Open,
        result_rx,
        started: Instant::now(),
        viewport_size: None,
    });

    assert!(app.collect_pending_navigation().is_some());
    assert!(app.pending_frame_capture.is_none());
    assert_eq!(app.last_frame_viewport, Some(viewport_size));
    assert_eq!(app.perf.resize, Some(Duration::ZERO));
    assert_eq!(app.perf.frame, Some(Duration::from_millis(8)));
    assert!(app.latest_frame.is_some());
    assert!(app.pending_observation_warmup.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn completed_background_navigation_does_not_replace_active_frame() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-background-navigation-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.defer_user_navigation = true;
    app.layout(PhysicalSize::new(1180, 760));
    let first_tab = app
        .active_tab()
        .map(|tab| tab.id)
        .ok_or_else(|| "missing active tab".to_string())?;
    let url = Url::parse("https://example.com").map_err(|error| error.to_string())?;
    let (result_tx, result_rx) = mpsc::channel();
    result_tx
        .send(Ok(AsyncNavigationResult {
            tab_id: first_tab,
            requested_url: url.clone(),
            final_url: url.clone(),
            status: EngineStatus {
                active_backend: EngineBackend::Servo,
                is_sandboxed: true,
                memory_usage_mb: 180,
                gpu_accelerated: false,
                sandbox_profile: None,
                firewall_status: None,
                layout_time_ms: 0.0,
                parallel_threads: 8,
            },
            distilled_page: None,
            can_go_back: false,
            can_go_forward: false,
            timings: AsyncNavigationTimings::default(),
            initial_frame: Some(Ok(AsyncFrameCapture {
                queue: Duration::from_millis(1),
                resize: None,
                capture: Duration::from_millis(5),
                frame: RenderedFrame {
                    width: 320,
                    height: 200,
                    pixels: vec![0x00ff00; 320 * 200],
                },
            })),
        }))
        .map_err(|error| error.to_string())?;
    app.pending_navigation = Some(PendingNavigation {
        url: url.clone(),
        kind: PendingNavigationKind::Open,
        result_rx,
        started: Instant::now(),
        viewport_size: None,
    });

    let second_tab = app.engine.open_tab();
    app.engine.switch_to_tab(second_tab)?;
    app.sync_address_to_active_tab();
    assert!(app.collect_pending_navigation().is_some());

    assert_eq!(app.active_tab().map(|tab| tab.id), Some(second_tab));
    assert_eq!(app.address_input, "about:blank");
    assert!(app.latest_frame.is_none());
    assert!(app.pending_frame_capture.is_none());
    assert_eq!(
        app.engine
            .get_tabs()
            .iter()
            .find(|tab| tab.id == first_tab)
            .and_then(|tab| tab.url.as_ref()),
        Some(&url)
    );
    assert!(app.last_status.contains("Background tab"));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn observation_warmup_respects_direct_mode_boundary() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-observation-warmup-boundary-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.new_tab();
    app.observation_warmup_enabled = true;
    app.set_browser_mode(BrowserMode::Assisted);
    assert!(app.schedule_observation_warmup_if_allowed());
    assert!(app.scheduled_observation_warmup.is_some());

    app.set_browser_mode(BrowserMode::Direct);
    assert!(app.scheduled_observation_warmup.is_none());

    assert!(!app.schedule_observation_warmup_if_allowed());
    assert!(app.scheduled_observation_warmup.is_none());
    assert!(app.pending_observation_warmup.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn observation_warmup_waits_for_foreground_work() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-observation-warmup-idle-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.new_tab();
    app.observation_warmup_enabled = true;
    app.set_browser_mode(BrowserMode::Assisted);

    assert!(app.schedule_observation_warmup_if_allowed());
    app.scheduled_observation_warmup
        .as_mut()
        .expect("scheduled warmup")
        .due = Instant::now() - Duration::from_millis(1);
    app.pending_browser_text = "typing".to_string();

    assert!(!app.maybe_start_scheduled_observation_warmup());
    assert!(app.scheduled_observation_warmup.is_some());
    assert!(app.pending_observation_warmup.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn visible_navigation_can_defer_user_distill() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-deferred-distill-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.defer_user_navigation = true;

    app.run_action(Action::DistillActive);

    assert!(matches!(
        app.pending_user_action,
        Some(Action::DistillActive)
    ));
    assert_eq!(app.last_status, "Distilling active page...");
    assert!(app.last_ok);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn viewport_wheel_queues_frame_warmup() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-wheel-warmup-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    app.navigate_to(
        Url::parse("data:text/html,<body style='height:2000px'>wheel</body>")
            .map_err(|error| error.to_string())?,
    );
    assert!(app.last_ok);
    app.latest_frame = Some(RenderedFrame {
        width: 2,
        height: 2,
        pixels: vec![0; 4],
    });
    app.frame_refresh_budget = 0;
    app.frame_dirty = false;
    let (x, y) = rect_center(app.browser_viewport_rect);

    app.scroll_at(
        x,
        y,
        &MouseScrollDelta::LineDelta(0.0, -1.0),
        app.window_size,
    );

    assert!(app.validation.browser_input_seen);
    assert!(app.frame_dirty);
    assert_eq!(app.frame_refresh_budget, FRAME_INTERACTION_WARMUP_BUDGET);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn frame_refresh_waits_for_foreground_servo_work() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-frame-refresh-foreground-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.new_tab();
    app.latest_frame = Some(RenderedFrame {
        width: 10,
        height: 10,
        pixels: vec![0; 100],
    });
    app.frame_refresh_budget = 1;
    app.frame_dirty = true;
    app.last_frame_refresh = Instant::now() - FRAME_REFRESH_DIRTY;
    let (_tx, result_rx) = mpsc::channel();
    app.pending_distillation = Some(PendingDistillation {
        result_rx,
        started: Instant::now(),
    });

    assert!(!app.maybe_refresh_frame());
    assert!(app.pending_frame_capture.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn viewport_mouse_move_forwarding_is_throttled() -> Result<(), String> {
    let data_dir =
        env::temp_dir().join(format!("sextant-browser-mouse-throttle-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    assert!(app.should_forward_browser_mouse_move(20.0, 20.0));
    app.last_viewport_mouse_move_forward = Some(Instant::now());
    app.last_viewport_mouse_move_point = Some((20.0, 20.0));

    assert!(!app.should_forward_browser_mouse_move(120.0, 120.0));
    app.last_viewport_mouse_move_forward =
        Some(Instant::now() - VIEWPORT_MOUSE_MOVE_MIN_INTERVAL - Duration::from_millis(1));
    assert!(!app
        .should_forward_browser_mouse_move(20.0 + VIEWPORT_MOUSE_MOVE_MIN_DISTANCE_PX / 2.0, 20.0));
    assert!(app.should_forward_browser_mouse_move(20.0 + VIEWPORT_MOUSE_MOVE_MIN_DISTANCE_PX, 20.0));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn viewport_text_input_flushes_without_debounce() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-key-debounce-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let active_tab = app.active_tab().expect("active tab").id;

    app.queue_browser_text("a");
    app.queue_browser_text("b");

    assert_eq!(app.pending_browser_text, "ab");
    assert_eq!(app.pending_browser_text_tab, Some(active_tab));
    assert_eq!(app.frame_refresh_budget, 0);
    assert!(!app.frame_dirty);
    assert!(app.pending_browser_text_due().is_some());
    assert!(app.flush_pending_browser_text_if_due());
    assert!(app.pending_browser_text.is_empty());
    assert_eq!(app.pending_browser_text_tab, None);
    assert!(matches!(
        app.pending_viewport_input.back(),
        Some(ViewportInputEvent::Text { text }) if text == "ab"
    ));
    assert_eq!(app.frame_refresh_budget, FRAME_INTERACTION_WARMUP_BUDGET);
    assert!(app.frame_dirty);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn viewport_text_input_does_not_follow_tab_switch() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-key-tab-isolation-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let first_tab = app.active_tab().expect("active tab").id;

    app.queue_browser_text("search");
    assert_eq!(app.pending_browser_text_tab, Some(first_tab));

    let second_tab = app.engine.open_tab();
    app.engine.switch_to_tab(second_tab)?;
    assert!(!app.flush_pending_browser_text());
    assert!(app.pending_browser_text.is_empty());
    assert!(app.pending_browser_text_tab.is_none());
    assert!(app.pending_viewport_input.is_empty());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn viewport_input_lane_coalesces_before_engine_enqueue() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-input-lane-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    assert!(app.queue_viewport_input(ViewportInputEvent::MouseMove { x: 10.0, y: 12.0 }));
    assert!(app.queue_viewport_input(ViewportInputEvent::MouseMove { x: 20.0, y: 24.0 }));
    assert!(app.queue_viewport_input(ViewportInputEvent::Wheel {
        delta_x: 1.0,
        delta_y: 2.0,
        pixel_mode: false,
    }));
    assert!(app.queue_viewport_input(ViewportInputEvent::Wheel {
        delta_x: 3.0,
        delta_y: 4.0,
        pixel_mode: false,
    }));
    assert!(app.queue_viewport_input(ViewportInputEvent::Text {
        text: "a".to_string(),
    }));
    assert!(app.queue_viewport_input(ViewportInputEvent::Text {
        text: "b".to_string(),
    }));

    assert_eq!(app.pending_viewport_input.len(), 3);
    match app.pending_viewport_input.get(0) {
        Some(ViewportInputEvent::MouseMove { x, y }) => assert_eq!((*x, *y), (20.0, 24.0)),
        _ => panic!("expected coalesced mouse move"),
    }
    match app.pending_viewport_input.get(1) {
        Some(ViewportInputEvent::Wheel {
            delta_x, delta_y, ..
        }) => assert_eq!((*delta_x, *delta_y), (4.0, 6.0)),
        _ => panic!("expected coalesced wheel"),
    }
    match app.pending_viewport_input.get(2) {
        Some(ViewportInputEvent::Text { text }) => assert_eq!(text, "ab"),
        _ => panic!("expected coalesced text"),
    }
    assert!(app.validation.browser_input_seen);
    assert!(app.frame_dirty);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn viewport_input_lane_drops_stale_tab_input() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-input-lane-stale-tab-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let first_tab_id = app.active_tab().map(|tab| tab.id).unwrap();

    assert!(app.queue_viewport_input(ViewportInputEvent::MouseMove { x: 10.0, y: 12.0 }));
    assert_eq!(app.pending_viewport_input_tab, Some(first_tab_id));
    assert_eq!(app.pending_viewport_input.len(), 1);

    let second_tab_id = app.engine.open_tab();
    app.engine.switch_to_tab(second_tab_id)?;

    assert!(!app.flush_viewport_input_lane());
    assert!(app.pending_viewport_input.is_empty());
    assert!(app.pending_viewport_input_tab.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn frame_refresh_waits_for_recent_viewport_input_flush() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-frame-refresh-input-settle-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.latest_frame = Some(RenderedFrame {
        width: 10,
        height: 10,
        pixels: vec![0; 100],
    });
    app.frame_refresh_budget = 1;
    app.frame_dirty = true;
    app.last_frame_refresh = Instant::now() - FRAME_REFRESH_INTERACTION;
    app.last_viewport_input_flush = Some(Instant::now());

    assert!(app
        .viewport_input_capture_delay()
        .is_some_and(|delay| delay <= VIEWPORT_INPUT_CAPTURE_SETTLE));
    assert!(!app.maybe_refresh_frame());
    assert!(app.pending_frame_capture.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn frame_refresh_uses_input_settle_deadline_for_interaction_capture() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-frame-refresh-input-deadline-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.latest_frame = Some(RenderedFrame {
        width: 10,
        height: 10,
        pixels: vec![0; 100],
    });
    app.frame_refresh_budget = 1;
    app.frame_dirty = true;
    app.last_frame_refresh = Instant::now();
    app.last_viewport_input_flush =
        Some(Instant::now() - VIEWPORT_INPUT_CAPTURE_SETTLE - Duration::from_millis(1));

    assert_eq!(app.viewport_input_capture_delay(), Some(Duration::ZERO));
    assert!(!app.maybe_refresh_frame());
    assert!(app.pending_frame_capture.is_some());
    assert!(app.last_viewport_input_flush.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn tab_changes_drop_stale_pending_frame_capture() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-stale-frame-capture-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let tab_id = app.active_tab().map(|tab| tab.id).unwrap();
    let (_reply_tx, result_rx) = mpsc::channel();

    app.pending_frame_capture = Some(PendingFrameCapture {
        tab_id,
        result_rx,
        started: Instant::now(),
        viewport_size: (640, 480),
        requested_resize: true,
        purpose: FrameCapturePurpose::RenderBridge,
    });

    assert!(app.drop_pending_frame_capture_for_tab_change());
    assert!(app.pending_frame_capture.is_none());
    assert!(!app.drop_pending_frame_capture_for_tab_change());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn small_window_browser_viewport_stays_inside_window() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-small-window-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let size = PhysicalSize::new(520, 520);

    app.layout(size);

    let viewport = app.browser_viewport_rect;
    assert!(
        viewport.x.saturating_add(viewport.w) <= size.width,
        "viewport should not overflow window width: {:?}",
        viewport
    );
    assert!(
        viewport.y.saturating_add(viewport.h) <= size.height.saturating_sub(STATUS_BAR_H),
        "viewport should not overflow visible status area: {:?}",
        viewport
    );
    assert_eq!(right_rail_x(size.width), size.width);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn browser_modes_gate_ai_observation_and_persistence() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-modes-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    assert!(app.capabilities().ai_observe_dom);
    assert!(app.action_enabled(Action::DistillActive));
    assert!(app.native_intent_allowed());

    app.set_browser_mode(BrowserMode::Direct);
    assert!(!app.capabilities().ai_observe_dom);
    assert!(!app.capabilities().write_wake);
    assert!(!app.capabilities().write_log_content);
    assert!(!app.action_enabled(Action::DistillActive));
    assert!(!app.action_enabled(Action::SearchWake));
    assert!(!app.native_intent_allowed());
    assert!(!app.action_enabled(Action::RunShowcase));
    assert_eq!(app.ai_status_label(), "OFF");
    assert_eq!(app.storage_status_label(), "PROFILE");
    assert!(app
        .disabled_reason(Action::DistillActive)
        .contains("blocks AI page observation"));
    assert!(app
        .disabled_reason(Action::RunShowcase)
        .contains("blocks proof workflows"));
    assert!(app.frame_capture_allowed(FrameCapturePurpose::RenderBridge));
    assert!(!app.frame_capture_allowed(FrameCapturePurpose::AiObservation));
    app.record_log("direct mode log", LogStatus::Success)?;
    assert!(!app.validation.log_seen);

    app.set_browser_mode(BrowserMode::Agent);
    assert!(app.capabilities().ai_control);
    assert!(app.capabilities().write_wake);
    assert!(app.capabilities().write_log_content);
    assert!(app.frame_capture_allowed(FrameCapturePurpose::AiObservation));
    assert!(app.action_enabled(Action::RunShowcase));
    assert_eq!(app.ai_status_label(), "CONTROL");

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn validation_rows_explain_mode_disabled_features() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-mode-validation-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    app.set_browser_mode(BrowserMode::Incognito);
    assert_eq!(app.storage_status_label(), "EPHEMERAL");
    let rows = app.validation_rows();

    let distill = rows.iter().find(|row| row.label == "DISTILL").unwrap();
    assert!(matches!(distill.status, ValidationStatus::Attention));
    assert!(distill.detail.contains("disables AI page observation"));

    let wake = rows.iter().find(|row| row.label == "WAKE").unwrap();
    assert!(matches!(wake.status, ValidationStatus::Attention));
    assert!(wake.detail.contains("disables Wake reads"));

    let log = rows.iter().find(|row| row.label == "CAPTAIN LOG").unwrap();
    assert!(matches!(log.status, ValidationStatus::Attention));
    assert!(log.detail.contains("disables content log persistence"));

    app.cleanup_incognito_storage();
    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn native_intents_stop_at_non_control_mode_boundary() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-intent-mode-block-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    for mode in [
        BrowserMode::Observe,
        BrowserMode::Direct,
        BrowserMode::Incognito,
    ] {
        app.set_browser_mode(mode);
        app.run_native_intent_sync("intent: open https://example.com and distill");
        assert!(!app.last_ok);
        assert_eq!(app.pilot_status, "BLOCKED");
        assert!(app.last_status.contains("blocks native intents"));
    }

    app.set_browser_mode(BrowserMode::Assisted);
    assert!(app.native_intent_allowed());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn proof_workflows_stop_at_non_control_mode_boundary() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-proof-mode-block-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;

    for (mode, label) in [
        (BrowserMode::Observe, "launch showcase"),
        (BrowserMode::Direct, "real browsing smoke"),
        (BrowserMode::Incognito, "shell interaction smoke"),
    ] {
        app.set_browser_mode(mode);
        match label {
            "launch showcase" => app.run_showcase_visible(),
            "real browsing smoke" => app.run_real_browsing_visible(),
            "shell interaction smoke" => app.run_shell_interaction_visible(),
            _ => unreachable!(),
        }
        assert!(!app.last_ok);
        assert_eq!(app.pilot_status, "BLOCKED");
        assert!(app.last_status.contains(label));
    }

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn parses_visible_browser_mode_argument() {
    assert_eq!(parse_browser_mode("agent").unwrap(), BrowserMode::Agent);
    assert_eq!(parse_browser_mode("ASSIST").unwrap(), BrowserMode::Assisted);
    assert_eq!(parse_browser_mode("observe").unwrap(), BrowserMode::Observe);
    assert_eq!(parse_browser_mode("direct").unwrap(), BrowserMode::Direct);
    assert_eq!(
        parse_browser_mode("incognito").unwrap(),
        BrowserMode::Incognito
    );
    assert!(parse_browser_mode("mystery").is_err());

    let args = vec![
        "sextant-browser".to_string(),
        "--browser-mode".to_string(),
        "direct".to_string(),
    ];
    assert_eq!(parse_browser_mode_arg(&args).unwrap(), BrowserMode::Direct);
    assert_eq!(parse_browser_mode_arg(&[]).unwrap(), BrowserMode::Assisted);
}

#[test]
fn parses_visible_user_render_path_argument() {
    assert_eq!(
        parse_user_render_path("bridge").unwrap(),
        UserRenderPath::FrameBridge
    );
    assert_eq!(
        parse_user_render_path("servo-direct").unwrap(),
        UserRenderPath::DirectServo
    );
    assert!(parse_user_render_path("magic").is_err());

    let args = vec![
        "sextant-browser".to_string(),
        "--render-path".to_string(),
        "direct".to_string(),
    ];
    assert_eq!(
        parse_user_render_path_arg(&args).unwrap(),
        UserRenderPath::DirectServo
    );
    assert_eq!(
        parse_user_render_path_arg(&[]).unwrap(),
        UserRenderPath::FrameBridge
    );
}

#[test]
fn direct_render_modes_report_bridge_gap_until_integrated() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-render-gap-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    assert_eq!(app.render_path_label(), "BRIDGE");
    assert!(app.direct_render_gap_label().is_none());

    app.set_browser_mode(BrowserMode::Direct);
    assert_eq!(
        app.direct_render_gap_label(),
        Some("direct compositor pending")
    );

    let error = app
        .set_user_render_path(UserRenderPath::DirectServo)
        .unwrap_err();
    assert!(error.contains("production shell integration is not complete"));
    assert_eq!(app.render_path_label(), "BRIDGE");
    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[cfg(feature = "servo-backend")]
#[test]
fn visible_navigation_can_pre_size_viewport_when_requested() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-pre-size-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    app.pre_size_visible_navigation = true;
    let url = Url::parse("https://example.com").map_err(|error| error.to_string())?;

    assert!(app.start_visible_navigation(url));
    let expected = {
        let viewport = app.browser_viewport_rect;
        Some((viewport.w.max(1), viewport.h.max(1)))
    };
    assert_eq!(
        app.pending_navigation
            .as_ref()
            .and_then(|pending| pending.viewport_size),
        expected
    );
    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn parses_window_smoke_user_distill_argument() {
    let args = vec![
        "sextant-browser".to_string(),
        "--window-smoke".to_string(),
        "https://example.com".to_string(),
        "--window-smoke-distill".to_string(),
        "--window-input-smoke".to_string(),
        "--window-verified-input-smoke".to_string(),
        "--window-search-submit-smoke".to_string(),
        "--window-live-search-smoke".to_string(),
        "--window-live-form-smoke".to_string(),
        "--window-live-link-smoke".to_string(),
        "--allow-insecure-local-tls".to_string(),
        "--window-smoke-timeout".to_string(),
        "30".to_string(),
    ];
    let spec = parse_window_smoke(&args).unwrap().unwrap();

    assert_eq!(spec.target.as_deref(), Some("https://example.com"));
    assert_eq!(spec.timeout, Duration::from_secs(30));
    assert!(spec.user_distill);
    assert!(spec.input_latency);
    assert!(spec.verified_input_smoke);
    assert!(spec.search_submit_smoke);
    assert!(spec.live_search_smoke);
    assert!(spec.live_form_smoke);
    assert!(spec.live_link_smoke);
    assert!(spec.allow_insecure_local_tls);
    assert!(input_latency_fixture_url().starts_with("data:text/html,"));
}

#[test]
fn local_appliance_tls_override_is_limited_to_local_targets() {
    let private = Url::parse("https://192.168.2.130:8080/app").expect("private URL should parse");
    let localhost = Url::parse("https://localhost:8443").expect("local URL should parse");
    let mdns = Url::parse("https://pylon.local:8443").expect("local URL should parse");
    let public = Url::parse("https://example.com").expect("public URL should parse");

    assert!(is_local_appliance_target(&private));
    assert!(is_local_appliance_target(&localhost));
    assert!(is_local_appliance_target(&mdns));
    assert!(!is_local_appliance_target(&public));
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
#[test]
fn appliance_cert_trust_store_is_origin_and_fingerprint_scoped() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-appliance-cert-trust-{}",
        Uuid::new_v4()
    ));
    let target =
        Url::parse("https://192.0.2.130:8080/app/login").map_err(|error| error.to_string())?;
    let same_origin =
        Url::parse("https://192.0.2.130:8080/app").map_err(|error| error.to_string())?;
    let other_origin =
        Url::parse("https://192.0.2.131:8080/app").map_err(|error| error.to_string())?;
    let fingerprint =
        "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
    let changed_fingerprint = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    let mut store = ApplianceCertTrustStore::default();
    store.remember(&target, fingerprint, Some("Pylon".to_string()))?;
    store.save(&data_dir)?;

    let mut loaded = ApplianceCertTrustStore::load(&data_dir)?;
    assert!(loaded.is_trusted(&same_origin, fingerprint)?);
    assert!(!loaded.is_trusted(&same_origin, changed_fingerprint)?);
    assert!(!loaded.is_trusted(&other_origin, fingerprint)?);
    assert_eq!(
        appliance_origin(&target)?,
        "https://192.0.2.130:8080".to_string()
    );
    assert_eq!(
        normalize_certificate_fingerprint(fingerprint)?,
        "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
    );
    assert!(loaded.forget(&same_origin)?);
    assert!(!loaded.is_trusted(&same_origin, fingerprint)?);
    assert!(!loaded.forget(&same_origin)?);

    loaded.remember(&target, fingerprint, Some("Pylon".to_string()))?;
    assert!(loaded.clear());
    assert!(!loaded.clear());
    assert!(loaded.entries.is_empty());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
#[test]
fn guard_appliance_cert_settings_select_and_forget_entry() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-appliance-cert-settings-{}",
        Uuid::new_v4()
    ));
    let target =
        Url::parse("https://192.0.2.130:8080/app/login").map_err(|error| error.to_string())?;
    let fingerprint = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
    let mut store = ApplianceCertTrustStore::default();
    store.remember(&target, fingerprint, Some("Pylon".to_string()))?;
    store.save(&data_dir)?;

    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    assert_eq!(app.appliance_cert_entries.len(), 1);

    let guard_tab_rect = app.guard_tab_rect;
    click_rect(&mut app, guard_tab_rect, "Guard tab")?;
    let panel = main_panel_rect(right_rail_x(app.window_size.width), app.window_size.height);
    let row_rect = guard_appliance_row_rects(panel, app.appliance_cert_entries.len())
        .into_iter()
        .next()
        .map(|(_, rect)| rect)
        .ok_or_else(|| "expected appliance certificate trust row".to_string())?;
    click_rect(&mut app, row_rect, "trusted appliance row")?;
    assert_eq!(app.selected_appliance_cert, Some(0));
    assert!(app.last_status.contains("Selected appliance"));

    click_rect(
        &mut app,
        guard_appliance_forget_rect(panel),
        "Forget appliance certificate",
    )?;
    assert!(app.appliance_cert_entries.is_empty());
    assert!(app.selected_appliance_cert.is_none());
    assert!(app.last_status.contains("Forgot local appliance"));

    let loaded = ApplianceCertTrustStore::load(&data_dir)?;
    assert!(loaded.entries.is_empty());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn runtime_incognito_switch_uses_ephemeral_storage_and_restores_profile() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!(
        "sextant-browser-runtime-incognito-{}",
        Uuid::new_v4()
    ));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let profile_data_dir = app.profile_data_dir.clone();

    app.set_browser_mode(BrowserMode::Incognito);
    assert_eq!(app.browser_mode, BrowserMode::Incognito);
    let incognito_data_dir = app
        .incognito_data_dir
        .clone()
        .ok_or_else(|| "incognito mode did not allocate an ephemeral data dir".to_string())?;
    assert!(incognito_data_dir.to_string_lossy().contains("incognito"));
    assert_ne!(app.data_dir, profile_data_dir);
    assert!(incognito_data_dir.exists());

    app.set_browser_mode(BrowserMode::Direct);
    assert_eq!(app.browser_mode, BrowserMode::Direct);
    assert_eq!(app.data_dir, profile_data_dir);
    assert!(app.incognito_data_dir.is_none());
    assert!(!incognito_data_dir.exists());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn perception_tab_switches_to_perception_view() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-sense-tab-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));

    let sense_rect = app.perception_tab_rect;
    click_rect(&mut app, sense_rect, "Sense tab")?;
    assert!(app.main_view == MainView::Perception);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn guard_tab_switches_to_guard_view() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-guard-tab-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));

    let guard_rect = app.guard_tab_rect;
    click_rect(&mut app, guard_rect, "Guard tab")?;
    assert!(app.main_view == MainView::Guard);

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[test]
fn guard_report_includes_trust_boundary_lines() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-guard-report-{}", Uuid::new_v4()));
    let app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let url = Url::parse("https://example.com").unwrap();

    let lines = guard_report_lines(&app, Some(&url), None);

    assert!(lines.iter().any(|line| line.starts_with("guard persona:")));
    assert!(lines.iter().any(|line| line.starts_with("airgap:")));
    assert!(lines.iter().any(|line| line.starts_with("firewall:")));
    assert!(lines.iter().any(|line| line.starts_with("privacy:")));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[cfg(feature = "xilem-shell")]
#[test]
fn guard_decision_blocks_blacklisted_navigation() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-guard-block-{}", Uuid::new_v4()));
    let app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let url = Url::parse("https://malicious-site.net/path").unwrap();

    let decision = app.guard_decision(&url);

    assert!(!decision.allowed);
    assert_eq!(decision.action, "BLOCK");
    assert!(decision.reason.contains("Global blacklist"));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[cfg(feature = "xilem-shell")]
#[test]
fn guard_decision_uses_data_dir_policy_overlay() -> Result<(), String> {
    let data_dir = env::temp_dir().join(format!("sextant-browser-guard-policy-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    std::fs::write(
        data_dir.join("guard-policy.json"),
        r#"{
            "persona_rules": {
                "browser-persona": [
                    {
                        "domain_pattern": "example.com",
                        "action": "Block",
                        "reason": "operator QA policy"
                    }
                ]
            }
        }"#,
    )
    .map_err(|error| error.to_string())?;
    let app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let url = Url::parse("https://example.com").unwrap();

    let decision = app.guard_decision(&url);
    let lines = guard_report_lines(&app, Some(&url), None);

    assert!(!decision.allowed);
    assert_eq!(decision.action, "BLOCK");
    assert_eq!(decision.reason, "operator QA policy");
    assert!(lines
        .iter()
        .any(|line| line.contains("guard policy: overlay")));
    assert!(lines
        .iter()
        .any(|line| line.contains("guard rules: persona=1")));

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[cfg(feature = "xilem-shell")]
#[test]
fn navigate_to_stops_at_local_guard_block() -> Result<(), String> {
    let data_dir =
        env::temp_dir().join(format!("sextant-browser-guard-navigate-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    let url = Url::parse("https://malicious-site.net/path").unwrap();

    app.navigate_to(url);

    assert!(!app.last_ok);
    assert!(app.last_status.contains("Local guard blocked navigate"));
    assert!(app.latest_frame.is_none());

    let _ = std::fs::remove_dir_all(data_dir);
    Ok(())
}

#[cfg(feature = "xilem-shell")]
#[test]
fn guard_probe_reports_policy_block_as_probe_result() -> Result<(), String> {
    let report = run_guard_probe("https://malicious-site.net/path")?;

    assert!(report.iter().any(|line| line.contains("firewall: BLOCK")));
    assert!(report
        .iter()
        .any(|line| line.contains("browser guard probe blocked navigation")));

    Ok(())
}

#[test]
fn perception_summary_classifies_interactive_pages() {
    let page = sextant_engine::DistilledPage {
        title: "Form".to_string(),
        url: Url::parse("https://example.com/form").unwrap(),
        content: "Form page".to_string(),
        semantic_map: vec![
            sextant_engine::SemanticNode {
                id: "h1".to_string(),
                node_type: NodeType::Heading,
                text: "Form".to_string(),
                selector: "h1".to_string(),
                attributes: Default::default(),
            },
            sextant_engine::SemanticNode {
                id: "input-q".to_string(),
                node_type: NodeType::Input,
                text: "Search".to_string(),
                selector: "input[name=q]".to_string(),
                attributes: Default::default(),
            },
        ],
        metadata: Default::default(),
    };

    assert!(page_perception_summary(&page).contains("interactive form page"));
    assert_eq!(semantic_counts(&page).inputs, 1);
}

#[test]
fn draw_char_writes_exact_glyph_bitmap() {
    // The optimized draw_char must paint precisely the 5x7 glyph bitmap: lit
    // bits -> foreground, every other pixel untouched. Verified at scale 1 and
    // scale 2 (each lit bit becomes a scale x scale block).
    const BG: u32 = 0x0000_0000;
    const FG: u32 = 0x00ff_ffff;

    for ch in ['A', 'Z', '0', '8', 'M', '/', '.', ' '] {
        let glyph = glyph(ch);

        // scale 1 into an 8x8 buffer at the origin.
        let mut buf = vec![BG; 8 * 8];
        draw_char(&mut buf, 8, 8, 0, 0, ch, FG, 1);
        for row in 0..7usize {
            for col in 0..5u32 {
                let lit = glyph[row] & (1 << (4 - col)) != 0;
                let got = buf[row * 8 + col as usize];
                assert_eq!(
                    got,
                    if lit { FG } else { BG },
                    "scale1 {ch:?} row {row} col {col} (lit={lit})"
                );
            }
        }
        // No stray pixels outside the 5-wide glyph (column 5..8 stays bg).
        for row in 0..7usize {
            for col in 5..8usize {
                assert_eq!(buf[row * 8 + col], BG, "scale1 {ch:?} stray at col {col}");
            }
        }

        // scale 2 into a 16x16 buffer: each lit bit -> a 2x2 block.
        let mut buf2 = vec![BG; 16 * 16];
        draw_char(&mut buf2, 16, 16, 0, 0, ch, FG, 2);
        for row in 0..7usize {
            for col in 0..5u32 {
                let lit = glyph[row] & (1 << (4 - col)) != 0;
                for dy in 0..2u32 {
                    for dx in 0..2u32 {
                        let y = row as u32 * 2 + dy;
                        let x = col * 2 + dx;
                        assert_eq!(
                            buf2[(y * 16 + x) as usize],
                            if lit { FG } else { BG },
                            "scale2 {ch:?} row {row} col {col} block ({dx},{dy})"
                        );
                    }
                }
            }
        }
    }
}
