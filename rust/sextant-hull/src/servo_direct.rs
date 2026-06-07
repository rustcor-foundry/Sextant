use std::env;
use std::time::Duration;

use url::Url;

mod direct_servo;

const DEFAULT_URL: &str = "https://example.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let verified_input_repeat_smoke = args
        .iter()
        .any(|arg| arg == "--verified-input-repeat-smoke");
    let verified_input_smoke =
        verified_input_repeat_smoke || args.iter().any(|arg| arg == "--verified-input-smoke");
    let search_submit_smoke = args.iter().any(|arg| arg == "--search-submit-smoke");
    let live_search_smoke = args.iter().any(|arg| arg == "--live-search-smoke");
    let retained_navigation_smoke = args.iter().any(|arg| arg == "--retained-navigation-smoke");
    let local_fixture = if verified_input_smoke && !has_explicit_target(&args) {
        Some(direct_servo::start_verified_input_fixture_server()?)
    } else if search_submit_smoke && !has_explicit_target(&args) {
        Some(direct_servo::start_search_submit_fixture_server()?)
    } else if retained_navigation_smoke && !has_explicit_target(&args) {
        Some(direct_servo::start_retained_navigation_fixture_server()?)
    } else {
        None
    };
    let live_default = live_search_smoke.then(|| {
        Url::parse("https://lite.duckduckgo.com/lite/").expect("live search URL should parse")
    });
    let target = parse_target(
        &args,
        local_fixture
            .as_ref()
            .map(|fixture| fixture.url())
            .or(live_default.as_ref()),
    )?;
    let smoke = args.iter().any(|arg| arg == "--smoke")
        || verified_input_smoke
        || search_submit_smoke
        || live_search_smoke
        || retained_navigation_smoke
        || args.iter().any(|arg| {
            matches!(
                arg.as_str(),
                "--first-interaction-smoke"
                    | "--load-smoke"
                    | "--reload-smoke"
                    | "--resize-smoke"
                    | "--tab-smoke"
            )
        })
        || parse_scripted_location(&args).is_some()
        || parse_scripted_history(&args).is_some()
        || parse_scripted_text(&args).is_some();
    let timeout = parse_timeout(&args)?;

    let outcome = direct_servo::run(direct_servo::DirectServoOptions {
        target,
        smoke,
        timeout,
        title: "Sextant Servo Direct".to_string(),
        log_prefix: "servo-direct",
        setup_servo_logging: true,
        scripted_text: parse_scripted_text(&args),
        scripted_first_interaction_smoke: args.iter().any(|arg| arg == "--first-interaction-smoke"),
        scripted_verified_input_smoke: verified_input_smoke,
        scripted_verified_input_repeat_smoke: verified_input_repeat_smoke,
        scripted_search_submit_smoke: search_submit_smoke,
        scripted_live_search_smoke: live_search_smoke,
        scripted_retained_navigation_smoke: retained_navigation_smoke,
        scripted_location: parse_scripted_location(&args),
        scripted_history: parse_scripted_history(&args),
        scripted_load_smoke: args.iter().any(|arg| arg == "--load-smoke"),
        scripted_reload_smoke: args.iter().any(|arg| arg == "--reload-smoke"),
        scripted_resize_smoke: args.iter().any(|arg| arg == "--resize-smoke"),
        scripted_tab_smoke: args.iter().any(|arg| arg == "--tab-smoke"),
        resource_audit: args.iter().any(|arg| arg == "--resource-audit"),
        config_dir: parse_config_dir(&args),
        disable_http_cache: args.iter().any(|arg| arg == "--disable-http-cache"),
        certificate_path: parse_path_arg(&args, "--certificate-path"),
        ignore_certificate_errors: args.iter().any(|arg| arg == "--allow-insecure-local-tls"),
        appliance_certificate_actions: None,
        bypass_proxy_for_target: args.iter().any(|arg| arg == "--allow-insecure-local-tls"),
        embed_parent_hwnd: parse_isize_arg(&args, "--embed-parent-hwnd"),
        embed_bounds: parse_embed_bounds(&args),
        host_command_rx: None,
        read_host_commands_from_stdin: false,
    })?;
    if let Some(before_load_complete) = outcome.first_interaction_before_load_complete {
        println!("[servo-direct] first interaction before load complete {before_load_complete}");
    }
    if let Some(verified_input) = outcome.verified_input {
        println!("[servo-direct] verified input {verified_input}");
    }
    if let Some(second_frame) = outcome.verified_input_second_frame {
        println!(
            "[servo-direct] verified repeat input frame {}",
            direct_servo::format_duration(second_frame)
        );
    }
    if let Some(before_load_complete) = outcome.verified_input_before_load_complete {
        println!("[servo-direct] verified input before load complete {before_load_complete}");
    }
    if let Some(search_submit) = outcome.search_submit {
        println!("[servo-direct] search submit {search_submit}");
    }
    if let Some(search_submit_url) = outcome.search_submit_url {
        println!("[servo-direct] search submit url {search_submit_url}");
    }
    if let Some(search_submit_title) = outcome.search_submit_title {
        println!("[servo-direct] search submit title {search_submit_title}");
    }
    if let Some(live_search) = outcome.live_search {
        println!("[servo-direct] live search {live_search}");
    }
    if let Some(live_search_url) = outcome.live_search_url {
        println!("[servo-direct] live search url {live_search_url}");
    }
    if let Some(live_search_title) = outcome.live_search_title {
        println!("[servo-direct] live search title {live_search_title}");
    }
    if let Some(retained_navigation_frame) = outcome.retained_navigation_frame {
        println!(
            "[servo-direct] retained navigation frame {}",
            direct_servo::format_duration(retained_navigation_frame)
        );
    }
    if let Some(retained_navigation_url) = outcome.retained_navigation_url {
        println!("[servo-direct] retained navigation url {retained_navigation_url}");
    }
    if let Some(retained_navigation_title) = outcome.retained_navigation_title {
        println!("[servo-direct] retained navigation title {retained_navigation_title}");
    }
    if let Some(location_url) = outcome.location_url {
        println!("[servo-direct] location url {location_url}");
    }
    if let Some(location_title) = outcome.location_title {
        println!("[servo-direct] location title {location_title}");
    }
    if let Some(history_final_url) = outcome.history_final_url {
        println!("[servo-direct] history final url {history_final_url}");
    }
    if let Some(history_final_title) = outcome.history_final_title {
        println!("[servo-direct] history final title {history_final_title}");
    }
    if let Some(load_url) = outcome.load_url {
        println!("[servo-direct] load url {load_url}");
    }
    if let Some(load_title) = outcome.load_title {
        println!("[servo-direct] load title {load_title}");
    }
    if let Some(reload_url) = outcome.reload_url {
        println!("[servo-direct] reload url {reload_url}");
    }
    if let Some(reload_title) = outcome.reload_title {
        println!("[servo-direct] reload title {reload_title}");
    }
    if let Some(fingerprint) = outcome.certificate_fingerprint_sha256 {
        println!("[servo-direct] certificate fingerprint sha256 {fingerprint}");
    }
    if outcome.appliance_certificate_trust_once_requested {
        println!("[servo-direct] appliance certificate trust once requested true");
    }
    if let Some(remembered) = outcome.appliance_certificate_remembered {
        println!("[servo-direct] appliance certificate remembered {remembered}");
    }
    if let Some(error) = outcome.appliance_certificate_remember_error {
        println!("[servo-direct] appliance certificate remember error {error}");
    }
    if let Some(audit) = outcome.resource_audit_json {
        println!("[servo-direct] resource audit {audit}");
    }
    println!(
        "[servo-direct] frame timing frames={} slow={}",
        outcome.direct_frame_count, outcome.direct_slow_frame_count
    );
    if let Some(max_frame) = outcome.direct_max_frame {
        println!(
            "[servo-direct] max direct frame total={} spin={} paint={} present={}",
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
    if let Some(tab_url) = outcome.tab_url {
        println!("[servo-direct] tab url {tab_url}");
    }
    if let Some(tab_title) = outcome.tab_title {
        println!("[servo-direct] tab title {tab_title}");
    }
    Ok(())
}

fn parse_target(args: &[String], default_target: Option<&Url>) -> Result<Url, String> {
    if let Some(index) = args.iter().position(|arg| arg == "--url") {
        if let Some(value) = args.get(index + 1) {
            return Url::parse(value).map_err(|error| error.to_string());
        }
        return Err("--url requires a URL value".to_string());
    }
    if let Some(value) = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with("--") && arg.parse::<u64>().is_err())
    {
        return Url::parse(value).map_err(|error| error.to_string());
    }
    if let Some(default_target) = default_target {
        return Ok(default_target.clone());
    }
    Url::parse(DEFAULT_URL).map_err(|error| error.to_string())
}

