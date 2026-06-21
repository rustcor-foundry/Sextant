//! Operator / proof harness + intent parsing — the visible-input proof
//! workflows (showcase / real-browsing / shell-interaction), operator smoke +
//! probe scripts and their report formatting, the CLI arg/flag parsers, and
//! the natural-language intent parsing/planning helpers. Free functions split
//! out of browser.rs; `use super::*` supplies BrowserApp + types, and the crate
//! root re-exports them (`use harness::*`) so main, the tests, and the impl
//! partitions call them unchanged.

use super::*;

pub(crate) fn sextant_window_icon() -> Option<Icon> {
    let size = 64u32;
    let rgba = include_bytes!("../../assets/icons/sextant-64.rgba");
    Icon::from_rgba(rgba.to_vec(), size, size).ok()
}

pub(crate) fn start_visible_input(app: &mut BrowserApp, input: &str) -> Result<bool, String> {
    app.address_input = input.to_string();
    if native_intent_body(input).is_some() {
        app.run_native_intent_sync(input);
        if !app.last_ok {
            return Err(app.last_status.clone());
        }
        return Ok(false);
    } else {
        let url = parse_navigation_target(input)?;
        app.start_visible_navigation(url);
    }
    if !app.last_ok {
        return Err(app.last_status.clone());
    }
    Ok(app.pending_navigation.is_some())
}

pub(crate) fn run_operator_smoke() -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting native-browser operator bridge smoke".to_string());

    let data_dir =
        env::temp_dir().join(format!("sextant-browser-operator-smoke-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let sentinel = "native-agent-bridge-online";
    let url = Url::parse(&format!(
        "data:text/html,{}",
        "<!DOCTYPE html>\
<title>Native Operator Smoke</title>\
<main>\
<h1>Native Operator Smoke</h1>\
<input id='q' value='empty'>\
<button id='go' onclick=\"document.getElementById('out').textContent=document.getElementById('q').value\">Go</button>\
<p id='out'>waiting</p>\
</main>"
    ))
    .map_err(|error| format!("operator smoke data URL did not parse: {}", error))?;

    app.navigate_to(url);
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());

    let fill = app
        .fill_selector_current_page("#q", sentinel)
        .map_err(|error| format!("native fill interaction failed: {}", error))?;
    if !fill.ok || fill.value != sentinel {
        return Err(format!(
            "native fill interaction returned unexpected result: {:?}",
            fill
        ));
    }
    app.record_log("operator smoke fill #q", LogStatus::Success)?;
    report.push(format!("filled {} with {}", fill.selector, fill.value));

    let click = app
        .click_selector_current_page("#go")
        .map_err(|error| format!("native click interaction failed: {}", error))?;
    if !click.ok {
        return Err(format!(
            "native click interaction returned unexpected result: {:?}",
            click
        ));
    }
    app.record_log("operator smoke click #go", LogStatus::Success)?;
    report.push(format!("clicked {} tag {}", click.selector, click.tag));

    app.distill_active();
    if !app.last_ok {
        return Err(app.last_status);
    }
    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.as_ref())
        .ok_or_else(|| "distill did not attach a page to the active tab".to_string())?;
    if !page.content.contains(sentinel) {
        return Err(format!(
            "distilled content did not include sentinel '{}': {}",
            sentinel, page.content
        ));
    }
    report.push(format!("distilled '{}' and found sentinel", page.title));

    if app.wake_results.is_empty() {
        return Err("Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    if app.recent_logs.len() < 3 {
        return Err(format!(
            "Captain's Log returned only {} recent entries after smoke workflow",
            app.recent_logs.len()
        ));
    }
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "Servo frame capture did not return a frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "Servo frame capture returned an empty frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));

    report.push("native-browser operator bridge smoke passed".to_string());
    Ok(report)
}

