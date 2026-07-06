// sextant-browser core loop — `main`, the visible/direct-servo run loops, and
// the operator-mode watchdog. The browser shell itself lives in the
// `sextant_hull` library crate; this bin drives it. (lite.rs include!s this
// file as a compatibility entry point, so keep the header a plain comment.)

use sextant_hull::*;

use softbuffer::{Context, Surface};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIconBuilder, TrayIconEvent};
#[cfg(feature = "servo-backend")]
use url::Url;
#[cfg(feature = "servo-backend")]
use uuid::Uuid;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::keyboard::Key;
use winit::window::WindowBuilder;

/// Wakes the winit loop for tray/menu activity (the tray runs its own hidden
/// message window, so we forward its events through an `EventLoopProxy`).
#[derive(Debug)]
enum SextantEvent {
    TrayMenu(MenuEvent),
    TrayIcon(TrayIconEvent),
}

// ---- single-instance lifecycle -------------------------------------------
// A named mutex makes the visible browser single-instance; a second launch
// focuses the already-running window (restoring it from the tray) and exits.

#[cfg(windows)]
fn claim_single_instance() -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "Local\\SextantBrowserSingleInstance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: name is a valid NUL-terminated UTF-16 string; the handle is
    // intentionally left open so the mutex is held for the whole process.
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        if handle.is_null() {
            return true; // cannot enforce — allow the launch
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            focus_existing_window();
            return false;
        }
    }
    true
}

#[cfg(windows)]
fn focus_existing_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };
    let title: Vec<u16> = "Sextant Browser"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: FindWindowW with a NUL-terminated title; the returned HWND is
    // only passed back to Win32 show/focus calls.
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_RESTORE);
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn claim_single_instance() -> bool {
    true
}

// ---- hub-managed direct windows ------------------------------------------
// The full/bridge shell is the lifecycle hub: it can open direct-render windows
// (a separate sextant-browser process per the --render-path direct path) and
// owns their lifetime via a kill-on-close Job Object. Processes assigned to the
// job (and their child Servo processes) are terminated when the last handle to
// the job closes — i.e. when the hub process exits. So direct windows survive
// close-to-tray (hub keeps running) and die on a real quit, with no explicit
// teardown. 0 = "no job" (assignment is skipped, windows just run untracked).

#[cfg(windows)]
fn create_managed_job() -> isize {
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    // SAFETY: a freshly created, unnamed job object configured with
    // KILL_ON_JOB_CLOSE; info is a zeroed POD struct of the matching size.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return 0;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        job as isize
    }
}

#[cfg(not(windows))]
fn create_managed_job() -> isize {
    0
}

