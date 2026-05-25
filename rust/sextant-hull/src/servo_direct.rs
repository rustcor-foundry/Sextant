use std::env;
use std::time::Duration;

use url::Url;

mod direct_servo;

const DEFAULT_URL: &str = "https://example.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let verified_input_smoke = args.iter().any(|arg| arg == "--verified-input-smoke");
    let search_submit_smoke = args.iter().any(|arg| arg == "--search-submit-smoke");
    let live_search_smoke = args.iter().any(|arg| arg == "--live-search-smoke");
    let local_fixture = if verified_input_smoke && !has_explicit_target(&args) {
        Some(direct_servo::start_verified_input_fixture_server()?)
    } else if search_submit_smoke && !has_explicit_target(&args) {
        Some(direct_servo::start_search_submit_fixture_server()?)
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
    let smoke = args.iter().any(|arg| arg == "--smoke");
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
        scripted_search_submit_smoke: search_submit_smoke,
        scripted_live_search_smoke: live_search_smoke,
        scripted_location: parse_scripted_location(&args),
        scripted_history: parse_scripted_history(&args),
        scripted_load_smoke: args.iter().any(|arg| arg == "--load-smoke"),
        scripted_reload_smoke: args.iter().any(|arg| arg == "--reload-smoke"),
        scripted_resize_smoke: args.iter().any(|arg| arg == "--resize-smoke"),
        scripted_tab_smoke: args.iter().any(|arg| arg == "--tab-smoke"),
        config_dir: parse_config_dir(&args),
        disable_http_cache: args.iter().any(|arg| arg == "--disable-http-cache"),
    })?;
    if let Some(before_load_complete) = outcome.first_interaction_before_load_complete {
        println!("[servo-direct] first interaction before load complete {before_load_complete}");
    }
    if let Some(verified_input) = outcome.verified_input {
        println!("[servo-direct] verified input {verified_input}");
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
    let index = args.iter().position(|arg| arg == "--config-dir")?;
    args.get(index + 1).map(std::path::PathBuf::from)
}