pub(crate) fn run_showcase_script() -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting Sextant launch showcase run".to_string());

    let data_dir = env::temp_dir().join(format!("sextant-browser-showcase-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));
    report.extend(run_showcase_workflow(&mut app)?);
    Ok(report)
}

pub(crate) fn run_real_browsing_smoke() -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting real browsing smoke suite".to_string());

    let data_dir =
        env::temp_dir().join(format!("sextant-browser-real-browsing-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    report.extend(run_real_browsing_workflow(&mut app)?);
    Ok(report)
}

pub(crate) fn run_shell_interaction_workflow(app: &mut BrowserApp) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push("starting visible shell interaction smoke".to_string());

    click_rect(app, app.address_rect, "address field")?;
    if app.focus != FocusTarget::Address {
        return Err("address field click did not focus the address input".to_string());
    }
    report.push("address field click focused input".to_string());

    app.address_input = "https://example.com".to_string();
    click_action(app, Action::Navigate, "RUN")?;
    ensure_app_ok(app, "address run click")?;
    report.push(app.last_status.clone());

    click_action(app, Action::DistillActive, "DISTILL")?;
    ensure_app_ok(app, "distill click")?;
    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.clone())
        .ok_or_else(|| "distill click did not attach page data".to_string())?;
    if !operator_page_search_text(&page).contains("Example Domain") {
        return Err("distill click did not produce Example Domain content".to_string());
    }
    report.push(format!("distill click stored '{}'", page.title));

    app.wake_query = "Example Domain".to_string();
    click_action(app, Action::SearchWake, "WAKE")?;
    ensure_app_ok(app, "Wake click")?;
    if app.wake_results.is_empty() {
        return Err("Wake click returned no results".to_string());
    }
    report.push(format!(
        "Wake click returned {} result(s)",
        app.wake_results.len()
    ));

    click_rect(app, app.wake_tab_rect, "Wake tab")?;
    ensure_view(app, MainView::Wake, "Wake tab")?;
    report.push("Wake tab click switched view".to_string());

    click_rect(app, app.log_tab_rect, "Log tab")?;
    ensure_view(app, MainView::Log, "Log tab")?;
    report.push("Log tab click switched view".to_string());

    click_rect(app, app.guard_tab_rect, "Guard tab")?;
    ensure_view(app, MainView::Guard, "Guard tab")?;
    report.push("Guard tab click switched view".to_string());

    click_rect(app, app.perception_tab_rect, "Sense tab")?;
    ensure_view(app, MainView::Perception, "Sense tab")?;
    report.push("Sense tab click switched view".to_string());

    click_rect(app, app.perf_tab_rect, "Perf tab")?;
    ensure_view(app, MainView::Perf, "Perf tab")?;
    report.push("Perf tab click switched view".to_string());

    click_rect(app, app.validation_tab_rect, "Validation tab")?;
    ensure_view(app, MainView::Validation, "Validation tab")?;
    report.push("Validation tab click switched view".to_string());

    click_rect(app, app.browser_tab_rect, "Browser tab")?;
    ensure_view(app, MainView::Browser, "Browser tab")?;
    report.push("Browser tab click switched view".to_string());

    let viewport_center = rect_center(app.browser_viewport_rect);
    if !app.handle_mouse_input(viewport_center.0, viewport_center.1, ElementState::Released) {
        return Err("browser viewport click was not handled".to_string());
    }
    if app.focus != FocusTarget::Browser || !app.validation.browser_focus_seen {
        return Err("browser viewport click did not focus the viewport".to_string());
    }
    report.push("browser viewport click focused Servo viewport".to_string());

    let first_tab_id = app
        .active_tab()
        .map(|tab| tab.id)
        .ok_or_else(|| "shell interaction smoke lost the first tab".to_string())?;
    click_action(app, Action::NewTab, "NEW TAB")?;
    ensure_app_ok(app, "new tab click")?;
    let second_tab_id = app
        .active_tab()
        .map(|tab| tab.id)
        .ok_or_else(|| "new tab click did not create an active tab".to_string())?;
    report.push(app.last_status.clone());

    app.address_input = "https://www.iana.org/domains/reserved".to_string();
    click_action(app, Action::Navigate, "RUN second tab")?;
    ensure_app_ok(app, "second tab navigation")?;
    report.push("second tab navigated independently".to_string());

    click_page_tab(app, first_tab_id, "first page tab")?;
    ensure_active_tab(app, first_tab_id, "first page tab")?;
    report.push("first page tab click restored the original tab".to_string());

    click_page_tab(app, second_tab_id, "second page tab")?;
    ensure_active_tab(app, second_tab_id, "second page tab")?;
    report.push("second page tab click restored the new tab".to_string());

    click_action(app, Action::CloseTab, "CLOSE TAB")?;
    ensure_app_ok(app, "close tab click")?;
    ensure_active_tab(app, first_tab_id, "close tab fallback")?;
    report.push(app.last_status.clone());

    for _ in 0..=MAX_VISIBLE_PAGE_TABS {
        click_action(app, Action::NewTab, "overflow NEW TAB")?;
        ensure_app_ok(app, "overflow new tab click")?;
    }
    if app
        .page_tab_rects
        .iter()
        .any(|region| region.tab_id == first_tab_id)
    {
        return Err("page-tab overflow did not move the first tab out of view".to_string());
    }
    while app.can_page_tabs_previous() {
        let previous_rect = app.page_tab_prev_rect;
        click_rect(app, previous_rect, "previous page-tab pager")?;
        ensure_app_ok(app, "previous page-tab pager")?;
    }
    click_page_tab(app, first_tab_id, "overflow first page tab")?;
    ensure_active_tab(app, first_tab_id, "overflow first page tab")?;
    report.push("page-tab overflow pager restored the hidden first tab".to_string());

    app.refresh_logs();
    if app.recent_logs.len() < 5 {
        return Err(format!(
            "Captain's Log returned only {} recent entries after shell interaction smoke",
            app.recent_logs.len()
        ));
    }
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "shell interaction smoke did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "shell interaction smoke captured an empty Servo frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));

    report.push("visible shell interaction smoke passed".to_string());
    Ok(report)
}

pub(crate) fn run_real_browsing_workflow(app: &mut BrowserApp) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    navigate_distill_expect(
        app,
        "https://example.com",
        "Example Domain",
        "baseline content page",
        &mut report,
    )?;

    let query = "Sextant native browser test";
    report.push("starting Google search interaction".to_string());
    app.navigate_to(parse_navigation_target("https://www.google.com")?);
    ensure_app_ok(app, "google navigation")?;
    report.push(app.last_status.clone());

    let fill = app
        .fill_selector_current_page("textarea[name=q]", query)
        .map_err(|error| format!("google search fill failed: {}", error))?;
    if !fill.ok || fill.value != query {
        return Err(format!(
            "google search fill returned unexpected result: {:?}",
            fill
        ));
    }
    app.record_log("real browsing fill google search", LogStatus::Success)?;
    report.push(format!("filled Google search box with '{}'", query));

    let submit = app
        .submit_selector_current_page("form")
        .map_err(|error| format!("google search submit failed: {}", error))?;
    if !submit.ok {
        return Err(format!(
            "google search submit returned unexpected result: {:?}",
            submit
        ));
    }
    app.record_log("real browsing submit google search", LogStatus::Success)?;
    report.push(format!("submitted Google search form tag {}", submit.tag));

    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains(query) {
        return Err(format!(
            "google search result did not expose query '{}' in distilled page or URL",
            query
        ));
    }
    report.push(format!(
        "Google search distilled '{}' and retained query",
        page.title
    ));

    let docs_tab_id = app.engine.open_tab();
    app.show_page_tab(docs_tab_id);
    app.sync_address_to_active_tab();
    app.layout(app.window_size);
    report.push(format!(
        "opened isolated documentation tab {} after Google search",
        short_id(docs_tab_id)
    ));

    navigate_distill_expect(
        app,
        "https://developer.mozilla.org/en-US/docs/Web/HTML",
        "HTML",
        "heavier documentation page",
        &mut report,
    )?;

    app.reload();
    ensure_app_ok(app, "reload")?;
    report.push(app.last_status.clone());
    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains("HTML") {
        return Err("reload did not preserve the documentation page content".to_string());
    }
    report.push("reload preserved distillable documentation content".to_string());

    navigate_distill_expect(
        app,
        "https://www.iana.org/domains/reserved",
        "IANA",
        "second history page",
        &mut report,
    )?;

    app.back();
    ensure_app_ok(app, "back")?;
    report.push(app.last_status.clone());
    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains("HTML") {
        return Err("back navigation did not return to the documentation page".to_string());
    }
    report.push("back navigation returned to documentation page".to_string());

    app.forward();
    ensure_app_ok(app, "forward")?;
    report.push(app.last_status.clone());
    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains("IANA") {
        return Err("forward navigation did not return to the IANA page".to_string());
    }
    report.push("forward navigation returned to IANA page".to_string());

    if app.wake_results.is_empty() {
        return Err("real browsing Wake search returned no result".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    if app.recent_logs.len() < 5 {
        return Err(format!(
            "Captain's Log returned only {} recent entries after real browsing suite",
            app.recent_logs.len()
        ));
    }
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "real browsing suite did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "real browsing suite captured an empty Servo frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));

    report.push("real browsing smoke suite passed".to_string());
    Ok(report)
}