/// Open a direct-render window as a child process and (on Windows) tie its
/// lifetime to the hub via `job`. `url` seeds the first navigation.
fn spawn_direct_window(job: isize, url: Option<String>) -> Option<Child> {
    let exe = env::current_exe().ok()?;
    let mut command = Command::new(exe);
    command
        .arg("--render-path")
        .arg("direct")
        .arg("--browser-mode")
        .arg("direct");
    if let Some(url) = url {
        // Startup navigation is only read from `--start`; a bare argv token
        // was ignored and the child opened the default page instead.
        command.arg("--start").arg(url);
    }
    let child = command.spawn().ok()?;
    #[cfg(windows)]
    if job != 0 {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        // SAFETY: job and the child's process handle are both valid; the child
        // is freshly spawned and still owned here.
        unsafe {
            if AssignProcessToJobObject(job as *mut _, child.as_raw_handle() as *mut _) == 0 {
                eprintln!(
                    "[sextant-browser] failed to assign direct child to kill-on-close job object"
                );
            }
        }
    }
    let _ = job;
    Some(child)
}

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .init();

    let args: Vec<String> = env::args().collect();
    match handle_list_local_appliance_certs_arg(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("[sextant-browser] failed: {error}");
            std::process::exit(2);
        }
    }
    match handle_forget_local_appliance_cert_arg(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("[sextant-browser] failed: {error}");
            std::process::exit(2);
        }
    }
    if let Some(path) = operator_arg_value(&args, DIRECT_INCOGNITO_CLEANUP_ARG) {
        if let Err(error) = cleanup_direct_incognito_data_dir(Path::new(&path)) {
            eprintln!("[sextant-browser] direct incognito cleanup failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|arg| arg == "--retro-egui-proof") {
        #[cfg(all(target_os = "windows", feature = "servo-backend"))]
        {
            if let Err(error) = run_retro_egui_proof() {
                eprintln!("[retro-egui-proof] failed: {error}");
                std::process::exit(1);
            }
            return;
        }
        #[cfg(not(all(target_os = "windows", feature = "servo-backend")))]
        {
            eprintln!("--retro-egui-proof requires the Windows servo-backend build");
            std::process::exit(2);
        }
    }

    let operator_timeout = match parse_operator_timeout(&args) {
        Ok(timeout) => timeout,
        Err(error) => {
            eprintln!("[operator] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    if let Some(path) = operator_arg_value(&args, "--guard-policy") {
        env::set_var("SEXTANT_GUARD_POLICY", path);
    }

    match parse_operator_run(&args) {
        Ok(Some(spec)) => run_operator_mode("operator-run", operator_timeout, move || {
            run_operator_script(spec)
        }),
        Ok(None) => {}
        Err(error) => {
            eprintln!("[operator-run] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    }

    if let Some(target) = operator_arg_value(&args, "--operator-probe") {
        run_operator_mode("operator-probe", operator_timeout, move || {
            run_operator_probe(&target)
        });
    }

    if let Some(target) = operator_arg_value(&args, "--perf-probe") {
        run_operator_mode("perf-probe", operator_timeout, move || {
            run_perf_probe(&target)
        });
    }

    if let Some(target) = operator_arg_value(&args, "--perception-probe") {
        run_operator_mode("perception-probe", operator_timeout, move || {
            run_perception_probe(&target)
        });
    }

    if let Some(target) = operator_arg_value(&args, "--guard-probe") {
        run_operator_mode("guard-probe", operator_timeout, move || {
            run_guard_probe(&target)
        });
    }

    if args.iter().any(|arg| arg == "--perf-baseline") {
        run_operator_mode("perf-baseline", operator_timeout, run_perf_baseline);
    }

    if args.iter().any(|arg| arg == "--operator-smoke") {
        run_operator_mode("operator-smoke", operator_timeout, run_operator_smoke);
    }

    if args.iter().any(|arg| arg == "--showcase-run") {
        run_operator_mode("showcase-run", operator_timeout, run_showcase_script);
    }

    if args.iter().any(|arg| arg == "--real-browsing-smoke") {
        run_operator_mode(
            "real-browsing-smoke",
            operator_timeout,
            run_real_browsing_smoke,
        );
    }

    if let Some(intent) = operator_arg_value(&args, "--intent-run") {
        let expect = operator_arg_value(&args, "--expect");
        run_operator_mode("intent-run", operator_timeout, move || {
            run_intent_script(&intent, expect.as_deref())
        });
    }

    if let Some(intent) = operator_arg_value(&args, "--consent-run") {
        let expect = operator_arg_value(&args, "--expect");
        run_operator_mode("consent-run", operator_timeout, move || {
            run_authorized_intent_script(&intent, expect.as_deref())
        });
    }

    let window_smoke = match parse_window_smoke(&args) {
        Ok(spec) => spec,
        Err(error) => {
            eprintln!("[window-smoke] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    let browser_mode = match parse_browser_mode_arg(&args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("[sextant-browser] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    let user_render_path = match parse_user_render_path_arg(&args) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("[sextant-browser] failed to parse arguments: {error}");
            std::process::exit(2);
        }
    };
    let pre_size_visible_navigation = args
        .iter()
        .any(|arg| arg == "--pre-size-navigation" || arg == "--pre-size-visible-navigation");
    let certificate_path = operator_arg_value(&args, "--certificate-path").map(PathBuf::from);
    let local_appliance_cert_fingerprint =
        operator_arg_value(&args, "--local-appliance-cert-fingerprint");
    let remember_local_appliance_cert =
        operator_arg_value(&args, "--remember-local-appliance-cert");
    let allow_insecure_local_tls = window_smoke
        .as_ref()
        .map(|smoke| smoke.allow_insecure_local_tls)
        .unwrap_or(false)
        || args.iter().any(|arg| arg == "--allow-insecure-local-tls");
    let startup_input = operator_arg_value(&args, "--start");
    let start_showcase = args
        .iter()
        .any(|arg| arg == "--start-showcase" || arg == "--demo");
    let start_real_browsing = args.iter().any(|arg| arg == "--start-real-browsing");
    let start_shell_interaction = args.iter().any(|arg| arg == "--start-shell-interaction");
    let start_location_smoke = operator_arg_value(&args, "--location-smoke")
        .or_else(|| operator_arg_value(&args, "--window-location-smoke"));
    let start_history_smoke = operator_arg_value(&args, "--history-smoke")
        .or_else(|| operator_arg_value(&args, "--window-history-smoke"));
    let start_first_interaction_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.first_interaction_smoke)
        .unwrap_or(false)
        || args.iter().any(|arg| {
            arg == "--first-interaction-smoke" || arg == "--window-first-interaction-smoke"
        });
    let start_verified_input_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.verified_input_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--verified-input-smoke" || arg == "--window-verified-input-smoke");
    let start_search_submit_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.search_submit_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--search-submit-smoke" || arg == "--window-search-submit-smoke");
    let start_live_search_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.live_search_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--live-search-smoke" || arg == "--window-live-search-smoke");
    let start_live_form_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.live_form_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--live-form-smoke" || arg == "--window-live-form-smoke");
    let start_live_link_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.live_link_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--live-link-smoke" || arg == "--window-live-link-smoke");
    let direct_resource_audit = args
        .iter()
        .any(|arg| arg == "--direct-resource-audit" || arg == "--resource-audit");
    let raw_direct_window = args.iter().any(|arg| arg == "--raw-direct-window");
    let hosted_direct_smoke = args.iter().any(|arg| arg == "--hosted-direct-smoke");
    let hosted_direct_smoke_action = if hosted_direct_smoke {
        match HostedDirectSmokeAction::parse_arg(operator_arg_value(
            &args,
            "--hosted-direct-smoke-cert-action",
        )) {
            Ok(action) => action,
            Err(error) => {
                eprintln!("[hosted-direct-smoke] failed to parse arguments: {error}");
                std::process::exit(2);
            }
        }
    } else {
        None
    };
    let hosted_direct_smoke_timeout = if hosted_direct_smoke {
        let default_timeout =
            match parse_duration_arg(&args, "--window-smoke-timeout", Duration::from_secs(20)) {
                Ok(timeout) => timeout,
                Err(error) => {
                    eprintln!("[hosted-direct-smoke] failed to parse arguments: {error}");
                    std::process::exit(2);
                }
            };
        match parse_duration_arg(&args, "--hosted-direct-smoke-timeout", default_timeout) {
            Ok(timeout) => Some(timeout),
            Err(error) => {
                eprintln!("[hosted-direct-smoke] failed to parse arguments: {error}");
                std::process::exit(2);
            }
        }
    } else {
        None
    };
    let embed_parent_hwnd = parse_isize_flag(&args, "--embed-parent-hwnd");
    let embed_x = parse_i32_flag(&args, "--embed-x").unwrap_or(0);
    let embed_y = parse_i32_flag(&args, "--embed-y").unwrap_or(0);
    let embed_width = parse_u32_flag(&args, "--embed-width");
    let embed_height = parse_u32_flag(&args, "--embed-height");
    let start_load_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.load_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--load-smoke" || arg == "--window-load-smoke");
    let start_reload_smoke = args
        .iter()
        .any(|arg| arg == "--reload-smoke" || arg == "--window-reload-smoke");
    let start_resize_smoke = window_smoke
        .as_ref()
        .map(|smoke| smoke.resize_smoke)
        .unwrap_or(false)
        || args
            .iter()
            .any(|arg| arg == "--resize-smoke" || arg == "--window-resize-smoke");

    if user_render_path == UserRenderPath::DirectServo
        && !raw_direct_window
        && window_smoke.is_none()
    {
        let interactive =
            hosted_direct_smoke_timeout.is_none() && hosted_direct_smoke_action.is_none();
        #[cfg(all(target_os = "windows", feature = "servo-backend"))]
        let hosted_result = if interactive {
            run_hosted_direct_app_egui(
                startup_input,
                browser_mode,
                certificate_path,
                local_appliance_cert_fingerprint,
                remember_local_appliance_cert,
                allow_insecure_local_tls,
                direct_resource_audit,
            )
        } else {
            run_hosted_direct_app(
                startup_input,
                browser_mode,
                certificate_path,
                local_appliance_cert_fingerprint,
                remember_local_appliance_cert,
                allow_insecure_local_tls,
                direct_resource_audit,
                hosted_direct_smoke_timeout,
                hosted_direct_smoke_action,
            )
        };
        #[cfg(not(all(target_os = "windows", feature = "servo-backend")))]
        let hosted_result = {
            let _ = interactive;
            run_hosted_direct_app(
                startup_input,
                browser_mode,
                certificate_path,
                local_appliance_cert_fingerprint,
                remember_local_appliance_cert,
                allow_insecure_local_tls,
                direct_resource_audit,
                hosted_direct_smoke_timeout,
                hosted_direct_smoke_action,
            )
        };
        if let Err(error) = hosted_result {
            eprintln!("[sextant-browser] failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    if user_render_path == UserRenderPath::DirectServo {
        if let Err(error) = run_direct_servo_browser(
            window_smoke,
            startup_input,
            start_showcase,
            start_real_browsing,
            start_shell_interaction,
            start_location_smoke,
            start_history_smoke,
            start_first_interaction_smoke,
            start_verified_input_smoke,
            start_search_submit_smoke,
            start_live_search_smoke,
            start_live_form_smoke,
            start_live_link_smoke,
            direct_resource_audit,
            start_load_smoke,
            start_reload_smoke,
            start_resize_smoke,
            browser_mode,
            pre_size_visible_navigation,
            certificate_path,
            local_appliance_cert_fingerprint,
            remember_local_appliance_cert,
            allow_insecure_local_tls,
            embed_parent_hwnd,
            embed_x,
            embed_y,
            embed_width,
            embed_height,
        ) {
            eprintln!("[sextant-browser] failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    // Interactive bridge shell defaults to the egui renderer (modern chrome).
    // Opt into the legacy softbuffer HUD with SEXTANT_SOFTBUFFER_BRIDGE=1.
    // Window-smoke and automated proofs still use the softbuffer shell.
    #[cfg(all(
        target_os = "windows",
        feature = "servo-backend",
        feature = "xilem-shell"
    ))]
    if bridge_shell_use_egui(window_smoke.is_none()) {
        if let Err(error) = run_visible_app_egui(browser_mode) {
            eprintln!("[sextant-browser] egui bridge failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = run_visible_app(
        window_smoke,
        startup_input,
        start_showcase,
        start_real_browsing,
        start_shell_interaction,
        browser_mode,
        user_render_path,
        pre_size_visible_navigation,
    ) {
        eprintln!("[sextant-browser] failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(feature = "servo-backend")]
fn run_direct_servo_browser(
    window_smoke: Option<WindowSmokeSpec>,
    startup_input: Option<String>,
    start_showcase: bool,
    start_real_browsing: bool,
    start_shell_interaction: bool,
    start_location_smoke: Option<String>,
    start_history_smoke: Option<String>,
    start_first_interaction_smoke: bool,
    start_verified_input_smoke: bool,
    start_search_submit_smoke: bool,
    start_live_search_smoke: bool,
    start_live_form_smoke: bool,
    start_live_link_smoke: bool,
    direct_resource_audit: bool,
    start_load_smoke: bool,
    start_reload_smoke: bool,
    start_resize_smoke: bool,
    browser_mode: BrowserMode,
    pre_size_visible_navigation: bool,
    certificate_path: Option<PathBuf>,
    local_appliance_cert_fingerprint: Option<String>,
    remember_local_appliance_cert: Option<String>,
    allow_insecure_local_tls: bool,
    embed_parent_hwnd: Option<isize>,
    embed_x: i32,
    embed_y: i32,
    embed_width: Option<u32>,
    embed_height: Option<u32>,
) -> Result<(), String> {
    if !matches!(browser_mode, BrowserMode::Direct | BrowserMode::Incognito) {
        return Err(format!(
            "raw direct Servo render path currently supports Direct or Incognito mode only; {} mode still requires the bridge shell for AI observation/control surfaces",
            browser_mode.label()
        ));
    }

    let mut unsupported = Vec::new();
    if start_showcase {
        unsupported.push("launch showcase");
    }
    if start_real_browsing {
        unsupported.push("real browsing workflow");
    }
    if let Some(smoke) = window_smoke.as_ref() {
        if smoke.user_distill {
            unsupported.push("user distill");
        }
        if smoke.input_latency && start_shell_interaction {
            unsupported.push("combined direct input and tab smoke");
        }
        if smoke.input_latency && start_location_smoke.is_some() {
            unsupported.push("combined direct input and location smoke");
        }
        if smoke.input_latency && start_history_smoke.is_some() {
            unsupported.push("combined direct input and history smoke");
        }
        if smoke.input_latency && start_first_interaction_smoke {
            unsupported.push("combined direct input and first-interaction smoke");
        }
        if smoke.input_latency && start_verified_input_smoke {
            unsupported.push("combined direct input and verified-input smoke");
        }
        if smoke.input_latency && start_search_submit_smoke {
            unsupported.push("combined direct input and search-submit smoke");
        }
        if smoke.input_latency && start_live_search_smoke {
            unsupported.push("combined direct input and live-search smoke");
        }
        if smoke.input_latency && start_live_form_smoke {
            unsupported.push("combined direct input and live-form smoke");
        }
        if smoke.input_latency && start_load_smoke {
            unsupported.push("combined direct input and load smoke");
        }
        if smoke.input_latency && start_reload_smoke {
            unsupported.push("combined direct input and reload smoke");
        }
        if smoke.input_latency && start_resize_smoke {
            unsupported.push("combined direct input and resize smoke");
        }
        if start_shell_interaction && start_location_smoke.is_some() {
            unsupported.push("combined direct tab and location smoke");
        }
        if start_shell_interaction && start_history_smoke.is_some() {
            unsupported.push("combined direct tab and history smoke");
        }
        if start_shell_interaction && start_first_interaction_smoke {
            unsupported.push("combined direct tab and first-interaction smoke");
        }
        if start_shell_interaction && start_verified_input_smoke {
            unsupported.push("combined direct tab and verified-input smoke");
        }
        if start_shell_interaction && start_search_submit_smoke {
            unsupported.push("combined direct tab and search-submit smoke");
        }
        if start_shell_interaction && start_live_search_smoke {
            unsupported.push("combined direct tab and live-search smoke");
        }
        if start_shell_interaction && start_live_form_smoke {
            unsupported.push("combined direct tab and live-form smoke");
        }
        if start_shell_interaction && start_load_smoke {
            unsupported.push("combined direct tab and load smoke");
        }
        if start_shell_interaction && start_resize_smoke {
            unsupported.push("combined direct tab and resize smoke");
        }
        if start_location_smoke.is_some() && start_history_smoke.is_some() {
            unsupported.push("combined direct location and history smoke");
        }
        if start_location_smoke.is_some() && start_first_interaction_smoke {
            unsupported.push("combined direct location and first-interaction smoke");
        }
        if start_location_smoke.is_some() && start_verified_input_smoke {
            unsupported.push("combined direct location and verified-input smoke");
        }
        if start_location_smoke.is_some() && start_search_submit_smoke {
            unsupported.push("combined direct location and search-submit smoke");
        }
        if start_location_smoke.is_some() && start_live_search_smoke {
            unsupported.push("combined direct location and live-search smoke");
        }
        if start_location_smoke.is_some() && start_live_form_smoke {
            unsupported.push("combined direct location and live-form smoke");
        }
        if start_location_smoke.is_some() && start_load_smoke {
            unsupported.push("combined direct location and load smoke");
        }
        if start_location_smoke.is_some() && start_resize_smoke {
            unsupported.push("combined direct location and resize smoke");
        }
        if start_history_smoke.is_some() && start_load_smoke {
            unsupported.push("combined direct history and load smoke");
        }
        if start_history_smoke.is_some() && start_first_interaction_smoke {
            unsupported.push("combined direct history and first-interaction smoke");
        }
        if start_history_smoke.is_some() && start_verified_input_smoke {
            unsupported.push("combined direct history and verified-input smoke");
        }
        if start_history_smoke.is_some() && start_search_submit_smoke {
            unsupported.push("combined direct history and search-submit smoke");
        }
        if start_history_smoke.is_some() && start_live_search_smoke {
            unsupported.push("combined direct history and live-search smoke");
        }
        if start_history_smoke.is_some() && start_live_form_smoke {
            unsupported.push("combined direct history and live-form smoke");
        }
        if start_first_interaction_smoke && start_load_smoke {
            unsupported.push("combined direct first-interaction and load smoke");
        }
        if start_first_interaction_smoke && start_resize_smoke {
            unsupported.push("combined direct first-interaction and resize smoke");
        }
        if start_first_interaction_smoke && start_verified_input_smoke {
            unsupported.push("combined direct first-interaction and verified-input smoke");
        }
        if start_first_interaction_smoke && start_live_search_smoke {
            unsupported.push("combined direct first-interaction and live-search smoke");
        }
        if start_first_interaction_smoke && start_live_form_smoke {
            unsupported.push("combined direct first-interaction and live-form smoke");
        }
        if start_verified_input_smoke && start_load_smoke {
            unsupported.push("combined direct verified-input and load smoke");
        }
        if start_verified_input_smoke && start_resize_smoke {
            unsupported.push("combined direct verified-input and resize smoke");
        }
        if start_verified_input_smoke && start_search_submit_smoke {
            unsupported.push("combined direct verified-input and search-submit smoke");
        }
        if start_verified_input_smoke && start_live_search_smoke {
            unsupported.push("combined direct verified-input and live-search smoke");
        }
        if start_verified_input_smoke && start_live_form_smoke {
            unsupported.push("combined direct verified-input and live-form smoke");
        }
        if start_search_submit_smoke && start_live_search_smoke {
            unsupported.push("combined direct search-submit and live-search smoke");
        }
        if start_search_submit_smoke && start_live_form_smoke {
            unsupported.push("combined direct search-submit and live-form smoke");
        }
        if start_search_submit_smoke && start_load_smoke {
            unsupported.push("combined direct search-submit and load smoke");
        }
        if start_search_submit_smoke && start_resize_smoke {
            unsupported.push("combined direct search-submit and resize smoke");
        }
        if start_live_search_smoke && start_load_smoke {
            unsupported.push("combined direct live-search and load smoke");
        }
        if start_live_search_smoke && start_resize_smoke {
            unsupported.push("combined direct live-search and resize smoke");
        }
        if start_live_search_smoke && start_live_form_smoke {
            unsupported.push("combined direct live-search and live-form smoke");
        }
        if start_live_form_smoke && start_load_smoke {
            unsupported.push("combined direct live-form and load smoke");
        }
        if start_live_form_smoke && start_resize_smoke {
            unsupported.push("combined direct live-form and resize smoke");
        }
        if start_history_smoke.is_some() && start_resize_smoke {
            unsupported.push("combined direct history and resize smoke");
        }
        if start_load_smoke && start_resize_smoke {
            unsupported.push("combined direct load and resize smoke");
        }
        if start_reload_smoke
            && (start_shell_interaction
                || start_location_smoke.is_some()
                || start_history_smoke.is_some()
                || start_first_interaction_smoke
                || start_verified_input_smoke
                || start_search_submit_smoke
                || start_live_search_smoke
                || start_live_form_smoke
                || start_load_smoke
                || start_resize_smoke)
        {
            unsupported.push("combined direct reload and other control smoke");
        }
        if start_live_link_smoke
            && (start_shell_interaction
                || start_location_smoke.is_some()
                || start_history_smoke.is_some()
                || start_first_interaction_smoke
                || start_verified_input_smoke
                || start_search_submit_smoke
                || start_live_search_smoke
                || start_live_form_smoke
                || start_load_smoke
                || start_reload_smoke
                || start_resize_smoke)
        {
            unsupported.push("combined direct live-link and other control smoke");
        }
    }
    if !unsupported.is_empty() {
        return Err(format!(
            "direct Servo render path currently supports raw URL browsing only; {} still require the bridge shell",
            unsupported.join(", ")
        ));
    }

    let smoke_mode = window_smoke.is_some();
    let timeout = window_smoke
        .as_ref()
        .map(|smoke| smoke.timeout)
        .unwrap_or(WINDOW_SMOKE_DEFAULT_TIMEOUT);
    let input_latency = window_smoke
        .as_ref()
        .map(|smoke| smoke.input_latency)
        .unwrap_or(false);
    let requested_target = window_smoke
        .as_ref()
        .and_then(|smoke| smoke.target.clone())
        .or(startup_input);
    let verified_input_fixture = if requested_target.is_none() && start_verified_input_smoke {
        Some(direct_servo::start_verified_input_fixture_server()?)
    } else {
        None
    };
    let search_submit_fixture = if requested_target.is_none() && start_search_submit_smoke {
        Some(direct_servo::start_search_submit_fixture_server()?)
    } else {
        None
    };
    let target_text = requested_target.unwrap_or_else(|| {
        if input_latency {
            input_latency_fixture_url()
        } else if start_first_interaction_smoke {
            input_latency_fixture_url()
        } else if start_verified_input_smoke {
            verified_input_fixture
                .as_ref()
                .expect("verified input fixture should be started")
                .url()
                .to_string()
        } else if start_search_submit_smoke {
            search_submit_fixture
                .as_ref()
                .expect("search submit fixture should be started")
                .url()
                .to_string()
        } else if start_live_search_smoke {
            "https://lite.duckduckgo.com/lite/".to_string()
        } else if start_live_form_smoke {
            "https://httpbin.org/forms/post".to_string()
        } else {
            "https://example.com".to_string()
        }
    });
    let target = parse_navigation_target(&target_text)?;
    if let Some(path) = certificate_path.as_ref() {
        if !path.is_file() {
            return Err(format!(
                "certificate path '{}' does not exist or is not a file",
                path.display()
            ));
        }
    }
    if allow_insecure_local_tls && !is_local_appliance_target(&target) {
        return Err(format!(
            "--allow-insecure-local-tls is limited to localhost, .local, private, and link-local targets; refused {}",
            target
        ));
    }
    let profile_data_dir = app_data_dir().join("browser");
    let mut profile_trusted_local_tls = false;
    if let Some(fingerprint) = remember_local_appliance_cert.as_ref() {
        if browser_mode == BrowserMode::Incognito {
            return Err(
                "--remember-local-appliance-cert is disabled in Incognito; use Trust Once semantics"
                    .to_string(),
            );
        }
        let mut trust_store = ApplianceCertTrustStore::load(&profile_data_dir)?;
        trust_store.remember(&target, fingerprint, target.host_str().map(str::to_string))?;
        trust_store.save(&profile_data_dir)?;
        profile_trusted_local_tls = true;
    } else if browser_mode != BrowserMode::Incognito {
        if let Some(fingerprint) = local_appliance_cert_fingerprint.as_ref() {
            let trust_store = ApplianceCertTrustStore::load(&profile_data_dir)?;
            profile_trusted_local_tls = trust_store.is_trusted(&target, fingerprint)?;
        }
    }
    let appliance_certificate_actions =
        if target.scheme() == "https" && is_local_appliance_target(&target) {
            let trust_target = target.clone();
            let trust_data_dir = profile_data_dir.clone();
            Some(direct_servo::DirectApplianceCertificateActions {
                persist_allowed: browser_mode != BrowserMode::Incognito,
                remember: Arc::new(move |fingerprint| {
                    let mut trust_store = ApplianceCertTrustStore::load(&trust_data_dir)?;
                    trust_store.remember(
                        &trust_target,
                        &fingerprint,
                        trust_target.host_str().map(str::to_string),
                    )?;
                    trust_store.save(&trust_data_dir)
                }),
            })
        } else {
            None
        };
    let scripted_text = input_latency.then(|| "sextant".to_string());
    let direct_data_dir = if browser_mode == BrowserMode::Incognito {
        let data_dir = env::temp_dir().join(format!(
            "sextant-browser-direct-incognito-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
        Some(data_dir)
    } else {
        None
    };

    // Servo config dir for the direct lane (cookies, IndexedDB, HSTS, auth cache):
    // the ephemeral dir in Incognito, otherwise a persistent profile dir so
    // storage is profile-scoped instead of falling back to Servo's cwd-relative
    // default (which would leak `IndexedDB/` next to the working directory and
    // persist in Incognito). `direct_data_dir` stays Incognito-only so the exit
    // cleanup never removes the persistent Direct profile.
    let direct_config_dir: Option<PathBuf> = match direct_data_dir.as_ref() {
        Some(ephemeral) => Some(ephemeral.clone()),
        None => {
            let data_dir = app_data_dir().join("browser").join("direct-servo");
            std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
            Some(data_dir)
        }
    };

    if smoke_mode {
        println!("[window-smoke] starting visible browser shell smoke");
        println!(
            "[window-smoke] browser mode {} ({})",
            browser_mode.label(),
            browser_mode.status()
        );
        println!(
            "[window-smoke] render path {} ({})",
            UserRenderPath::DirectServo.label(),
            UserRenderPath::DirectServo.status()
        );
        println!("[window-smoke] render gap shell chrome compositor pending");
        println!(
            "[window-smoke] pre-size navigation {}",
            pre_size_visible_navigation
        );
        if let Some(data_dir) = direct_data_dir.as_ref() {
            println!(
                "[window-smoke] ephemeral browser data dir {}",
                data_dir.display()
            );
            println!(
                "[window-smoke] direct Servo config dir {}",
                data_dir.display()
            );
            println!("[window-smoke] direct Servo http cache disabled true");
        }
        if let Some(path) = certificate_path.as_ref() {
            println!(
                "[window-smoke] direct Servo certificate path {}",
                path.display()
            );
        }
        if allow_insecure_local_tls {
            println!("[window-smoke] direct Servo insecure local TLS true");
        }
        if profile_trusted_local_tls {
            println!("[window-smoke] direct Servo trusted local appliance TLS true");
        }
        let viewport_width = embed_width.unwrap_or(1180);
        let viewport_height = embed_height.unwrap_or(760);
        println!("[window-smoke] direct viewport {viewport_width}x{viewport_height}");
    } else {
        println!(
            "[window-start] starting direct Servo raw browsing lane in {} mode",
            browser_mode.label()
        );
    }

    let ignore_certificate_errors = allow_insecure_local_tls || profile_trusted_local_tls;
    let bypass_proxy_for_target = is_local_appliance_target(&target)
        && (ignore_certificate_errors || target.scheme() == "https");
    let outcome = direct_servo::run(direct_servo::DirectServoOptions {
        target: target.clone(),
        smoke: smoke_mode,
        timeout,
        title: "Sextant Browser Direct".to_string(),
        log_prefix: "window-direct",
        setup_servo_logging: false,
        scripted_text,
        scripted_first_interaction_smoke: start_first_interaction_smoke,
        scripted_verified_input_smoke: start_verified_input_smoke,
        scripted_verified_input_repeat_smoke: false,
        scripted_search_submit_smoke: start_search_submit_smoke,
        scripted_live_search_smoke: start_live_search_smoke,
        scripted_live_form_smoke: start_live_form_smoke,
        scripted_live_link_smoke: start_live_link_smoke,
        scripted_retained_navigation_smoke: false,
        scripted_location: start_location_smoke,
        scripted_history: start_history_smoke,
        scripted_load_smoke: start_load_smoke,
        scripted_reload_smoke: start_reload_smoke,
        scripted_resize_smoke: start_resize_smoke,
        scripted_tab_smoke: start_shell_interaction,
        resource_audit: direct_resource_audit,
        config_dir: direct_config_dir.clone(),
        disable_http_cache: browser_mode == BrowserMode::Incognito,
        certificate_path: certificate_path.clone(),
        ignore_certificate_errors,
        appliance_certificate_actions,
        bypass_proxy_for_target,
        embed_parent_hwnd,
        embed_bounds: embed_width.zip(embed_height).map(|(width, height)| {
            direct_servo::DirectEmbedBounds {
                x: embed_x,
                y: embed_y,
                width,
                height,
            }
        }),
        host_command_rx: None,
        read_host_commands_from_stdin: embed_parent_hwnd.is_some(),
    })?;
    let cleanup_result = direct_data_dir
        .as_ref()
        .map(|data_dir| cleanup_or_defer_direct_incognito_data_dir(data_dir))
        .unwrap_or(Ok(()));
    if let Err(cleanup_error) = cleanup_result {
        return Err(cleanup_error);
    }

    if !smoke_mode
        && (outcome.appliance_certificate_trust_once_requested
            || outcome.appliance_certificate_remembered == Some(true))
    {
        if embed_parent_hwnd.is_some() {
            let decision = if outcome.appliance_certificate_remembered == Some(true) {
                "remembered"
            } else {
                "trust-once"
            };
            if let Some(fingerprint) = outcome.certificate_fingerprint_sha256.as_ref() {
                println!(
                    "[window-direct] hosted certificate trust {decision} fingerprint {fingerprint}"
                );
            } else {
                println!("[window-direct] hosted certificate trust {decision}");
            }
            return Ok(());
        }
        relaunch_direct_local_appliance_after_trust(
            browser_mode,
            &target,
            &outcome,
            certificate_path.as_deref(),
            direct_resource_audit,
        )?;
        return Ok(());
    }

    if smoke_mode {
        if let Some(first_present) = outcome.first_present {
            println!(
                "[window-smoke] first direct present in {}",
                direct_servo::format_duration(first_present)
            );
            println!(
                "[window-smoke] first Servo frame in {}",
                direct_servo::format_duration(first_present)
            );
        }
        println!(
            "[window-smoke] direct frame timing frames={} slow={}",
            outcome.direct_frame_count, outcome.direct_slow_frame_count
        );
        if let Some(max_frame) = outcome.direct_max_frame {
            println!(
                "[window-smoke] max direct frame total={} spin={} paint={} present={}",
                direct_servo::format_duration(max_frame),
                outcome
                    .direct_max_spin
                    .map(direct_servo::format_duration)
                    .unwrap_or_else(|| "n/a".to_string()),
                outcome
                    .direct_max_paint
                    .map(direct_servo::format_duration)
                    .unwrap_or_else(|| "n/a".to_string()),
                outcome
                    .direct_max_present
                    .map(direct_servo::format_duration)
                    .unwrap_or_else(|| "n/a".to_string())
            );
        }
        if let Some(input_frame) = outcome.input_frame {
            println!(
                "[window-smoke] input frame {}",
                direct_servo::format_duration(input_frame)
            );
        }
        if let Some(first_interaction_frame) = outcome.first_interaction_frame {
            println!(
                "[window-smoke] first interaction frame {}",
                direct_servo::format_duration(first_interaction_frame)
            );
        }
        if let Some(before_load_complete) = outcome.first_interaction_before_load_complete {
            println!(
                "[window-smoke] first interaction before load complete {}",
                before_load_complete
            );
        }
        if let Some(verified_input_frame) = outcome.verified_input_frame {
            println!(
                "[window-smoke] verified input frame {}",
                direct_servo::format_duration(verified_input_frame)
            );
        }
        if let Some(verified_input_second_frame) = outcome.verified_input_second_frame {
            println!(
                "[window-smoke] verified repeat input frame {}",
                direct_servo::format_duration(verified_input_second_frame)
            );
        }
        if let Some(verified_input) = outcome.verified_input {
            println!("[window-smoke] verified input {}", verified_input);
        }
        if let Some(before_load_complete) = outcome.verified_input_before_load_complete {
            println!(
                "[window-smoke] verified input before load complete {}",
                before_load_complete
            );
        }
        if let Some(search_submit_frame) = outcome.search_submit_frame {
            println!(
                "[window-smoke] search submit frame {}",
                direct_servo::format_duration(search_submit_frame)
            );
        }
        if let Some(search_submit) = outcome.search_submit {
            println!("[window-smoke] search submit {}", search_submit);
        }
        if let Some(search_submit_url) = outcome.search_submit_url {
            println!("[window-smoke] search submit url {}", search_submit_url);
        }
        if let Some(search_submit_title) = outcome.search_submit_title {
            println!("[window-smoke] search submit title {}", search_submit_title);
        }
        if let Some(live_search_frame) = outcome.live_search_frame {
            println!(
                "[window-smoke] live search frame {}",
                direct_servo::format_duration(live_search_frame)
            );
        }
        if let Some(live_search) = outcome.live_search {
            println!("[window-smoke] live search {}", live_search);
        }
        if let Some(live_search_url) = outcome.live_search_url {
            println!("[window-smoke] live search url {}", live_search_url);
        }
        if let Some(live_search_title) = outcome.live_search_title {
            println!("[window-smoke] live search title {}", live_search_title);
        }
        if let Some(live_form_frame) = outcome.live_form_frame {
            println!(
                "[window-smoke] live form frame {}",
                direct_servo::format_duration(live_form_frame)
            );
        }
        if let Some(live_form) = outcome.live_form {
            println!("[window-smoke] live form {}", live_form);
        }
        if let Some(live_form_url) = outcome.live_form_url {
            println!("[window-smoke] live form url {}", live_form_url);
        }
        if let Some(live_form_title) = outcome.live_form_title {
            println!("[window-smoke] live form title {}", live_form_title);
        }
        if let Some(live_link_frame) = outcome.live_link_frame {
            println!(
                "[window-smoke] live link frame {}",
                direct_servo::format_duration(live_link_frame)
            );
        }
        if let Some(live_link) = outcome.live_link {
            println!("[window-smoke] live link {}", live_link);
        }
        if let Some(live_link_url) = outcome.live_link_url {
            println!("[window-smoke] live link url {}", live_link_url);
        }
        if let Some(live_link_title) = outcome.live_link_title {
            println!("[window-smoke] live link title {}", live_link_title);
        }
        if let Some(location_frame) = outcome.location_frame {
            println!(
                "[window-smoke] location frame {}",
                direct_servo::format_duration(location_frame)
            );
        }
        if let Some(location_url) = outcome.location_url {
            println!("[window-smoke] location url {}", location_url);
        }
        if let Some(location_title) = outcome.location_title {
            println!("[window-smoke] location title {}", location_title);
        }
        if let Some(history_navigation_frame) = outcome.history_navigation_frame {
            println!(
                "[window-smoke] history navigation frame {}",
                direct_servo::format_duration(history_navigation_frame)
            );
        }
        if let Some(history_back_frame) = outcome.history_back_frame {
            println!(
                "[window-smoke] history back frame {}",
                direct_servo::format_duration(history_back_frame)
            );
        }
        if let Some(history_forward_frame) = outcome.history_forward_frame {
            println!(
                "[window-smoke] history forward frame {}",
                direct_servo::format_duration(history_forward_frame)
            );
        }
        if let Some(history_final_url) = outcome.history_final_url {
            println!("[window-smoke] history final url {}", history_final_url);
        }
        if let Some(history_final_title) = outcome.history_final_title {
            println!("[window-smoke] history final title {}", history_final_title);
        }
        if let Some(load_complete) = outcome.load_complete {
            println!(
                "[window-smoke] load complete {}",
                direct_servo::format_duration(load_complete)
            );
        }
        if let Some(load_url) = outcome.load_url {
            println!("[window-smoke] load url {}", load_url);
        }
        if let Some(load_title) = outcome.load_title {
            println!("[window-smoke] load title {}", load_title);
        }
        if let Some(fingerprint) = outcome.certificate_fingerprint_sha256 {
            println!("[window-smoke] certificate fingerprint sha256 {fingerprint}");
        }
        if let Some(remembered) = outcome.appliance_certificate_remembered {
            println!("[window-smoke] appliance certificate remembered {remembered}");
        }
        if let Some(error) = outcome.appliance_certificate_remember_error {
            println!("[window-smoke] appliance certificate remember error {error}");
        }
        if let Some(audit) = outcome.resource_audit_json {
            println!("[window-smoke] resource audit {audit}");
        }
        if let Some(reload_frame) = outcome.reload_frame {
            println!(
                "[window-smoke] reload frame {}",
                direct_servo::format_duration(reload_frame)
            );
        }
        if let Some(reload_url) = outcome.reload_url {
            println!("[window-smoke] reload url {}", reload_url);
        }
        if let Some(reload_title) = outcome.reload_title {
            println!("[window-smoke] reload title {}", reload_title);
        }
        if let Some(resize_frame) = outcome.resize_frame {
            println!(
                "[window-smoke] resize frame {}",
                direct_servo::format_duration(resize_frame)
            );
        }
        if let Some(tab_frame) = outcome.tab_frame {
            println!(
                "[window-smoke] tab frame {}",
                direct_servo::format_duration(tab_frame)
            );
        }
        if let Some(tab_url) = outcome.tab_url {
            println!("[window-smoke] tab url {}", tab_url);
        }
        if let Some(tab_title) = outcome.tab_title {
            println!("[window-smoke] tab title {}", tab_title);
        }
    }
    Ok(())
}

#[cfg(feature = "servo-backend")]
fn relaunch_direct_local_appliance_after_trust(
    browser_mode: BrowserMode,
    target: &Url,
    outcome: &direct_servo::DirectServoOutcome,
    certificate_path: Option<&Path>,
    direct_resource_audit: bool,
) -> Result<(), String> {
    let mut args = vec![
        "--browser-mode".to_string(),
        browser_mode.cli_arg().to_string(),
        "--render-path".to_string(),
        "direct".to_string(),
        "--start".to_string(),
        target.as_str().to_string(),
    ];
    if let Some(path) = certificate_path {
        args.extend([
            "--certificate-path".to_string(),
            path.to_string_lossy().to_string(),
        ]);
    }
    if direct_resource_audit {
        args.push("--direct-resource-audit".to_string());
    }
    if outcome.appliance_certificate_remembered == Some(true) {
        if let Some(fingerprint) = outcome.certificate_fingerprint_sha256.as_ref() {
            args.extend([
                "--local-appliance-cert-fingerprint".to_string(),
                fingerprint.clone(),
            ]);
        } else {
            args.push("--allow-insecure-local-tls".to_string());
        }
    } else {
        args.push("--allow-insecure-local-tls".to_string());
    }

    let exe = env::current_exe().map_err(|error| error.to_string())?;
    let mut command = std::process::Command::new(exe);
    command.args(args);
    if let Ok(current_dir) = env::current_dir() {
        command.current_dir(current_dir);
    }
    command
        .spawn()
        .map_err(|error| format!("failed to relaunch trusted local appliance window: {error}"))?;
    Ok(())
}

#[cfg(feature = "servo-backend")]
fn cleanup_or_defer_direct_incognito_data_dir(data_dir: &Path) -> Result<(), String> {
    match cleanup_direct_incognito_data_dir(data_dir) {
        Ok(()) => Ok(()),
        Err(cleanup_error) => {
            let exe = env::current_exe().map_err(|error| {
                format!(
                    "{cleanup_error}; additionally failed to locate cleanup helper executable: {error}"
                )
            })?;
            std::process::Command::new(exe)
                .arg(DIRECT_INCOGNITO_CLEANUP_ARG)
                .arg(data_dir)
                .spawn()
                .map(|_| ())
                .map_err(|error| {
                    format!(
                        "{cleanup_error}; additionally failed to launch cleanup helper: {error}"
                    )
                })
        }
    }
}

fn cleanup_direct_incognito_data_dir(data_dir: &Path) -> Result<(), String> {
    let temp_dir = env::temp_dir();
    let file_name = data_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !data_dir.starts_with(&temp_dir)
        || !file_name.starts_with("sextant-browser-direct-incognito-")
    {
        return Err(format!(
            "refusing to remove unexpected direct incognito data dir {}",
            data_dir.display()
        ));
    }

    let mut last_error = None;
    for _ in 0..50 {
        match std::fs::remove_dir_all(data_dir) {
            Ok(()) => return Ok(()),
            Err(_) if !data_dir.exists() => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    Err(format!(
        "direct incognito data cleanup failed for {}: {}",
        data_dir.display(),
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "unknown cleanup error".to_string())
    ))
}

const OPERATOR_TEMP_DIR_PREFIXES: &[&str] = &[
    "sextant-browser-operator-smoke-",
    "sextant-browser-showcase-",
    "sextant-browser-real-browsing-",
    "sextant-browser-intent-run-",
    "sextant-browser-operator-run-",
    "sextant-browser-incognito-",
];

fn sweep_operator_temp_data_dirs() {
    let Ok(entries) = std::fs::read_dir(env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if OPERATOR_TEMP_DIR_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

#[cfg(not(feature = "servo-backend"))]
fn run_direct_servo_browser(
    _window_smoke: Option<WindowSmokeSpec>,
    _startup_input: Option<String>,
    _start_showcase: bool,
    _start_real_browsing: bool,
    _start_shell_interaction: bool,
    _start_location_smoke: Option<String>,
    _start_history_smoke: Option<String>,
    _start_first_interaction_smoke: bool,
    _start_verified_input_smoke: bool,
    _start_search_submit_smoke: bool,
    _start_live_search_smoke: bool,
    _start_live_form_smoke: bool,
    _start_live_link_smoke: bool,
    _direct_resource_audit: bool,
    _start_load_smoke: bool,
    _start_reload_smoke: bool,
    _start_resize_smoke: bool,
    _browser_mode: BrowserMode,
    _pre_size_visible_navigation: bool,
    _certificate_path: Option<PathBuf>,
    _local_appliance_cert_fingerprint: Option<String>,
    _remember_local_appliance_cert: Option<String>,
    _allow_insecure_local_tls: bool,
    _embed_parent_hwnd: Option<isize>,
    _embed_x: i32,
    _embed_y: i32,
    _embed_width: Option<u32>,
    _embed_height: Option<u32>,
) -> Result<(), String> {
    Err("direct Servo render path requires the servo-backend feature".to_string())
}

fn run_operator_mode<F>(label: &'static str, timeout: Duration, run: F) -> !
where
    F: FnOnce() -> Result<Vec<String>, String> + Send + 'static,
{
    let (result_tx, result_rx) = mpsc::channel();
    if let Err(error) = thread::Builder::new()
        .name(format!("sextant-browser-{}", label))
        .spawn(move || {
            let _ = result_tx.send(run());
        })
    {
        eprintln!("[{label}] failed to start worker: {error}");
        std::process::exit(1);
    }

    match result_rx.recv_timeout(timeout) {
        Ok(Ok(report)) => {
            for line in report {
                println!("[{label}] {line}");
            }
            std::process::exit(0);
        }
        Ok(Err(error)) => {
            eprintln!("[{label}] failed: {error}");
            std::process::exit(1);
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            eprintln!(
                "[{label}] timed out after {}s; terminating native-browser operator run",
                timeout.as_secs()
            );
            sweep_operator_temp_data_dirs();
            std::process::exit(124);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            eprintln!("[{label}] worker exited before reporting a result");
            std::process::exit(1);
        }
    }
}

fn run_visible_app(
    window_smoke: Option<WindowSmokeSpec>,
    startup_input: Option<String>,
    start_showcase: bool,
    start_real_browsing: bool,
    start_shell_interaction: bool,
    browser_mode: BrowserMode,
    user_render_path: UserRenderPath,
    pre_size_visible_navigation: bool,
) -> Result<(), String> {
    // Single-instance: only one interactive Sextant window at a time. A second
    // launch focuses the running one (restoring it from the tray) and exits.
    // Smoke runs are exempt so operator/CI checks can overlap.
    if window_smoke.is_none() && !claim_single_instance() {
        return Ok(());
    }

    let event_loop = EventLoopBuilder::<SextantEvent>::with_user_event()
        .build()
        .map_err(|error| format!("event loop initialization failed: {error}"))?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Sextant Browser")
            .with_inner_size(PhysicalSize::new(1180, 760))
            .with_window_icon(sextant_window_icon())
            .build(&event_loop)
            .map_err(|error| format!("window creation failed: {error}"))?,
    );

    let context = Context::new(window.clone())
        .map_err(|error| format!("softbuffer context initialization failed: {error}"))?;
    let mut surface = Surface::new(&context, window.clone())
        .map_err(|error| format!("softbuffer surface initialization failed: {error}"))?;
    let mut surface_size = PhysicalSize::new(0, 0);
    let mut app =
        BrowserApp::new().map_err(|error| format!("browser app initialization failed: {error}"))?;
    if user_render_path != app.user_render_path {
        app.set_user_render_path(user_render_path)?;
    }
    app.pre_size_visible_navigation = pre_size_visible_navigation;
    app.observation_warmup_enabled = true;
    app.set_browser_mode(browser_mode);
    if !app.last_ok {
        return Err(app.last_status.clone());
    }
    app.layout(window.inner_size());
    let visible_started = Instant::now();
    let mut visible_startup_work = Duration::ZERO;
    let mut pending_start_inputs = Vec::<(&'static str, String)>::new();
    let mut active_start_input: Option<(&'static str, String)> = None;
    let mut active_start_action: Option<Action> = None;
    let mut active_start_notice_drawn = false;

    if start_showcase {
        println!("[window-start] running launch showcase");
        let started = Instant::now();
        app.run_showcase_visible();
        visible_startup_work += started.elapsed();
        if !app.last_ok {
            return Err(format!("window showcase start failed: {}", app.last_status));
        }
        println!("[window-start] {}", app.last_status);
    }

    if start_real_browsing {
        println!("[window-start] running real browsing smoke");
        let started = Instant::now();
        app.run_real_browsing_visible();
        visible_startup_work += started.elapsed();
        if !app.last_ok {
            return Err(format!(
                "window real browsing start failed: {}",
                app.last_status
            ));
        }
        println!("[window-start] {}", app.last_status);
    }

    if start_shell_interaction {
        println!("[window-start] running shell interaction smoke");
        let started = Instant::now();
        app.run_shell_interaction_visible();
        visible_startup_work += started.elapsed();
        if !app.last_ok {
            return Err(format!(
                "window shell interaction start failed: {}",
                app.last_status
            ));
        }
        println!("[window-start] {}", app.last_status);
    }

    app.defer_user_navigation = true;

    if let Some(input) = startup_input.as_ref() {
        pending_start_inputs.push(("window-start", input.clone()));
    }

    if let Some(smoke) = window_smoke.as_ref() {
        println!("[window-smoke] starting visible browser shell smoke");
        println!(
            "[window-smoke] browser mode {} ({})",
            app.browser_mode.label(),
            app.browser_mode.status()
        );
        println!(
            "[window-smoke] render path {} ({})",
            app.render_path_label(),
            app.render_path_status()
        );
        if let Some(gap) = app.direct_render_gap_label() {
            println!("[window-smoke] render gap {gap}");
        }
        println!(
            "[window-smoke] pre-size navigation {}",
            app.pre_size_visible_navigation
        );
        if let Some(data_dir) = app.incognito_data_dir.as_ref() {
            println!(
                "[window-smoke] ephemeral browser data dir {}",
                data_dir.display()
            );
        }
        if let Some(target) = smoke.target.as_ref() {
            pending_start_inputs.push(("window-smoke", target.clone()));
        } else if smoke.input_latency {
            pending_start_inputs.push(("window-smoke", input_latency_fixture_url()));
        }
    }

    app.update_title(&window);
    window.request_redraw();

    let smoke_mode = window_smoke.is_some();
    let smoke_deadline = window_smoke
        .as_ref()
        .map(|spec| Instant::now() + spec.timeout);
    let smoke_requires_frame = cfg!(feature = "servo-backend")
        && (startup_input.is_some()
            || start_showcase
            || start_real_browsing
            || start_shell_interaction
            || window_smoke
                .as_ref()
                .and_then(|spec| spec.target.as_ref())
                .is_some()
            || window_smoke
                .as_ref()
                .map(|spec| spec.input_latency)
                .unwrap_or(false));
    let smoke_error = Arc::new(Mutex::new(None::<String>));
    let smoke_error_for_loop = smoke_error.clone();
    let smoke_started = visible_started;
    let mut smoke_first_draw: Option<Duration> = None;
    let mut smoke_first_frame: Option<Duration> = None;
    let mut smoke_first_frame_seen_at: Option<Instant> = None;
    let smoke_requires_user_distill = window_smoke
        .as_ref()
        .map(|spec| spec.user_distill)
        .unwrap_or(false);
    let smoke_requires_input_latency = cfg!(feature = "servo-backend")
        && window_smoke
            .as_ref()
            .map(|spec| spec.input_latency)
            .unwrap_or(false);
    let smoke_input_uses_fixture = window_smoke
        .as_ref()
        .map(|spec| spec.input_latency && spec.target.is_none())
        .unwrap_or(false);
    let mut smoke_user_distill_enqueued = false;
    let mut smoke_user_distill_done = false;
    let mut smoke_input_started: Option<Instant> = None;
    let mut smoke_input_done = false;
    let mut visible_first_draw_seen = false;

    // System tray: close-to-tray keeps the (in-process) Servo engine warm for
    // instant reopen; the tray menu's Quit is the only full exit. Tray and menu
    // events are forwarded onto the winit loop via the user-event proxy so a
    // click wakes the loop even while idle (ControlFlow::Wait).
    //
    // Register the handlers BEFORE building the tray: tray-icon/muda stash the
    // handler in a OnceCell, and the first event sent (which can fire as soon as
    // the icon exists) initializes that cell. Setting the handler afterwards
    // would be a silent no-op and every click would be dropped.
    let tray_proxy = event_loop.create_proxy();
    let menu_proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = tray_proxy.send_event(SextantEvent::TrayIcon(event));
    }));
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(SextantEvent::TrayMenu(event));
    }));
    let tray_menu = Menu::new();
    let show_item = MenuItem::new("Show Sextant", true, None);
    let direct_item = MenuItem::new("New Direct Window\tCtrl+Shift+D", true, None);
    let quit_item = MenuItem::new("Quit Sextant", true, None);
    let _ = tray_menu.append(&show_item);
    let _ = tray_menu.append(&direct_item);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&quit_item);
    let show_id = show_item.id().clone();
    let direct_id = direct_item.id().clone();
    let quit_id = quit_item.id().clone();
    let tray =
        tray_icon::Icon::from_rgba(WINDOW_ICON_RGBA.to_vec(), WINDOW_ICON_DIM, WINDOW_ICON_DIM)
            .ok()
            .and_then(|icon| {
                TrayIconBuilder::new()
                    .with_tooltip("Sextant Browser")
                    .with_icon(icon)
                    .with_menu(Box::new(tray_menu))
                    .build()
                    .ok()
            });

    // Hub-managed direct windows: spawned children are tied to this Job Object,
    // so they (and their Servo children) are killed when the hub process exits.
    let direct_job = create_managed_job();
    let mut direct_children: Vec<Child> = Vec::new();
    let open_direct_window = move |app: &BrowserApp, children: &mut Vec<Child>| {
        let url = app
            .active_tab()
            .and_then(|tab| tab.url.as_ref())
            .map(|url| url.to_string());
        if let Some(child) = spawn_direct_window(direct_job, url) {
            children.push(child);
        }
    };

    let event_loop_result = event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    if tray.is_some() {
                        // Close minimizes to the tray; Servo stays warm. Full
                        // exit is the tray menu's "Quit Sextant".
                        window.set_visible(false);
                    } else {
                        app.cleanup_incognito_storage();
                        elwt.exit();
                    }
                }
                WindowEvent::Resized(size) => {
                    app.layout(size);
                    window.request_redraw();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    app.cursor = Some((position.x, position.y));
                    app.handle_mouse_move(position.x, position.y);
                    window.request_redraw();
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    app.modifiers = modifiers.state();
                }
                WindowEvent::KeyboardInput { event: key_event, .. } => {
                    // Ctrl+Q is a guaranteed quit from the visible window. The
                    // tray's native context menu can be unreliable on Windows
                    // Server / RDP (a background SetForegroundWindow is
                    // restricted there), so always offer a keyboard exit.
                    let pressed = key_event.state == ElementState::Pressed;
                    let quit_chord = pressed
                        && app.modifiers.control_key()
                        && matches!(
                            &key_event.logical_key,
                            Key::Character(c) if c.eq_ignore_ascii_case("q")
                        );
                    // Ctrl+Shift+D opens the current page in a hub-managed direct
                    // (fast, raw-Servo) window.
                    let direct_chord = pressed
                        && app.modifiers.control_key()
                        && app.modifiers.shift_key()
                        && matches!(
                            &key_event.logical_key,
                            Key::Character(c) if c.eq_ignore_ascii_case("d")
                        );
                    if quit_chord {
                        app.cleanup_incognito_storage();
                        elwt.exit();
                    } else if direct_chord {
                        open_direct_window(&app, &mut direct_children);
                    } else {
                        app.handle_key(key_event);
                        app.update_title(&window);
                        window.request_redraw();
                    }
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some((x, y)) = app.cursor {
                        if !app.handle_mouse_input(x, y, state) && state == ElementState::Released {
                            app.click(x, y);
                        }
                        app.update_title(&window);
                        window.request_redraw();
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if let Some((x, y)) = app.cursor {
                        app.scroll_at(x, y, &delta, window.inner_size());
                        window.request_redraw();
                    }
                }
                WindowEvent::RedrawRequested => {
                    match draw(&window, &mut surface, &mut surface_size, &app) {
                        Ok(draw_stats) => {
                            visible_first_draw_seen = true;
                            if active_start_input.is_some() || active_start_action.is_some() {
                                active_start_notice_drawn = true;
                            }
                            if smoke_mode {
                                let draw_elapsed = *smoke_first_draw
                                    .get_or_insert_with(|| smoke_started.elapsed());
                                if smoke_requires_frame && app.latest_frame.is_none() {
                                    if app.frame_refresh_budget == 0 {
                                        app.begin_frame_warmup();
                                    }
                                    return;
                                }
                                if smoke_requires_user_distill && !smoke_user_distill_done {
                                    return;
                                }
                                if app.latest_frame.is_some() && smoke_first_frame.is_none() {
                                    smoke_first_frame = Some(smoke_started.elapsed());
                                    smoke_first_frame_seen_at = Some(Instant::now());
                                }
                                if smoke_requires_input_latency && !smoke_input_done {
                                    return;
                                }
                                println!(
                                    "[window-smoke] visible shell draw passed in {}",
                                    format_duration(draw_elapsed)
                                );
                                if visible_startup_work > Duration::ZERO {
                                    println!(
                                        "[window-smoke] startup/navigation work {}",
                                        format_duration(visible_startup_work)
                                    );
                                }
                                println!(
                                    "[window-smoke] draw cost total {} | frame blit {} | present {}",
                                    format_duration(draw_stats.total),
                                    fmt_duration(draw_stats.frame_blit),
                                    format_duration(draw_stats.present)
                                );
                                println!("[window-smoke] perf {}", app.perf.summary());
                                if let Some(event) = app.slowest_perf_event() {
                                    println!(
                                        "[window-smoke] slowest perf {} {} | {}",
                                        event.phase,
                                        format_duration(event.duration),
                                        event.label
                                    );
                                }
                                let slow_events = app.slowest_perf_events(6);
                                if !slow_events.is_empty() {
                                    let summary = slow_events
                                        .iter()
                                        .map(|event| {
                                            format!(
                                                "{} {} {}",
                                                event.phase,
                                                format_duration(event.duration),
                                                event.label
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" | ");
                                    println!("[window-smoke] slow perf {}", summary);
                                }
                                if let Some(frame) = app.latest_frame.as_ref() {
                                    println!(
                                        "[window-smoke] latest Servo frame {}x{} ({} pixels)",
                                        frame.width,
                                        frame.height,
                                        frame.pixels.len()
                                    );
                                }
                                if let Some(first_frame) = smoke_first_frame {
                                    println!(
                                        "[window-smoke] first Servo frame in {}",
                                        format_duration(first_frame)
                                    );
                                }
                                app.cleanup_incognito_storage();
                                elwt.exit();
                            }
                        }
                        Err(error) => {
                            window.set_title(&format!("Sextant Browser - draw failed: {}", error));
                            if smoke_mode {
                                eprintln!("[window-smoke] draw failed: {error}");
                                if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                    *smoke_error = Some(error.to_string());
                                }
                                app.cleanup_incognito_storage();
                                elwt.exit();
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::UserEvent(SextantEvent::TrayMenu(menu_event)) => {
                if menu_event.id == quit_id {
                    app.cleanup_incognito_storage();
                    elwt.exit();
                } else if menu_event.id == show_id {
                    window.set_visible(true);
                    window.focus_window();
                    window.request_redraw();
                } else if menu_event.id == direct_id {
                    open_direct_window(&app, &mut direct_children);
                }
            }
            Event::UserEvent(SextantEvent::TrayIcon(tray_event)) => {
                // Left-click (or double-click) restores the window; right-click
                // is left to the native context menu on platforms that support
                // it. Quit is the menu's "Quit Sextant" or Ctrl+Q in the window.
                let restore = matches!(
                    tray_event,
                    TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
                        button_state: tray_icon::MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick {
                        button: tray_icon::MouseButton::Left,
                        ..
                    }
                );
                if restore {
                    window.set_visible(true);
                    window.focus_window();
                    window.request_redraw();
                }
            }
            Event::AboutToWait => {
                // Reap direct windows the user has closed (the Job Object handles
                // the kill-on-quit case; this just keeps the tracking list tidy).
                direct_children.retain_mut(|child| matches!(child.try_wait(), Ok(None)));

                if !visible_first_draw_seen
                    && (!pending_start_inputs.is_empty()
                        || app.pending_user_navigation.is_some()
                        || app.pending_user_action.is_some())
                {
                    window.request_redraw();
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                    return;
                }

                if active_start_input.is_none() && active_start_action.is_none() {
                    if let Some(input) = app.pending_user_navigation.take() {
                        active_start_input = Some(("window-user", input));
                        active_start_notice_drawn = false;
                        app.update_title(&window);
                        window.request_redraw();
                        return;
                    }
                    if let Some(action) = app.pending_user_action.take() {
                        active_start_action = Some(action);
                        active_start_notice_drawn = false;
                        app.update_title(&window);
                        window.request_redraw();
                        return;
                    }
                    if let Some((source, input)) = pending_start_inputs.first().cloned() {
                        pending_start_inputs.remove(0);
                        app.address_input = input.clone();
                        app.last_status =
                            format!("Opening {}...", truncate(&input, 80));
                        app.last_ok = true;
                        active_start_input = Some((source, input));
                        active_start_notice_drawn = false;
                        app.update_title(&window);
                        window.request_redraw();
                        return;
                    }
                }

                if (active_start_input.is_some() || active_start_action.is_some())
                    && !active_start_notice_drawn
                {
                    window.request_redraw();
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                    return;
                }

                if let Some(action) = active_start_action.take() {
                    let label = deferred_user_navigation_label(action);
                    println!("[window-user] {label}");
                    let started = Instant::now();
                    let mut started_async = false;
                    match action {
                        Action::Back => {
                            started_async =
                                app.start_visible_navigation_control(PendingNavigationKind::Back);
                        }
                        Action::DistillActive => {
                            started_async = app.start_visible_distillation();
                        }
                        Action::Forward => {
                            started_async =
                                app.start_visible_navigation_control(PendingNavigationKind::Forward);
                        }
                        Action::Reload => {
                            started_async =
                                app.start_visible_navigation_control(PendingNavigationKind::Reload);
                        }
                        _ => {}
                    }
                    if !started_async {
                        visible_startup_work += started.elapsed();
                        app.refresh_logs();
                    }
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                    return;
                }

                if let Some((source, input)) = active_start_input.take() {
                    println!("[{source}] opening {}", input);
                    let started = Instant::now();
                    match start_visible_input(&mut app, &input) {
                        Ok(started_async) => {
                            if !started_async {
                                visible_startup_work += started.elapsed();
                            }
                            println!("[{source}] {}", app.last_status);
                            app.update_title(&window);
                            window.request_redraw();
                        }
                        Err(error) => {
                            visible_startup_work += started.elapsed();
                            if smoke_mode {
                                let error = format!("{source} open failed: {error}");
                                eprintln!("[window-smoke] {error}");
                                if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                    *smoke_error = Some(error);
                                }
                                app.cleanup_incognito_storage();
                                elwt.exit();
                            } else {
                                window.set_title(&format!(
                                    "Sextant Browser - startup open failed: {}",
                                    error
                                ));
                                window.request_redraw();
                            }
                        }
                    }
                    return;
                }

                if let Some(deadline) = smoke_deadline {
                    if Instant::now() >= deadline {
                        let error = "visible shell smoke timed out before first successful draw";
                        eprintln!("[window-smoke] {error}");
                        if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                            *smoke_error = Some(error.to_string());
                        }
                        app.cleanup_incognito_storage();
                        elwt.exit();
                        return;
                    }
                }

                if smoke_requires_input_latency
                    && smoke_first_frame.is_some()
                    && smoke_first_frame_seen_at
                        .map(|seen| seen.elapsed() >= WINDOW_INPUT_SMOKE_FRAME_SETTLE)
                        .unwrap_or(false)
                    && smoke_input_started.is_none()
                    && !smoke_input_done
                    && app.pending_navigation.is_none()
                    && app.pending_frame_capture.is_none()
                    && app.pending_distillation.is_none()
                    && app.pending_persistence.is_none()
                    && app.pending_wake_search.is_none()
                {
                    let started = Instant::now();
                    let clicked = if smoke_input_uses_fixture {
                        true
                    } else {
                        let (x, y) = rect_center(app.browser_viewport_rect);
                        app.handle_mouse_input(x, y, ElementState::Pressed)
                            && app.handle_mouse_input(x, y, ElementState::Released)
                    };
                    app.queue_browser_text("sextant input smoke");
                    let text_flushed = app.flush_pending_browser_text();
                    let input_flushed = app.flush_viewport_input_lane();
                    if !clicked || !text_flushed || !input_flushed {
                        let error = "window input latency smoke failed to enqueue viewport input";
                        eprintln!("[window-smoke] {error}");
                        if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                            *smoke_error = Some(error.to_string());
                        }
                        app.cleanup_incognito_storage();
                        elwt.exit();
                        return;
                    }
                    println!(
                        "[window-smoke] input enqueue {}",
                        format_duration(started.elapsed())
                    );
                    smoke_input_started = Some(Instant::now());
                    window.request_redraw();
                }

                let mut input_flushed = false;
                if app.flush_pending_browser_text_if_due() {
                    input_flushed = true;
                }
                if app.flush_viewport_input_lane() {
                    input_flushed = true;
                }
                if input_flushed {
                    window.request_redraw();
                }
                if let Some(elapsed) = app.collect_pending_navigation() {
                    visible_startup_work += elapsed;
                    app.refresh_logs();
                    app.update_title(&window);
                    if smoke_requires_user_distill
                        && !smoke_user_distill_enqueued
                        && app.last_ok
                        && app.pending_navigation.is_none()
                    {
                        println!(
                            "[window-user] {}",
                            deferred_user_navigation_label(Action::DistillActive)
                        );
                        let started = Instant::now();
                        smoke_user_distill_enqueued = true;
                        if !app.start_visible_distillation() {
                            visible_startup_work += started.elapsed();
                            if smoke_mode {
                                let error =
                                    format!("window user distill failed: {}", app.last_status);
                                eprintln!("[window-smoke] {error}");
                                if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                    *smoke_error = Some(error);
                                }
                                app.cleanup_incognito_storage();
                                elwt.exit();
                                return;
                            }
                        }
                        println!("[window-user] {}", app.last_status);
                    }
                    window.request_redraw();
                }
                if app.collect_pending_observation_warmup().is_some() {
                    window.request_redraw();
                }
                if smoke_requires_user_distill
                    && !smoke_user_distill_enqueued
                    && active_start_input.is_none()
                    && pending_start_inputs.is_empty()
                    && app.pending_navigation.is_none()
                    && app.last_ok
                {
                    println!(
                        "[window-user] {}",
                        deferred_user_navigation_label(Action::DistillActive)
                    );
                    let started = Instant::now();
                    smoke_user_distill_enqueued = true;
                    if !app.start_visible_distillation() {
                        visible_startup_work += started.elapsed();
                        if smoke_mode {
                            let error = format!("window user distill failed: {}", app.last_status);
                            eprintln!("[window-smoke] {error}");
                            if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                *smoke_error = Some(error);
                            }
                            app.cleanup_incognito_storage();
                            elwt.exit();
                            return;
                        }
                    }
                    println!("[window-user] {}", app.last_status);
                    window.request_redraw();
                }
                if let Some(elapsed) = app.collect_pending_distillation() {
                    visible_startup_work += elapsed;
                    if smoke_requires_user_distill {
                        if app.last_ok {
                            if app.pending_persistence.is_none() {
                                smoke_user_distill_done = true;
                            }
                        } else if smoke_mode {
                            let error = format!("window user distill failed: {}", app.last_status);
                            eprintln!("[window-smoke] {error}");
                            if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                *smoke_error = Some(error);
                            }
                            app.cleanup_incognito_storage();
                            elwt.exit();
                            return;
                        }
                    }
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                if let Some(elapsed) = app.collect_pending_persistence() {
                    visible_startup_work += elapsed;
                    if smoke_requires_user_distill {
                        if app.last_ok {
                            smoke_user_distill_done = true;
                        } else if smoke_mode {
                            let error =
                                format!("window user persistence failed: {}", app.last_status);
                            eprintln!("[window-smoke] {error}");
                            if let Ok(mut smoke_error) = smoke_error_for_loop.lock() {
                                *smoke_error = Some(error);
                            }
                            app.cleanup_incognito_storage();
                            elwt.exit();
                            return;
                        }
                    }
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                if app.collect_pending_wake_search().is_some() {
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                if app.collect_pending_log_writes() {
                    app.update_title(&window);
                    window.request_redraw();
                }
                if app.collect_pending_log_refresh() {
                    app.update_title(&window);
                    window.request_redraw();
                }
                #[cfg(feature = "xilem-shell")]
                if app.collect_pending_pilot_plan().is_some() {
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                #[cfg(feature = "xilem-shell")]
                if app.collect_pending_pilot_analysis().is_some() {
                    println!("[window-user] {}", app.last_status);
                    app.update_title(&window);
                    window.request_redraw();
                }
                if app.collect_pending_frame_capture() {
                    if let Some(started) = smoke_input_started {
                        if smoke_requires_input_latency && !smoke_input_done {
                            smoke_input_done = true;
                            println!(
                                "[window-smoke] input frame {}",
                                format_duration(started.elapsed())
                            );
                        }
                    }
                    window.request_redraw();
                }
                if app.maybe_refresh_frame() {
                    window.request_redraw();
                }
                if app.maybe_start_scheduled_observation_warmup() {
                    window.request_redraw();
                }
                if smoke_requires_input_latency
                    && smoke_input_started.is_none()
                    && !smoke_input_done
                    && smoke_first_frame_seen_at.is_some()
                {
                    let wake_at = smoke_first_frame_seen_at
                        .and_then(|seen| seen.checked_add(WINDOW_INPUT_SMOKE_FRAME_SETTLE))
                        .unwrap_or_else(Instant::now);
                    elwt.set_control_flow(ControlFlow::WaitUntil(wake_at));
                } else if app.pending_frame_capture.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if app.pending_distillation.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if app.pending_persistence.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if app.pilot_plan_pending() || app.pilot_analysis_pending() {
                    // Local-model plan/analysis can take seconds; keep the loop
                    // ticking coarsely so lane polls run without input.
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(50),
                    ));
                } else if app.pending_wake_search.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if !app.pending_log_writes.is_empty() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(50),
                    ));
                } else if app.pending_log_refresh.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(50),
                    ));
                } else if !app.pending_viewport_input.is_empty() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(1),
                    ));
                } else if let Some(due) = app.pending_browser_text_due() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(due));
                } else if app.pending_navigation.is_some() {
                    window.request_redraw();
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if app.pending_observation_warmup.is_some() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if let Some(due) = app.scheduled_observation_warmup_due() {
                    elwt.set_control_flow(ControlFlow::WaitUntil(due));
                } else if app.main_view == MainView::Browser
                    && (app.latest_frame.is_some() || app.frame_refresh_budget > 0)
                {
                    let delay = if app.frame_refresh_budget > 0 {
                        app.viewport_input_capture_delay()
                            .unwrap_or(FRAME_REFRESH_INTERACTION)
                    } else if app.frame_dirty {
                        FRAME_REFRESH_DIRTY
                    } else {
                        FRAME_REFRESH_IDLE
                    };
                    let mut wake_at = Instant::now() + delay;
                    if let Some(deadline) = smoke_deadline {
                        if deadline < wake_at {
                            wake_at = deadline;
                        }
                    }
                    elwt.set_control_flow(ControlFlow::WaitUntil(wake_at));
                } else {
                    if let Some(deadline) = smoke_deadline {
                        let pulse = Instant::now() + Duration::from_millis(100);
                        let wake_at = if pulse < deadline { pulse } else { deadline };
                        elwt.set_control_flow(ControlFlow::WaitUntil(wake_at));
                    } else {
                        elwt.set_control_flow(ControlFlow::Wait);
                    }
                }
            }
            _ => {}
        })
        .map_err(|error| format!("event loop failed: {error}"));

    event_loop_result?;

    if let Ok(mut smoke_error) = smoke_error.lock() {
        if let Some(error) = smoke_error.take() {
            return Err(error);
        }
    }

    Ok(())
}