fn has_explicit_target(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--url")
        || args
            .iter()
            .skip(1)
            .any(|arg| !arg.starts_with("--") && arg.parse::<u64>().is_err())
}

fn parse_timeout(args: &[String]) -> Result<Duration, String> {
    let Some(index) = args.iter().position(|arg| arg == "--timeout-seconds") else {
        return Ok(DEFAULT_TIMEOUT);
    };
    let value = args
        .get(index + 1)
        .ok_or_else(|| "--timeout-seconds requires a value".to_string())?;
    let seconds = value.parse::<u64>().map_err(|error| error.to_string())?;
    Ok(Duration::from_secs(seconds.max(1)))
}

fn parse_scripted_text(args: &[String]) -> Option<String> {
    let index = args.iter().position(|arg| arg == "--input-smoke-text")?;
    args.get(index + 1).cloned()
}

fn parse_scripted_location(args: &[String]) -> Option<String> {
    let index = args.iter().position(|arg| arg == "--location-smoke")?;
    args.get(index + 1).cloned()
}

fn parse_scripted_history(args: &[String]) -> Option<String> {
    let index = args.iter().position(|arg| arg == "--history-smoke")?;
    args.get(index + 1).cloned()
}

fn parse_config_dir(args: &[String]) -> Option<std::path::PathBuf> {
    parse_path_arg(args, "--config-dir")
}

fn parse_path_arg(args: &[String], flag: &str) -> Option<std::path::PathBuf> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(std::path::PathBuf::from)
}

fn parse_isize_arg(args: &[String], flag: &str) -> Option<isize> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1)?.parse::<isize>().ok()
}

fn parse_i32_arg(args: &[String], flag: &str) -> Option<i32> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1)?.parse::<i32>().ok()
}

fn parse_u32_arg(args: &[String], flag: &str) -> Option<u32> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1)?.parse::<u32>().ok()
}

fn parse_embed_bounds(args: &[String]) -> Option<direct_servo::DirectEmbedBounds> {
    Some(direct_servo::DirectEmbedBounds {
        x: parse_i32_arg(args, "--embed-x").unwrap_or(0),
        y: parse_i32_arg(args, "--embed-y").unwrap_or(0),
        width: parse_u32_arg(args, "--embed-width")?,
        height: parse_u32_arg(args, "--embed-height")?,
    })
}