pub(crate) fn navigate_distill_expect(
    app: &mut BrowserApp,
    target: &str,
    expect: &str,
    label: &str,
    report: &mut Vec<String>,
) -> Result<(), String> {
    report.push(format!("opening {label}: {target}"));
    app.navigate_to(parse_navigation_target(target)?);
    ensure_app_ok(app, label)?;
    report.push(app.last_status.clone());

    let page = distill_operator_page(app)?;
    let haystack = operator_page_search_text(&page);
    if !haystack.contains(expect) {
        return Err(format!(
            "{label} did not include expected text '{}' in distilled page '{}' ({})",
            expect, page.title, page.url
        ));
    }
    let counts = semantic_counts(&page);
    report.push(format!(
        "{label} distilled '{}' via {} ({} chars, h={} links={} inputs={} images={} text={})",
        page.title,
        distillation_label(&page),
        page.content.chars().count(),
        counts.headings,
        counts.links,
        counts.inputs,
        counts.images,
        counts.text
    ));
    Ok(())
}

pub(crate) fn ensure_app_ok(app: &BrowserApp, label: &str) -> Result<(), String> {
    if app.last_ok {
        Ok(())
    } else {
        Err(format!("{label} failed: {}", app.last_status))
    }
}

pub(crate) fn click_action(
    app: &mut BrowserApp,
    action: Action,
    label: &str,
) -> Result<(), String> {
    let rect = app
        .buttons
        .iter()
        .find(|button| button.action == action)
        .map(|button| button.rect)
        .ok_or_else(|| format!("{label} button was not present in the visible chrome"))?;
    click_rect(app, rect, label)
}

pub(crate) fn click_page_tab(
    app: &mut BrowserApp,
    tab_id: Uuid,
    label: &str,
) -> Result<(), String> {
    let rect = app
        .page_tab_rects
        .iter()
        .find(|region| region.tab_id == tab_id)
        .map(|region| region.rect)
        .ok_or_else(|| format!("{label} was not visible in the page-tab strip"))?;
    click_rect(app, rect, label)
}

pub(crate) fn click_rect(app: &mut BrowserApp, rect: Rect, label: &str) -> Result<(), String> {
    let (x, y) = rect_center(rect);
    app.click(x, y);
    if app.last_ok {
        Ok(())
    } else {
        Err(format!("{label} click failed: {}", app.last_status))
    }
}

pub(crate) fn rect_center(rect: Rect) -> (f64, f64) {
    (
        rect.x as f64 + rect.w as f64 / 2.0,
        rect.y as f64 + rect.h as f64 / 2.0,
    )
}

pub(crate) fn ensure_view(app: &BrowserApp, view: MainView, label: &str) -> Result<(), String> {
    if app.main_view == view {
        Ok(())
    } else {
        Err(format!("{label} did not switch to the expected view"))
    }
}

pub(crate) fn ensure_active_tab(app: &BrowserApp, tab_id: Uuid, label: &str) -> Result<(), String> {
    match app.active_tab().map(|tab| tab.id) {
        Some(active_id) if active_id == tab_id => Ok(()),
        Some(active_id) => Err(format!(
            "{label} expected active tab {}, got {}",
            short_id(tab_id),
            short_id(active_id)
        )),
        None => Err(format!("{label} expected an active tab")),
    }
}

pub(crate) fn run_showcase_workflow(app: &mut BrowserApp) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    let intent = "intent: open https://example.com and distill";
    app.run_native_intent_sync(intent);
    if !app.last_ok {
        return Err(format!("showcase intent failed: {}", app.last_status));
    }
    report.push(format!("intent status: {}", app.pilot_status));
    report.push(format!("intent result: {}", app.pilot_result));

    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.clone())
        .ok_or_else(|| "showcase intent did not attach distilled page data".to_string())?;
    if !operator_page_search_text(&page).contains("Example Domain") {
        return Err("showcase intent page did not contain Example Domain".to_string());
    }
    let counts = semantic_counts(&page);
    report.push(format!(
        "intent distilled '{}' via {} (h={} links={} text={})",
        page.title,
        distillation_label(&page),
        counts.headings,
        counts.links,
        counts.text
    ));

    app.new_tab();
    if !app.last_ok {
        return Err(format!("showcase new tab failed: {}", app.last_status));
    }
    report.push(app.last_status.clone());

    let sentinel = "native pilot showcase";
    let url = Url::parse(&format!(
        "data:text/html,{}",
        "<!DOCTYPE html>\
<title>Sextant Showcase</title>\
<main>\
<h1>Sextant Showcase</h1>\
<p>Intent Bar, Wake, and native operator bridge are online.</p>\
<input id='q' value='empty'>\
<button id='go' onclick=\"document.getElementById('out').textContent=document.getElementById('q').value\">Go</button>\
<p id='out'>waiting</p>\
</main>"
    ))
    .map_err(|error| format!("showcase data URL did not parse: {}", error))?;
    app.navigate_to(url);
    if !app.last_ok {
        return Err(format!("showcase data page failed: {}", app.last_status));
    }
    report.push(app.last_status.clone());

    let fill = app
        .fill_selector_current_page("#q", sentinel)
        .map_err(|error| format!("showcase fill failed: {}", error))?;
    if !fill.ok || fill.value != sentinel {
        return Err(format!(
            "showcase fill returned unexpected result: {:?}",
            fill
        ));
    }
    app.record_log("showcase fill #q", LogStatus::Success)?;
    report.push(format!("filled {} with {}", fill.selector, fill.value));

    let click = app
        .click_selector_current_page("#go")
        .map_err(|error| format!("showcase click failed: {}", error))?;
    if !click.ok {
        return Err(format!(
            "showcase click returned unexpected result: {:?}",
            click
        ));
    }
    app.record_log("showcase click #go", LogStatus::Success)?;
    report.push(format!("clicked {} tag {}", click.selector, click.tag));

    let page = distill_operator_page(app)?;
    if !operator_page_search_text(&page).contains(sentinel) {
        return Err(format!(
            "showcase distilled page did not include sentinel '{}'",
            sentinel
        ));
    }
    report.push(format!(
        "interactive page distilled '{}' with sentinel",
        page.title
    ));

    if app.wake_results.is_empty() {
        return Err("showcase Wake search returned no result".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    if app.recent_logs.len() < 5 {
        return Err(format!(
            "showcase Captain's Log returned only {} recent entries",
            app.recent_logs.len()
        ));
    }
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "showcase did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "showcase captured an empty Servo frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));

    report.push(format!(
        "validation checks passed {}/{}",
        app.validation_pass_count(),
        app.validation_rows().len()
    ));
    report.push("Sextant launch showcase run passed".to_string());
    Ok(report)
}

pub(crate) fn run_intent_script(intent: &str, expect: Option<&str>) -> Result<Vec<String>, String> {
    run_intent_script_inner(intent, expect, false)
}

pub(crate) fn run_authorized_intent_script(
    intent: &str,
    expect: Option<&str>,
) -> Result<Vec<String>, String> {
    run_intent_script_inner(intent, expect, true)
}

pub(crate) fn run_intent_script_inner(
    intent: &str,
    expect: Option<&str>,
    authorize_consent: bool,
) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!(
        "starting native {}intent run for {}",
        if authorize_consent { "authorized " } else { "" },
        intent
    ));

    let data_dir = env::temp_dir().join(format!("sextant-browser-intent-run-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    app.address_input = intent.to_string();
    app.run_native_intent_sync(intent);
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(format!("pilot status: {}", app.pilot_status));
    report.push(format!("pilot result: {}", app.pilot_result));
    if app.pending_consent.is_some() {
        if !authorize_consent {
            return Err(
                "intent run paused for Captain's Key consent; use --consent-run to authorize and resume"
                    .to_string(),
            );
        }
        report.push("authorizing pending Captain's Key consent".to_string());
        app.authorize_pilot_consent();
        if !app.last_ok {
            return Err(app.last_status);
        }
        report.push(format!("pilot status after consent: {}", app.pilot_status));
        report.push(format!("pilot result after consent: {}", app.pilot_result));
    }

    let page = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.clone())
        .ok_or_else(|| "intent run did not attach distilled page data".to_string())?;
    let counts = semantic_counts(&page);
    report.push(format!(
        "distilled '{}' from {} via {} ({} chars, h={} links={} inputs={} images={} text={})",
        page.title,
        page.url,
        distillation_label(&page),
        page.content.chars().count(),
        counts.headings,
        counts.links,
        counts.inputs,
        counts.images,
        counts.text
    ));

    if let Some(text) = expect {
        let haystack = operator_page_search_text(&page);
        if !haystack.contains(text) {
            return Err(format!(
                "expected text '{}' was not found in distilled page '{}' ({})",
                text, page.title, page.url
            ));
        }
        report.push(format!("found expected text '{}'", text));
    }

    if app.wake_results.is_empty() {
        return Err("intent run Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    if !app.recent_logs.iter().any(|entry| {
        entry.intent.starts_with("pilot intent ") || entry.intent.starts_with("native intent ")
    }) {
        return Err("Captain's Log did not include the intent audit entry".to_string());
    }
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    if cfg!(feature = "servo-backend") {
        app.refresh_frame();
        let frame = app
            .latest_frame
            .as_ref()
            .ok_or_else(|| "intent run did not capture a Servo frame".to_string())?;
        if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
            return Err(format!(
                "intent run captured an empty Servo frame: {}x{} pixels={}",
                frame.width,
                frame.height,
                frame.pixels.len()
            ));
        }
        report.push(format!(
            "captured Servo frame {}x{} ({} pixels)",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    } else {
        report.push("reader/fallback build completed without Servo frame capture".to_string());
    }

    report.push(format!(
        "native {}intent run passed",
        if authorize_consent { "authorized " } else { "" }
    ));
    Ok(report)
}

pub(crate) fn run_operator_script(spec: OperatorRunSpec) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!(
        "starting scripted native-browser run for {}",
        spec.target
    ));

    let data_dir = env::temp_dir().join(format!("sextant-browser-operator-run-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let url = parse_navigation_target(&spec.target)?;
    app.navigate_to(url);
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());

    let mut distilled = false;
    for step in spec.steps {
        match step {
            OperatorStep::Fill { selector, value } => {
                let result = app.fill_selector_current_page(&selector, &value)?;
                if !result.ok || result.value != value {
                    return Err(format!(
                        "native fill interaction returned unexpected result: {:?}",
                        result
                    ));
                }
                app.record_log(
                    &format!("operator run fill {}", selector),
                    LogStatus::Success,
                )?;
                report.push(format!("filled {} with {}", selector, value));
                distilled = false;
            }
            OperatorStep::Click { selector } => {
                let result = app.click_selector_current_page(&selector)?;
                if !result.ok {
                    return Err(format!(
                        "native click interaction returned unexpected result: {:?}",
                        result
                    ));
                }
                app.record_log(
                    &format!("operator run click {}", selector),
                    LogStatus::Success,
                )?;
                report.push(format!("clicked {} tag {}", selector, result.tag));
                distilled = false;
            }
            OperatorStep::Submit { selector } => {
                let result = app.submit_selector_current_page(&selector)?;
                if !result.ok {
                    return Err(format!(
                        "native submit interaction returned unexpected result: {:?}",
                        result
                    ));
                }
                app.record_log(
                    &format!("operator run submit {}", selector),
                    LogStatus::Success,
                )?;
                report.push(format!("submitted {} tag {}", selector, result.tag));
                distilled = false;
            }
            OperatorStep::Expect { text } => {
                let page = distill_operator_page(&mut app)?;
                distilled = true;
                let haystack = operator_page_search_text(&page);
                if !haystack.contains(&text) {
                    return Err(format!(
                        "expected text '{}' was not found in distilled page '{}' ({})",
                        text, page.title, page.url
                    ));
                }
                report.push(format!("found expected text '{}'", text));
            }
        }
    }

    if !distilled {
        let page = distill_operator_page(&mut app)?;
        let counts = semantic_counts(&page);
        report.push(format!(
            "distilled '{}' from {} via {} ({} chars, h={} links={} inputs={} images={} text={})",
            page.title,
            page.url,
            distillation_label(&page),
            page.content.chars().count(),
            counts.headings,
            counts.links,
            counts.inputs,
            counts.images,
            counts.text
        ));
    }

    if app.wake_results.is_empty() {
        return Err("scripted run Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    app.refresh_logs();
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "scripted run did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "scripted run captured an empty Servo frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));

    report.push("scripted native-browser run passed".to_string());
    Ok(report)
}

pub(crate) fn run_operator_probe(target: &str) -> Result<Vec<String>, String> {
    run_page_probe(target, false)
}

pub(crate) fn run_perf_probe(target: &str) -> Result<Vec<String>, String> {
    run_page_probe(target, true)
}

pub(crate) fn run_perception_probe(target: &str) -> Result<Vec<String>, String> {
    let mut report = run_page_probe(target, false)?;
    let data_line = report
        .iter()
        .position(|line| line.starts_with("distilled "))
        .unwrap_or(report.len());
    report.insert(
        data_line.saturating_add(1),
        "perception probe includes page type, semantic counts, key nodes, and distillation metadata"
            .to_string(),
    );
    Ok(report)
}

pub(crate) fn run_guard_probe(target: &str) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!("starting browser guard probe for {}", target));

    let data_dir = env::temp_dir().join(format!("sextant-browser-guard-probe-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let url = parse_navigation_target(target)?;
    report.extend(guard_report_lines(&app, Some(&url), None));
    app.navigate_to(url);
    if !app.last_ok {
        if app.last_status.starts_with("Local guard blocked") {
            report.push(app.last_status.clone());
            report.push("browser guard probe blocked navigation".to_string());
            return Ok(report);
        }
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());

    app.distill_active();
    if app.last_ok {
        if let Some(page) = app.active_tab().and_then(|tab| tab.distilled_page.as_ref()) {
            report.extend(guard_report_lines(&app, Some(&page.url), Some(page)));
        }
    } else {
        report.push(format!("guard distillation skipped: {}", app.last_status));
    }
    report.push("browser guard probe passed".to_string());
    Ok(report)
}

pub(crate) fn run_page_probe(
    target: &str,
    repeat_frame_capture: bool,
) -> Result<Vec<String>, String> {
    let mut report = Vec::new();
    report.push(format!("starting native-browser probe for {}", target));

    let data_dir =
        env::temp_dir().join(format!("sextant-browser-operator-probe-{}", Uuid::new_v4()));
    let mut app = BrowserApp::new_with_data_dir(data_dir.clone())?;
    app.layout(PhysicalSize::new(1180, 760));
    report.push(format!("isolated data dir: {}", data_dir.display()));

    let url = parse_navigation_target(target)?;
    app.navigate_to(url.clone());
    if !app.last_ok {
        return Err(app.last_status);
    }
    report.push(app.last_status.clone());
    push_perf_report(&mut report, "after navigation", &app);
    push_eval_probe_report(&mut report, &mut app, "before distillation");

    app.distill_active();
    if !app.last_ok {
        return Err(app.last_status);
    }
    push_perf_report(&mut report, "after distillation", &app);

    let tab = app
        .active_tab()
        .ok_or_else(|| "probe finished without an active tab".to_string())?;
    let page = tab
        .distilled_page
        .as_ref()
        .ok_or_else(|| "probe did not attach distilled page data".to_string())?;
    let counts = semantic_counts(page);
    report.push(format!(
        "distilled '{}' from {} via {} ({} chars, h={} links={} inputs={} images={} text={})",
        page.title,
        page.url,
        distillation_label(page),
        page.content.chars().count(),
        counts.headings,
        counts.links,
        counts.inputs,
        counts.images,
        counts.text
    ));
    push_perception_report(&mut report, page);
    push_eval_probe_report(&mut report, &mut app, "after distillation");

    if app.wake_results.is_empty() {
        return Err("probe Wake search returned no result after distillation".to_string());
    }
    report.push(format!(
        "Wake search returned {} result(s)",
        app.wake_results.len()
    ));

    if repeat_frame_capture {
        app.distill_active();
        if !app.last_ok {
            return Err(format!("repeat distillation failed: {}", app.last_status));
        }
        push_perf_report(&mut report, "after repeat distillation", &app);
    }

    app.refresh_logs();
    report.push(format!(
        "Captain's Log returned {} recent entries",
        app.recent_logs.len()
    ));

    app.refresh_frame();
    let frame = app
        .latest_frame
        .as_ref()
        .ok_or_else(|| "probe did not capture a Servo frame".to_string())?;
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        return Err(format!(
            "probe captured an empty Servo frame: {}x{} pixels={}",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
    }
    report.push(format!(
        "captured Servo frame {}x{} ({} pixels)",
        frame.width,
        frame.height,
        frame.pixels.len()
    ));
    push_perf_report(&mut report, "after frame capture", &app);

    if repeat_frame_capture {
        app.refresh_frame();
        let frame = app
            .latest_frame
            .as_ref()
            .ok_or_else(|| "probe did not capture a warm Servo frame".to_string())?;
        if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
            return Err(format!(
                "probe captured an empty warm Servo frame: {}x{} pixels={}",
                frame.width,
                frame.height,
                frame.pixels.len()
            ));
        }
        report.push(format!(
            "captured warm Servo frame {}x{} ({} pixels)",
            frame.width,
            frame.height,
            frame.pixels.len()
        ));
        push_perf_report(&mut report, "after warm frame capture", &app);
    }

    report.push("native-browser probe passed".to_string());
    Ok(report)
}

pub(crate) fn run_perf_baseline() -> Result<Vec<String>, String> {
    let targets = [
        "https://example.com",
        "https://www.rust-lang.org/",
        "https://developer.mozilla.org/en-US/docs/Web/HTML",
        "https://en.wikipedia.org/wiki/Browser_engine",
    ];
    let mut report = vec![format!(
        "starting browser performance baseline across {} target(s)",
        targets.len()
    )];
    for target in targets {
        report.push(format!("baseline target: {target}"));
        match run_perf_probe(target) {
            Ok(lines) => {
                for line in lines {
                    if line.starts_with("perf ")
                        || line.starts_with("distilled ")
                        || line.starts_with("captured Servo frame ")
                    {
                        report.push(format!("{target}: {line}"));
                    } else if line.starts_with("captured warm Servo frame ") {
                        report.push(format!("{target}: {line}"));
                    }
                }
            }
            Err(error) => {
                report.push(format!("{target}: failed: {error}"));
            }
        }
    }
    report.push("browser performance baseline complete".to_string());
    Ok(report)
}

pub(crate) fn push_perf_report(report: &mut Vec<String>, label: &str, app: &BrowserApp) {
    report.push(format!("perf {label}: {}", app.perf.summary()));
}

pub(crate) fn push_perception_report(
    report: &mut Vec<String>,
    page: &sextant_engine::DistilledPage,
) {
    let counts = semantic_counts(page);
    report.push(format!(
        "perception summary: {}",
        page_perception_summary(page)
    ));
    report.push(format!(
        "perception counts: headings={} links={} inputs={} images={} text={} buttons={}",
        counts.headings, counts.links, counts.inputs, counts.images, counts.text, counts.buttons
    ));
    for (index, node) in key_semantic_nodes(page).into_iter().take(6).enumerate() {
        report.push(format!(
            "perception node {}: {} | {} | {}",
            index + 1,
            semantic_node_type_label(&node.node_type),
            truncate(&node.selector, 48),
            truncate(&node.text, 96)
        ));
    }
    if let Some(source) = page
        .metadata
        .get("distillation_backend")
        .or_else(|| page.metadata.get("source"))
        .or_else(|| page.metadata.get("distiller"))
    {
        report.push(format!("perception source: {source}"));
    }
    if let Some(eval_ms) = page.metadata.get("live_dom_eval_ms") {
        let queue_ms = page
            .metadata
            .get("live_dom_queue_ms")
            .map(String::as_str)
            .unwrap_or("unknown");
        let parse_ms = page
            .metadata
            .get("live_dom_parse_ms")
            .map(String::as_str)
            .unwrap_or("unknown");
        let script_ms = page
            .metadata
            .get("live_dom_script_ms")
            .map(String::as_str)
            .unwrap_or("unknown");
        report.push(format!(
            "perception live-dom timing: queue={}ms eval={}ms parse={}ms script={}ms",
            queue_ms, eval_ms, parse_ms, script_ms
        ));
    }
    if let Some(load_before) = page.metadata.get("live_dom_load_status_before") {
        let load_after = page
            .metadata
            .get("live_dom_load_status_after")
            .map(String::as_str)
            .unwrap_or("unknown");
        report.push(format!(
            "perception live-dom status: load_before={} load_after={}",
            load_before, load_after
        ));
    }
}

pub(crate) fn push_eval_probe_report(report: &mut Vec<String>, app: &mut BrowserApp, label: &str) {
    let Some(tab_id) = app.active_tab().map(|tab| tab.id) else {
        return;
    };
    match app.engine.eval_probe_tab(tab_id) {
        Ok(probe) => report.push(format!(
            "eval probe {label}: queue={}ms eval={}ms script={}ms load_before={} load_after={} value={}",
            probe.queue_ms,
            probe.eval_ms,
            probe.script_ms,
            probe.load_status_before,
            probe.load_status_after,
            truncate(&probe.value, 48)
        )),
        Err(error) => report.push(format!(
            "eval probe {label} failed: {}",
            truncate(&error, 120)
        )),
    }
}

pub(crate) fn distill_operator_page(
    app: &mut BrowserApp,
) -> Result<sextant_engine::DistilledPage, String> {
    app.distill_active();
    if !app.last_ok {
        return Err(app.last_status.clone());
    }
    app.active_tab()
        .and_then(|tab| tab.distilled_page.clone())
        .ok_or_else(|| "operator distill did not attach distilled page data".to_string())
}

pub(crate) fn operator_page_search_text(page: &sextant_engine::DistilledPage) -> String {
    let mut values = vec![
        page.title.clone(),
        page.url.to_string(),
        page.content.clone(),
    ];
    for (key, value) in page.url.query_pairs() {
        values.push(key.into_owned());
        values.push(value.into_owned());
    }
    for node in &page.semantic_map {
        values.push(node.text.clone());
        values.push(node.selector.clone());
        for (key, value) in &node.attributes {
            values.push(key.clone());
            values.push(value.clone());
        }
    }
    values.join("\n")
}

pub(crate) fn parse_operator_run(args: &[String]) -> Result<Option<OperatorRunSpec>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--operator-run") else {
        return Ok(None);
    };
    let target = args
        .get(index + 1)
        .ok_or_else(|| "--operator-run requires a target URL or search phrase".to_string())?
        .clone();
    let mut steps = Vec::new();
    let mut cursor = index + 2;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "--fill" => {
                let selector = args
                    .get(cursor + 1)
                    .ok_or_else(|| "--fill requires a selector and value".to_string())?
                    .clone();
                let value = args
                    .get(cursor + 2)
                    .ok_or_else(|| "--fill requires a selector and value".to_string())?
                    .clone();
                steps.push(OperatorStep::Fill { selector, value });
                cursor += 3;
            }
            "--click" => {
                let selector = args
                    .get(cursor + 1)
                    .ok_or_else(|| "--click requires a selector".to_string())?
                    .clone();
                steps.push(OperatorStep::Click { selector });
                cursor += 2;
            }
            "--submit" => {
                let selector = args
                    .get(cursor + 1)
                    .ok_or_else(|| "--submit requires a selector".to_string())?
                    .clone();
                steps.push(OperatorStep::Submit { selector });
                cursor += 2;
            }
            "--expect" => {
                let text = args
                    .get(cursor + 1)
                    .ok_or_else(|| "--expect requires text".to_string())?
                    .clone();
                steps.push(OperatorStep::Expect { text });
                cursor += 2;
            }
            "--operator-timeout" => {
                args.get(cursor + 1)
                    .ok_or_else(|| "--operator-timeout requires seconds".to_string())?;
                cursor += 2;
            }
            "--guard-policy" => {
                args.get(cursor + 1)
                    .ok_or_else(|| "--guard-policy requires a path".to_string())?;
                cursor += 2;
            }
            other => {
                return Err(format!(
                    "unknown operator-run argument '{}'; expected --fill, --click, --submit, --expect, --guard-policy, or --operator-timeout",
                    other
                ));
            }
        }
    }
    Ok(Some(OperatorRunSpec { target, steps }))
}

pub(crate) fn parse_operator_timeout(args: &[String]) -> Result<Duration, String> {
    parse_duration_arg(args, "--operator-timeout", OPERATOR_DEFAULT_TIMEOUT)
}

pub(crate) fn parse_window_smoke(args: &[String]) -> Result<Option<WindowSmokeSpec>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--window-smoke") else {
        return Ok(None);
    };
    let target = args
        .get(index + 1)
        .filter(|value| !value.starts_with("--"))
        .cloned();
    let timeout = parse_duration_arg(args, "--window-smoke-timeout", WINDOW_SMOKE_DEFAULT_TIMEOUT)?;
    let user_distill = args
        .iter()
        .any(|arg| arg == "--window-smoke-distill" || arg == "--user-distill");
    let input_latency = args
        .iter()
        .any(|arg| arg == "--window-input-smoke" || arg == "--input-latency-smoke");
    let first_interaction_smoke = args
        .iter()
        .any(|arg| arg == "--first-interaction-smoke" || arg == "--window-first-interaction-smoke");
    let verified_input_smoke = args
        .iter()
        .any(|arg| arg == "--verified-input-smoke" || arg == "--window-verified-input-smoke");
    let search_submit_smoke = args
        .iter()
        .any(|arg| arg == "--search-submit-smoke" || arg == "--window-search-submit-smoke");
    let live_search_smoke = args
        .iter()
        .any(|arg| arg == "--live-search-smoke" || arg == "--window-live-search-smoke");
    let live_form_smoke = args
        .iter()
        .any(|arg| arg == "--live-form-smoke" || arg == "--window-live-form-smoke");
    let live_link_smoke = args
        .iter()
        .any(|arg| arg == "--live-link-smoke" || arg == "--window-live-link-smoke");
    let load_smoke = args
        .iter()
        .any(|arg| arg == "--load-smoke" || arg == "--window-load-smoke");
    let resize_smoke = args
        .iter()
        .any(|arg| arg == "--resize-smoke" || arg == "--window-resize-smoke");
    let allow_insecure_local_tls = args.iter().any(|arg| arg == "--allow-insecure-local-tls");
    Ok(Some(WindowSmokeSpec {
        target,
        timeout,
        user_distill,
        input_latency,
        first_interaction_smoke,
        verified_input_smoke,
        search_submit_smoke,
        live_search_smoke,
        live_form_smoke,
        live_link_smoke,
        load_smoke,
        resize_smoke,
        allow_insecure_local_tls,
    }))
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend", test))]
pub(crate) fn is_local_appliance_target(target: &Url) -> bool {
    let Some(host) = target.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return true;
    }
    let Ok(addr) = host.parse::<IpAddr>() else {
        return false;
    };
    match addr {
        IpAddr::V4(addr) => addr.is_loopback() || addr.is_private() || addr.is_link_local(),
        IpAddr::V6(addr) => {
            let first = addr.segments()[0];
            addr.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

pub(crate) fn input_latency_fixture_url() -> String {
    let html = "<!doctype html><meta charset='utf-8'><title>Sextant Input Smoke</title>\
<body style='margin:0;background:#10161d;color:white;font:20px sans-serif;display:grid;place-items:center;height:100vh'>\
<input id='q' autofocus style='font:24px sans-serif;width:70vw;padding:18px' value=''></body>";
    let encoded: String = url::form_urlencoded::byte_serialize(html.as_bytes()).collect();
    format!("data:text/html,{}", encoded)
}

pub(crate) fn parse_browser_mode_arg(args: &[String]) -> Result<BrowserMode, String> {
    let Some(value) = operator_arg_value(args, "--browser-mode") else {
        return Ok(BrowserMode::Assisted);
    };
    parse_browser_mode(&value)
}

pub(crate) fn parse_user_render_path_arg(args: &[String]) -> Result<UserRenderPath, String> {
    let Some(value) = operator_arg_value(args, "--render-path")
        .or_else(|| operator_arg_value(args, "--user-render-path"))
    else {
        return Ok(UserRenderPath::FrameBridge);
    };
    parse_user_render_path(&value)
}

pub(crate) fn parse_browser_mode(value: &str) -> Result<BrowserMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "agent" => Ok(BrowserMode::Agent),
        "assist" | "assisted" => Ok(BrowserMode::Assisted),
        "observe" => Ok(BrowserMode::Observe),
        "direct" => Ok(BrowserMode::Direct),
        "incog" | "incognito" => Ok(BrowserMode::Incognito),
        _ => Err(format!(
            "unknown browser mode '{}'; expected agent, assisted, observe, direct, or incognito",
            value
        )),
    }
}

pub(crate) fn parse_user_render_path(value: &str) -> Result<UserRenderPath, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "bridge" | "frame-bridge" | "frame_bridge" | "render-bridge" | "render_bridge" => {
            Ok(UserRenderPath::FrameBridge)
        }
        "direct" | "direct-servo" | "direct_servo" | "servo-direct" | "servo_direct" => {
            Ok(UserRenderPath::DirectServo)
        }
        _ => Err(format!(
            "unknown user render path '{}'; expected bridge or direct",
            value
        )),
    }
}

pub(crate) fn parse_duration_arg(
    args: &[String],
    flag: &str,
    default: Duration,
) -> Result<Duration, String> {
    let Some(value) = operator_arg_value(args, flag) else {
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

pub(crate) fn operator_arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

pub(crate) fn parse_isize_flag(args: &[String], flag: &str) -> Option<isize> {
    operator_arg_value(args, flag).and_then(|value| value.parse::<isize>().ok())
}

pub(crate) fn parse_i32_flag(args: &[String], flag: &str) -> Option<i32> {
    operator_arg_value(args, flag).and_then(|value| value.parse::<i32>().ok())
}

pub(crate) fn parse_u32_flag(args: &[String], flag: &str) -> Option<u32> {
    operator_arg_value(args, flag).and_then(|value| value.parse::<u32>().ok())
}

pub(crate) fn parse_navigation_target(input: &str) -> Result<Url, String> {
    if let Ok(url) = Url::parse(input) {
        return Ok(url);
    }
    if input.contains('.') && !input.contains(' ') {
        return Url::parse(&format!("https://{}", input))
            .map_err(|error| format!("URL parse failed: {}", error));
    }
    let query = url::form_urlencoded::byte_serialize(input.as_bytes()).collect::<String>();
    Url::parse(&format!("https://duckduckgo.com/?q={}", query))
        .map_err(|error| format!("Search URL parse failed: {}", error))
}

pub(crate) fn native_intent_body(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    for prefix in [
        "intent:",
        "pilot:",
        "research:",
        "summarize:",
        "summarise:",
        "remember:",
    ] {
        if trimmed.len() >= prefix.len() && trimmed[..prefix.len()].eq_ignore_ascii_case(prefix) {
            let body = trimmed[prefix.len()..].trim();
            return (!body.is_empty()).then(|| body.to_string());
        }
    }

    let lower = trimmed.to_ascii_lowercase();
    let explicit_work = lower.starts_with("research ")
        || lower.starts_with("summarize ")
        || lower.starts_with("summarise ")
        || lower.starts_with("remember ")
        || lower.starts_with("distill ");
    let open_then_memory = lower.starts_with("open ")
        && (lower.contains(" and distill")
            || lower.contains(" and remember")
            || lower.contains(" and summarize")
            || lower.contains(" and summarise"));

    if explicit_work || open_then_memory {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub(crate) fn intent_needs_consent(intent: &str) -> bool {
    let lower = intent.to_ascii_lowercase();
    [
        "buy",
        "purchase",
        "checkout",
        "delete",
        "remove",
        "transfer",
        "wire",
        "sign",
        "authorize",
        "submit payment",
        "send money",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

pub(crate) fn plan_native_intent(intent: &str) -> Result<NativeIntentPlan, String> {
    let target_input = intent_navigation_text(intent);
    let target = parse_navigation_target(&target_input)?;
    let should_distill = intent_should_distill(intent);
    let mut steps = vec![
        format!("Resolve target: {}", truncate(&target_input, 42)),
        format!(
            "Navigate with {}",
            if cfg!(feature = "servo-backend") {
                "Servo"
            } else {
                "fallback backend"
            }
        ),
    ];
    if should_distill {
        steps.push("Distill page into Wake".to_string());
        steps.push("Search Wake for the recorded page".to_string());
    }

    Ok(NativeIntentPlan {
        intent: intent.to_string(),
        target,
        should_distill,
        steps,
    })
}

#[cfg(feature = "xilem-shell")]
pub(crate) fn pilot_action_plan(intent: &str) -> Result<Vec<PilotAction>, String> {
    if intent_needs_consent(intent) {
        let plan = plan_native_intent(intent)?;
        return Ok(vec![
            PilotAction::Analyze(
                "Sensitive browser action detected; Captain's Key consent is required.".to_string(),
            ),
            PilotAction::RequestConsent(format!(
                "Authorize this browser action before execution: {}",
                truncate(intent, 72)
            )),
            PilotAction::Navigate(plan.target.clone()),
            PilotAction::Distill,
            PilotAction::Perceive,
            PilotAction::Analyze(format!(
                "Captain's Key authorized a bounded review of {} for {}. Destructive or purchasing clicks remain gated.",
                short_url(&plan.target),
                truncate(intent, 42)
            )),
        ]);
    }

    let plan = plan_native_intent(intent)?;
    let mut actions = vec![PilotAction::Navigate(plan.target.clone())];
    if plan.should_distill {
        actions.push(PilotAction::Distill);
        actions.push(PilotAction::Analyze(format!(
            "Opened {} and stored the distilled page in Wake for {} across {} planned steps.",
            short_url(&plan.target),
            truncate(&plan.intent, 42),
            plan.steps.len()
        )));
    } else {
        actions.push(PilotAction::Analyze(format!(
            "Opened {} through the Pilot action lane for {} across {} planned steps.",
            short_url(&plan.target),
            truncate(&plan.intent, 42),
            plan.steps.len()
        )));
    }
    Ok(actions)
}

#[cfg(feature = "xilem-shell")]
pub(crate) fn pilot_action_step_label(action: &PilotAction) -> String {
    match action {
        PilotAction::Navigate(url) => format!("Pilot navigate: {}", short_url(url)),
        PilotAction::Distill => "Pilot distill active page into Wake".to_string(),
        PilotAction::Perceive => "Pilot perceive active browser state".to_string(),
        PilotAction::PerceiveMultiModal => {
            "Pilot queue multimodal perception through Neural Bridge".to_string()
        }
        PilotAction::RequestConsent(message) => {
            format!(
                "Pilot request Captain's Key consent: {}",
                truncate(message, 56)
            )
        }
        PilotAction::Analyze(message) => format!("Pilot analyze: {}", truncate(message, 64)),
        PilotAction::OpenTab(url) => format!("Pilot open tab: {}", short_url(url)),
        PilotAction::SwitchTab(tab_id) => format!("Pilot switch tab: {}", short_id(*tab_id)),
        PilotAction::CloseTab(tab_id) => format!("Pilot close tab: {}", short_id(*tab_id)),
    }
}

pub(crate) fn intent_should_distill(intent: &str) -> bool {
    let lower = intent.to_ascii_lowercase();
    [
        "research",
        "summarize",
        "summarise",
        "distill",
        "remember",
        "wake",
        "learn",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

pub(crate) fn intent_navigation_text(intent: &str) -> String {
    if let Some(candidate) = first_navigation_candidate(intent) {
        return candidate;
    }

    let mut text = intent.trim().to_string();
    let lower = text.to_ascii_lowercase();
    for prefix in [
        "open ",
        "go to ",
        "visit ",
        "research ",
        "summarize ",
        "summarise ",
        "distill ",
        "remember ",
        "find ",
        "search for ",
        "look up ",
    ] {
        if lower.starts_with(prefix) {
            text = text[prefix.len()..].trim().to_string();
            break;
        }
    }

    strip_memory_suffixes(&text)
}

pub(crate) fn first_navigation_candidate(input: &str) -> Option<String> {
    for token in input.split_whitespace() {
        let candidate = token.trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';'
            )
        });
        let candidate = candidate.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?'));
        if candidate.is_empty() {
            continue;
        }
        if Url::parse(candidate).is_ok() {
            return Some(candidate.to_string());
        }
        if candidate.contains('.') && !candidate.contains('/') {
            let lower = candidate.to_ascii_lowercase();
            let looks_like_domain = lower
                .split('.')
                .last()
                .map(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
                .unwrap_or(false);
            if looks_like_domain {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

pub(crate) fn strip_memory_suffixes(input: &str) -> String {
    let mut text = input.trim().to_string();
    loop {
        let lower = text.to_ascii_lowercase();
        let Some(index) = [
            " and distill",
            " and remember",
            " and summarize",
            " and summarise",
            " then distill",
            " then remember",
            " then summarize",
            " then summarise",
        ]
        .iter()
        .filter_map(|suffix| lower.find(suffix))
        .min() else {
            break;
        };
        text.truncate(index);
        text = text
            .trim()
            .trim_end_matches(|c: char| c == ',' || c == ';')
            .to_string();
    }
    if text.is_empty() {
        input.trim().to_string()
    } else {
        text
    }
}
