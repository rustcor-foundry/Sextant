use chrono::Utc;
use serde_json::{json, Value};
use sextant_firewall::FirewallPolicy;
use sextant_log::{CaptainsLog, LogEntry, LogStatus};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;
use uuid::Uuid;

const PROTOCOL_VERSION: &str = "2025-11-25";
const PERSONA_ID: &str = "browser-persona";
const SERVER_NAME: &str = "sextant-mcp";
const SERVER_VERSION: &str = "0.1.0";

fn main() {
    if let Err(error) = run() {
        eprintln!("[sextant-mcp] {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--list-tools") {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "tools": tools() }))
                .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--self-test") {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "server": server_info(),
                "capabilities": server_capabilities(),
                "browserCapabilities": browser_capabilities(),
                "tools": tools(),
                "resources": resources(),
            }))
            .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--launch-preflight") {
        let timeout_seconds = parse_cli_timeout(&args, "--timeout-seconds", 60)?;
        let result = browser_launch_preflight_tool(json!({
            "timeout_seconds": timeout_seconds,
            "visible_timeout_seconds": 30,
        }));
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--hardening-preflight") {
        let timeout_seconds = parse_cli_timeout(&args, "--timeout-seconds", 240)?;
        let visible_timeout_seconds = parse_cli_timeout(&args, "--visible-timeout-seconds", 60)?;
        let result = browser_hardening_preflight_tool(json!({
            "timeout_seconds": timeout_seconds,
            "visible_timeout_seconds": visible_timeout_seconds,
        }));
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--real-browsing-smoke") {
        let timeout_seconds = parse_cli_timeout(&args, "--timeout-seconds", 180)?;
        let result = browser_real_browsing_smoke_tool(json!({
            "timeout_seconds": timeout_seconds,
        }));
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(arguments) = browser_render_path_baseline_cli_arguments(&args)? {
        let result = run_browser_render_path_baseline_tool(arguments)?;
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if tool_result_failed(&result) {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(arguments) = browser_direct_present_smoke_cli_arguments(&args)? {
        let result = run_browser_direct_present_smoke_tool(arguments)?;
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if tool_result_failed(&result) {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(arguments) = browser_window_smoke_cli_arguments(&args)? {
        let result = run_browser_window_smoke_tool(arguments)?;
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if tool_result_failed(&result) {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(target) = operator_arg_value(&args, "--perf-probe") {
        let timeout_seconds = parse_cli_timeout(&args, "--timeout-seconds", 180)?;
        let result = browser_perf_probe_tool(json!({
            "target": target,
            "timeout_seconds": timeout_seconds,
        }))?;
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(target) = operator_arg_value(&args, "--perception-probe") {
        let timeout_seconds = parse_cli_timeout(&args, "--timeout-seconds", 180)?;
        let result = browser_perception_probe_tool(json!({
            "target": target,
            "timeout_seconds": timeout_seconds,
        }))?;
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(target) = operator_arg_value(&args, "--guard-probe") {
        let timeout_seconds = parse_cli_timeout(&args, "--timeout-seconds", 120)?;
        let mut arguments = json!({
            "target": target,
            "timeout_seconds": timeout_seconds,
        });
        if let Some(policy_path) = operator_arg_value(&args, "--guard-policy") {
            arguments["policy_path"] = Value::String(policy_path);
        }
        let result = browser_guard_probe_tool(arguments)?;
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--guard-policy-read") {
        let result = browser_guard_policy_read_tool(json!({}));
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(path) = operator_arg_value(&args, "--guard-policy-write") {
        let raw = std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read guard policy source {path}: {error}"))?;
        let policy = serde_json::from_str::<Value>(&raw)
            .map_err(|error| format!("failed to parse guard policy source {path}: {error}"))?;
        let result = browser_guard_policy_write_tool(json!({
            "policy": policy,
            "source": path,
        }));
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--perf-baseline") {
        let timeout_seconds = parse_cli_timeout(&args, "--timeout-seconds", 240)?;
        let result = browser_perf_baseline_tool(json!({
            "timeout_seconds": timeout_seconds,
        }));
        if args.iter().any(|arg| arg == "--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
            );
        } else {
            println!("{}", tool_text(&result));
        }
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::process::exit(1);
        }
        return Ok(());
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle_message(message),
            Err(error) => Some(error_response(
                Value::Null,
                -32700,
                &format!("Parse error: {error}"),
            )),
        };

        if let Some(response) = response {
            let encoded = serde_json::to_string(&response).map_err(|error| error.to_string())?;
            writeln!(stdout, "{encoded}").map_err(|error| error.to_string())?;
            stdout.flush().map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn handle_message(message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = match message.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => {
            return id.map(|id| error_response(id, -32600, "Invalid Request: missing method"));
        }
    };

    match method {
        "initialize" => id.map(|id| {
            response(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": server_capabilities(),
                    "serverInfo": server_info(),
                    "instructions": "Sextant exposes local browser automation through the native sextant-browser operator bridge and a separate direct Servo presentation proof. Start with browser_capabilities, then use browser_direct_present_smoke for direct compositor proof, or browser_real_browsing_smoke, browser_perception_probe, browser_perf_probe, browser_intent_run, browser_operator_probe, or browser_operator_run for bounded real browsing checks.",
                    "browserModes": browser_modes(),
                }),
            )
        }),
        "notifications/initialized" => None,
        "ping" => id.map(|id| response(id, json!({}))),
        "tools/list" => id.map(|id| response(id, json!({ "tools": tools() }))),
        "tools/call" => id.map(|id| handle_tool_call(id, message.get("params").cloned())),
        "resources/list" => id.map(|id| response(id, json!({ "resources": resources() }))),
        "resources/read" => id.map(|id| handle_resource_read(id, message.get("params").cloned())),
        "resources/templates/list" => id.map(|id| response(id, json!({ "resourceTemplates": [] }))),
        "prompts/list" => id.map(|id| response(id, json!({ "prompts": [] }))),
        _ => id.map(|id| error_response(id, -32601, &format!("Method not found: {method}"))),
    }
}

fn handle_tool_call(id: Value, params: Option<Value>) -> Value {
    let Some(params) = params else {
        return error_response(id, -32602, "Invalid params: missing tool call params");
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error_response(id, -32602, "Invalid params: missing tool name");
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result = match name {
        "browser_capabilities" => {
            let _ = record_audit("browser_capabilities", LogStatus::Success, json!({}));
            Ok(tool_success(browser_capabilities()))
        }
        "browser_operator_smoke" => run_browser_operator_tool(
            "browser_operator_smoke",
            operator_timeout(&arguments),
            vec!["--operator-smoke".to_string()],
            json!({ "arguments": arguments }),
        ),
        "browser_showcase_run" => run_browser_operator_tool(
            "browser_showcase_run",
            operator_timeout(&arguments),
            vec!["--showcase-run".to_string()],
            json!({ "arguments": arguments }),
        ),
        "browser_launch_preflight" => Ok(browser_launch_preflight_tool(arguments)),
        "browser_hardening_preflight" => Ok(browser_hardening_preflight_tool(arguments)),
        "browser_real_browsing_smoke" => Ok(browser_real_browsing_smoke_tool(arguments)),
        "browser_render_path_baseline" => run_browser_render_path_baseline_tool(arguments),
        "browser_direct_present_smoke" => run_browser_direct_present_smoke_tool(arguments),
        "browser_window_smoke" => run_browser_window_smoke_tool(arguments),
        "browser_intent_run" => {
            let Some(intent) = required_string(&arguments, "intent") else {
                let _ = record_audit(
                    "browser_intent_run",
                    LogStatus::Failure("missing intent".to_string()),
                    arguments,
                );
                return response(
                    id,
                    tool_error("browser_intent_run requires a string intent argument"),
                );
            };
            let mut args = vec!["--intent-run".to_string(), intent];
            if let Some(expect) = required_string(&arguments, "expect") {
                args.extend(["--expect".to_string(), expect]);
            }
            run_browser_operator_tool(
                "browser_intent_run",
                operator_timeout(&arguments),
                args,
                json!({ "arguments": arguments }),
            )
        }
        "browser_authorized_intent_run" => {
            let Some(intent) = required_string(&arguments, "intent") else {
                let _ = record_audit(
                    "browser_authorized_intent_run",
                    LogStatus::Failure("missing intent".to_string()),
                    arguments,
                );
                return response(
                    id,
                    tool_error("browser_authorized_intent_run requires a string intent argument"),
                );
            };
            let mut args = vec!["--consent-run".to_string(), intent];
            if let Some(expect) = required_string(&arguments, "expect") {
                args.extend(["--expect".to_string(), expect]);
            }
            run_browser_operator_tool(
                "browser_authorized_intent_run",
                operator_timeout(&arguments),
                args,
                json!({ "arguments": arguments }),
            )
        }
        "browser_operator_probe" => {
            let Some(target) = required_string(&arguments, "target") else {
                let _ = record_audit(
                    "browser_operator_probe",
                    LogStatus::Failure("missing target".to_string()),
                    arguments,
                );
                return response(
                    id,
                    tool_error("browser_operator_probe requires a string target argument"),
                );
            };
            run_browser_operator_tool(
                "browser_operator_probe",
                operator_timeout(&arguments),
                vec!["--operator-probe".to_string(), target],
                json!({ "arguments": arguments }),
            )
        }
        "browser_perf_probe" => browser_perf_probe_tool(arguments),
        "browser_perf_baseline" => Ok(browser_perf_baseline_tool(arguments)),
        "browser_perception_probe" => browser_perception_probe_tool(arguments),
        "browser_guard_probe" => browser_guard_probe_tool(arguments),
        "browser_guard_policy_read" => Ok(browser_guard_policy_read_tool(arguments)),
        "browser_guard_policy_write" => Ok(browser_guard_policy_write_tool(arguments)),
        "browser_operator_run" => {
            let Some(target) = required_string(&arguments, "target") else {
                let _ = record_audit(
                    "browser_operator_run",
                    LogStatus::Failure("missing target".to_string()),
                    arguments,
                );
                return response(
                    id,
                    tool_error("browser_operator_run requires a string target argument"),
                );
            };
            match operator_run_args(&arguments, target) {
                Ok(args) => run_browser_operator_tool(
                    "browser_operator_run",
                    operator_timeout(&arguments),
                    args,
                    json!({ "arguments": arguments }),
                ),
                Err(error) => {
                    let _ = record_audit(
                        "browser_operator_run",
                        LogStatus::Failure(error.clone()),
                        arguments,
                    );
                    Ok(tool_error(&error))
                }
            }
        }
        "captains_log_recent" => Ok(captains_log_recent_tool(arguments)),
        _ => return error_response(id, -32602, &format!("Unknown tool: {name}")),
    };

    match result {
        Ok(result) => response(id, result),
        Err(error) => response(id, tool_error(&error)),
    }
}

fn handle_resource_read(id: Value, params: Option<Value>) -> Value {
    let uri = params
        .and_then(|params| params.get("uri").cloned())
        .and_then(|uri| uri.as_str().map(str::to_string));
    let Some(uri) = uri else {
        return error_response(id, -32602, "Invalid params: missing resource uri");
    };

    let Some(text) = resource_text(&uri) else {
        return error_response(id, -32602, &format!("Unknown resource: {uri}"));
    };

    response(
        id,
        json!({
            "contents": [{
                "uri": uri,
                "mimeType": "text/markdown",
                "text": text,
            }]
        }),
    )
}

fn response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

fn server_info() -> Value {
    json!({
        "name": SERVER_NAME,
        "version": SERVER_VERSION,
    })
}

fn server_capabilities() -> Value {
    json!({
        "tools": {
            "listChanged": false,
        },
        "resources": {
            "listChanged": false,
        },
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "browser_capabilities",
            "title": "Browser Capabilities",
            "description": "Describe the native Sextant Browser engine, operator bridge, and supported MCP tools.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": [],
            },
        },
        {
            "name": "browser_operator_smoke",
            "title": "Browser Operator Smoke",
            "description": "Run the deterministic native browser operator smoke check through sextant-browser.",
            "inputSchema": timeout_schema(),
        },
        {
            "name": "browser_showcase_run",
            "title": "Browser Showcase Run",
            "description": "Run the launch-demo browser proof: Intent Bar, Servo navigation, Wake, Log, native form interaction, and frame capture.",
            "inputSchema": timeout_schema(),
        },
        {
            "name": "browser_launch_preflight",
            "title": "Browser Launch Preflight",
            "description": "Run the launch-critical browser checks in sequence: operator smoke, Intent Bar, showcase, and visible showcase smoke.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "timeout_seconds": timeout_property(),
                    "visible_timeout_seconds": window_timeout_property(),
                },
                "required": [],
            },
        },
        {
            "name": "browser_hardening_preflight",
            "title": "Browser Hardening Preflight",
            "description": "Run the broader browser hardening checks in sequence: launch preflight, real browsing, visible real browsing, and visible shell interaction.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "timeout_seconds": timeout_property(),
                    "visible_timeout_seconds": window_timeout_property(),
                },
                "required": [],
            },
        },
        {
            "name": "browser_real_browsing_smoke",
            "title": "Browser Real Browsing Smoke",
            "description": "Run the broader real browsing hardening suite: content navigation, Google search form interaction, documentation page distillation, reload, back/forward, Wake, Log, and frame capture.",
            "inputSchema": timeout_schema(),
        },
        {
            "name": "browser_direct_present_smoke",
            "title": "Browser Direct Present Smoke",
            "description": "Run the experimental direct Servo presentation proof using Servo WindowRenderingContext and present(), bypassing the production frame-capture bridge.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Optional URL to load in the direct Servo presentation proof. Defaults to https://example.com."
                    },
                    "timeout_seconds": window_timeout_property(),
                },
                "required": [],
            },
        },
        {
            "name": "browser_render_path_baseline",
            "title": "Browser Render Path Baseline",
            "description": "Compare direct Servo presentation proof timing against the production bridge-backed visible browser smoke for the same target.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Optional URL to load in both render-path checks. Defaults to https://example.com."
                    },
                    "timeout_seconds": window_timeout_property(),
                },
                "required": [],
            },
        },
        {
            "name": "browser_window_smoke",
            "title": "Browser Window Smoke",
            "description": "Launch the visible sextant-browser shell, optionally navigate to a target, draw once, and exit.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Optional URL or search phrase to open before the visible shell draw check."
                    },
                    "showcase": {
                        "type": "boolean",
                        "description": "When true, run the launch showcase workflow before the visible shell draw check."
                    },
                    "real_browsing": {
                        "type": "boolean",
                        "description": "When true, run the broader real browsing workflow before the visible shell draw check."
                    },
                    "shell_interaction": {
                        "type": "boolean",
                        "description": "When true, click through native chrome controls before the visible shell draw check."
                    },
                    "user_distill": {
                        "type": "boolean",
                        "description": "When true, queue a user-visible DISTILL action through the event-loop async path before the draw check exits."
                    },
                    "mode": browser_mode_property(),
                    "render_path": render_path_property(),
                    "pre_size_navigation": {
                        "type": "boolean",
                        "description": "When true, pass the visible viewport size into Servo navigation before the first bridge capture. Experimental and site-dependent."
                    },
                    "timeout_seconds": window_timeout_property(),
                },
                "required": [],
            },
        },
        {
            "name": "browser_intent_run",
            "title": "Browser Intent Run",
            "description": "Run a native Intent Bar command through sextant-browser, including navigation, optional distillation, Wake, Log, and frame checks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "intent": {
                        "type": "string",
                        "description": "Native Intent Bar text, for example: intent: open https://example.com and distill"
                    },
                    "expect": {
                        "type": "string",
                        "description": "Optional text expected in the distilled page."
                    },
                    "timeout_seconds": timeout_property(),
                },
                "required": ["intent"],
            },
        },
        {
            "name": "browser_authorized_intent_run",
            "title": "Browser Authorized Intent Run",
            "description": "Run a sensitive native Intent Bar command, authorize the pending Captain's Key consent, and resume the bounded browser plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "intent": {
                        "type": "string",
                        "description": "Sensitive Intent Bar text, for example: intent: buy https://example.com and checkout"
                    },
                    "expect": {
                        "type": "string",
                        "description": "Optional text expected in the resumed distilled page."
                    },
                    "timeout_seconds": timeout_property(),
                },
                "required": ["intent"],
            },
        },
        {
            "name": "browser_operator_probe",
            "title": "Browser Operator Probe",
            "description": "Navigate the native browser to a URL or search phrase, distill the page, and report operator results including phase timings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "URL or search phrase to open in the native browser operator bridge."
                    },
                    "timeout_seconds": timeout_property(),
                },
                "required": ["target"],
            },
        },
        {
            "name": "browser_perf_probe",
            "title": "Browser Performance Probe",
            "description": "Run a timed single-page native browsing pass with navigation, distillation, Wake, viewport resize, first frame, and warm frame timings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "URL or search phrase to open in the native browser performance probe."
                    },
                    "timeout_seconds": timeout_property(),
                },
                "required": ["target"],
            },
        },
        {
            "name": "browser_perf_baseline",
            "title": "Browser Performance Baseline",
            "description": "Run the built-in normal-browsing performance baseline across simple and heavier representative pages.",
            "inputSchema": timeout_schema(),
        },
        {
            "name": "browser_perception_probe",
            "title": "Browser Perception Probe",
            "description": "Navigate and distill a target page, then report the native browser's semantic perception summary, counts, key nodes, and timing lines.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "URL or search phrase to open before producing a page perception report."
                    },
                    "timeout_seconds": timeout_property(),
                },
                "required": ["target"],
            },
        },
        {
            "name": "browser_guard_probe",
            "title": "Browser Guard Probe",
            "description": "Evaluate a target page against the browser's local trust-boundary guard: air-gap status, firewall decision, and privacy redaction sample.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "URL or search phrase to evaluate through the local guard path."
                    },
                    "policy_path": {
                        "type": "string",
                        "description": "Optional path to a JSON firewall policy overlay for this guard probe."
                    },
                    "timeout_seconds": timeout_property(),
                },
                "required": ["target"],
            },
        },
        {
            "name": "browser_guard_policy_read",
            "title": "Browser Guard Policy Read",
            "description": "Read the canonical native browser GUARD firewall policy overlay from the local browser data directory.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": [],
            },
        },
        {
            "name": "browser_guard_policy_write",
            "title": "Browser Guard Policy Write",
            "description": "Validate and write the canonical native browser GUARD firewall policy overlay.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "policy": {
                        "type": "object",
                        "description": "Firewall policy overlay with persona_rules and optional global_blacklist."
                    },
                    "source": {
                        "type": "string",
                        "description": "Optional note describing where the policy came from."
                    }
                },
                "required": ["policy"],
            },
        },
        {
            "name": "browser_operator_run",
            "title": "Browser Operator Run",
            "description": "Run an ordered native browser selector script with fill, click, submit, and expect steps.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "URL or search phrase to open before executing the script."
                    },
                    "steps": {
                        "type": "array",
                        "description": "Ordered script steps. Fill steps require selector and value; click/submit steps require selector; expect steps require text.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "enum": ["fill", "click", "submit", "expect"]
                                },
                                "selector": { "type": "string" },
                                "value": { "type": "string" },
                                "text": { "type": "string" }
                            },
                            "required": ["action"]
                        }
                    },
                    "timeout_seconds": timeout_property(),
                },
                "required": ["target"],
            },
        },
        {
            "name": "captains_log_recent",
            "title": "Captain's Log Recent",
            "description": "Read recent Captain's Log entries for the browser persona, including MCP tool audits.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Maximum entries to return."
                    },
                    "persona_id": {
                        "type": "string",
                        "description": "Persona id to query. Defaults to browser-persona."
                    }
                },
                "required": [],
            },
        },
    ])
}

fn timeout_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "timeout_seconds": timeout_property(),
        },
        "required": [],
    })
}

fn timeout_property() -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "maximum": 600,
        "description": "Native browser operator timeout in seconds. Defaults to 120."
    })
}

fn window_timeout_property() -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "maximum": 120,
        "description": "Visible browser smoke timeout in seconds. Defaults to 15."
    })
}

fn browser_mode_property() -> Value {
    json!({
        "type": "string",
        "enum": ["agent", "assisted", "observe", "direct", "incognito"],
        "description": "Optional visible browser mode for a user-facing shell run. Defaults to assisted."
    })
}

fn render_path_property() -> Value {
    json!({
        "type": "string",
        "enum": ["bridge", "direct"],
        "description": "Optional production user render path. `bridge` is currently integrated; `direct` is parsed by sextant-browser but intentionally rejected until production integration lands."
    })
}

fn resources() -> Value {
    json!([
        {
            "uri": "sextant://browser/capabilities",
            "name": "browser-capabilities",
            "title": "Sextant Browser Capabilities",
            "description": "Current truthful native browser capability inventory.",
            "mimeType": "text/markdown",
        },
        {
            "uri": "sextant://browser/operator-workflow",
            "name": "operator-workflow",
            "title": "Native Operator Workflow",
            "description": "How MCP tools map to sextant-browser operator commands.",
            "mimeType": "text/markdown",
        },
        {
            "uri": "sextant://browser/mcp-tools",
            "name": "mcp-tools",
            "title": "Sextant MCP Tool Scope",
            "description": "Tool boundaries for Codex and Claude integration.",
            "mimeType": "text/markdown",
        }
    ])
}

fn resource_text(uri: &str) -> Option<String> {
    match uri {
        "sextant://browser/capabilities" => Some(
            [
                "# Sextant Browser Capabilities",
                "",
                "- Canonical binary: `sextant-browser`",
                "- Default engine: Servo-backed native browser lane",
                "- Fallback lane: `sextant-browser --no-default-features`",
                "- Automation: bounded native operator smoke/probe/run modes",
                "- Showcase: bounded `--showcase-run` proof for launch demos",
                "- Launch preflight: bounded sequence covering operator, intent, showcase, and visible showcase checks",
                "- Hardening preflight: bounded sequence covering launch, real browsing, visible real browsing, and visible shell interaction checks",
                "- Real browsing smoke: bounded sequence covering Google search, heavier pages, reload, back/forward, Wake, Log, and frame capture",
                "- Guard probes: bounded `--guard-probe` path for air-gap status, firewall decisions, and privacy redaction samples",
                "- Guard policy: canonical `guard-policy.json` read/write tools for persona firewall overlays",
                "- Perception probes: bounded `--perception-probe` path for page type, semantic counts, key nodes, source metadata, and timings",
                "- Performance probes: bounded `--perf-probe` and `--perf-baseline` timing paths for navigation, distillation, Wake, viewport resize, and frame capture",
                "- Direct presentation proof: bounded `browser_direct_present_smoke` path for Servo `WindowRenderingContext` + `present()` without the production frame-capture bridge",
                "- Render-path baseline: bounded `browser_render_path_baseline` path comparing direct-present timing with the production bridge-backed visible smoke",
                "- Intent automation: bounded `--intent-run` mode for native Intent Bar workflows",
                "- Authorized intent automation: bounded `--consent-run` mode for Captain's Key consent/resume workflows",
                "- Visible shell checks: bounded `--window-smoke` launch/draw/navigation mode",
                "- Browser modes: Agent, Assisted, Observe, Direct, and Incognito",
                "- Mode boundary: Direct and Incognito block AI DOM/frame observation, Wake reads/writes, Captain's Log content persistence, and MCP exposure",
                "- Render boundary: `sextant-browser` still uses the temporary frame-capture render bridge for user display; `sextant-servo-direct` proves direct Servo presentation separately until the compositor path is integrated into the production shell",
                "- Viewport input lane: visible mouse, wheel, and text input is queued/coalesced before engine enqueue, then Servo prioritizes that input ahead of pending frame capture work",
                "- Persistence: Digital Wake and Captain's Log under the browser data directory",
                "- Current MCP posture: local stdio server, no network listener, tool calls audited to Captain's Log when the data store is available",
            ]
            .join("\n"),
        ),
        "sextant://browser/operator-workflow" => Some(
            [
                "# Native Operator Workflow",
                "",
                "Use `browser_operator_smoke` first when validating the bridge itself.",
                "Use `browser_launch_preflight` before live demos to check the full launch-critical path.",
                "Use `browser_hardening_preflight` before deeper hardening/demo prep to check launch plus broader browsing and visible shell interaction.",
                "Use `browser_showcase_run` for a launch-demo proof across Intent, Servo, Wake, Log, interaction, and frame capture.",
                "Use `browser_real_browsing_smoke` when hardening normal user-side browsing beyond the curated showcase path.",
                "Use `browser_guard_probe` when an agent needs local trust-boundary state for a target page.",
                "Use `browser_guard_policy_read` and `browser_guard_policy_write` when an agent needs to inspect or update the canonical persona firewall overlay.",
                "Use `browser_perception_probe` when an agent needs a structured semantic read of a target page.",
                "Use `browser_perf_probe` or `browser_perf_baseline` when tuning normal-browsing performance.",
                "Use `browser_direct_present_smoke` when validating whether Servo can present directly to a native window without the current frame-capture bridge.",
                "Use `browser_render_path_baseline` when comparing direct Servo presentation timing against the production bridge path for one URL.",
                "Use `browser_intent_run` for Intent Bar workflows such as opening and distilling a page.",
                "Use `browser_authorized_intent_run` for sensitive Intent Bar workflows that should authorize pending consent and resume the bounded browser plan.",
                "Use `browser_operator_probe` for page-level navigation and distillation checks.",
                "Use `browser_operator_run` for ordered selector scripts:",
                "Use `browser_window_smoke` when validating the actual visible shell startup/draw path.",
                "",
                "```json",
                r#"{"target":"https://www.google.com","steps":[{"action":"fill","selector":"textarea[name=q]","value":"Sextant native browser test"},{"action":"submit","selector":"form"},{"action":"expect","text":"Sextant native browser test"}]}"#,
                "```",
                "",
                "Intent example:",
                "",
                "```json",
                r#"{"intent":"intent: open https://example.com and distill","expect":"Example Domain"}"#,
                "```",
            ]
            .join("\n"),
        ),
        "sextant://browser/mcp-tools" => Some(
            [
                "# Sextant MCP Tool Scope",
                "",
                "This first layer advertises only capabilities that exist today.",
                "",
                "- Discovery is handled by `browser_capabilities` and static resources.",
                "- Real browsing is routed through `sextant-browser` operator mode.",
                "- Real browsing hardening is routed through `sextant-browser --real-browsing-smoke`.",
                "- Local guard checks are routed through `sextant-browser --guard-probe` and returned as structured `guard` output.",
                "- Local guard policy management reads and writes the canonical browser `guard-policy.json` overlay.",
                "- Page perception is routed through `sextant-browser --perception-probe` and returned as structured `perception` output.",
                "- Normal-browsing performance timing is routed through `sextant-browser --perf-probe` and `sextant-browser --perf-baseline`.",
                "- Direct Servo presentation timing is routed through `sextant-servo-direct --smoke` and returned as structured `directPresent` output.",
                "- Render-path comparisons run direct Servo presentation plus production bridge-backed Direct-mode visible smoke and return structured `renderPathBaseline` output.",
                "- Launch showcase checks are routed through `sextant-browser --showcase-run`.",
                "- Launch preflight runs the bounded browser commands in sequence and reports each result.",
                "- Hardening preflight runs launch-critical checks plus real-browsing and visible shell-interaction proofs.",
                "- Intent work is routed through `sextant-browser --intent-run`.",
                "- Authorized sensitive intent work is routed through `sextant-browser --consent-run`.",
                "- Visible shell validation is routed through `sextant-browser --window-smoke`.",
                "- Browser modes are advertised as scope boundaries: Agent has full control/observation/persistence, Assisted observes with human-first control, Observe observes without persistence, Direct is the user performance path with AI observation disabled, and Incognito is the human-only path with AI observation and content persistence disabled.",
                "- Current production visible rendering still uses the temporary frame-capture render bridge in every mode; AI frame observation is a separate gated capability. The experimental direct-present proof is exposed separately until production shell integration lands.",
                "- Operator tools are bounded by `timeout_seconds` and return captured stdout/stderr.",
                "- Tool calls are recorded with the `sextant-mcp-stdio` signature when Captain's Log is writable.",
                "- Long-lived shared browser sessions are intentionally deferred until the browser runtime is extracted into reusable session code.",
            ]
            .join("\n"),
        ),
        _ => None,
    }
}

fn browser_modes() -> Value {
    json!([
        {
            "id": "agent",
            "label": "AGENT",
            "order": 1,
            "description": "AI can observe, control, persist, and expose browser tools.",
            "capabilities": {
                "aiControl": true,
                "aiObserveDom": true,
                "aiObserveFrame": true,
                "readWake": true,
                "writeWake": true,
                "writeLogContent": true,
                "exposeMcpTools": true,
                "directRenderRequired": false,
            }
        },
        {
            "id": "assisted",
            "label": "ASSIST",
            "order": 2,
            "description": "Human-first browsing with AI sidecar observation and consent-gated actions.",
            "capabilities": {
                "aiControl": false,
                "aiObserveDom": true,
                "aiObserveFrame": true,
                "readWake": true,
                "writeWake": true,
                "writeLogContent": true,
                "exposeMcpTools": true,
                "directRenderRequired": false,
            }
        },
        {
            "id": "observe",
            "label": "OBSERVE",
            "order": 3,
            "description": "AI can observe the page but cannot act or persist page content.",
            "capabilities": {
                "aiControl": false,
                "aiObserveDom": true,
                "aiObserveFrame": true,
                "readWake": true,
                "writeWake": false,
                "writeLogContent": false,
                "exposeMcpTools": true,
                "directRenderRequired": false,
            }
        },
        {
            "id": "direct",
            "label": "DIRECT",
            "order": 4,
            "description": "Human performance path. AI observation, persistence, and MCP exposure are disabled.",
            "capabilities": {
                "aiControl": false,
                "aiObserveDom": false,
                "aiObserveFrame": false,
                "readWake": false,
                "writeWake": false,
                "writeLogContent": false,
                "exposeMcpTools": false,
                "directRenderRequired": true,
            }
        },
        {
            "id": "incognito",
            "label": "INCOG",
            "order": 5,
            "description": "Human-only private path. AI observation and content persistence are disabled.",
            "capabilities": {
                "aiControl": false,
                "aiObserveDom": false,
                "aiObserveFrame": false,
                "readWake": false,
                "writeWake": false,
                "writeLogContent": false,
                "exposeMcpTools": false,
                "directRenderRequired": true,
            }
        }
    ])
}

fn browser_runtime_loops() -> Value {
    json!({
        "userInteractionLoop": {
            "status": "implemented",
            "owner": "winit event loop",
            "responsibilities": [
                "window input",
                "chrome drawing",
                "visible navigation polling",
                "viewport input-lane flushing",
                "pending text flush",
                "render-bridge frame polling"
            ]
        },
        "viewportInputLane": {
            "status": "implemented",
            "owner": "sextant-browser shell queue plus Servo service scheduler",
            "responsibilities": [
                "coalesce mouse moves before engine enqueue",
                "merge adjacent wheel deltas before engine enqueue",
                "batch printable text before engine enqueue",
                "prioritize queued viewport input ahead of pending frame captures inside the Servo service"
            ],
            "modeBoundary": "available in every browser mode as the normal user interaction path"
        },
        "servoServiceLoop": {
            "status": "implemented",
            "owner": "sextant-engine Servo service thread",
            "responsibilities": [
                "Servo WebView sessions",
                "navigation",
                "DOM evaluation",
                "prioritized input command delivery",
                "frame capture"
            ]
        },
        "renderBridgeLoop": {
            "status": "temporary",
            "owner": "sextant-browser visible frame worker",
            "responsibilities": [
                "resize Servo viewport",
                "capture RenderedFrame pixels",
                "feed softbuffer blit path"
            ],
            "modeBoundary": "available in every mode for user display until direct Servo compositor rendering replaces it"
        },
        "directPresentationLoop": {
            "status": "experimental proof",
            "owner": "sextant-servo-direct",
            "responsibilities": [
                "create Servo WindowRenderingContext",
                "paint WebView directly",
                "present through Servo rendering context without frame readback"
            ],
            "modeBoundary": "not integrated into production browser modes yet; intended replacement for the render bridge in Direct and Incognito"
        },
        "aiObservationLoop": {
            "status": "baseline",
            "owner": "operator/Pilot/MCP proof paths",
            "responsibilities": [
                "DOM distillation",
                "AI frame observation",
                "Wake reads/writes when permitted",
                "Captain's Log content persistence when permitted"
            ],
            "modeBoundary": "disabled in Direct and Incognito"
        },
        "persistenceLoop": {
            "status": "baseline",
            "owner": "Digital Wake and Captain's Log",
            "responsibilities": [
                "Wake search",
                "Wake page records",
                "MCP audit records",
                "consent signatures"
            ],
            "modeBoundary": "content persistence disabled in Observe, Direct, and Incognito; MCP tool audit remains local server-side"
        }
    })
}

fn browser_capabilities() -> Value {
    json!({
        "browser": {
            "canonicalBinary": "sextant-browser",
            "compatibilityAlias": "sextant-hull-lite",
            "defaultEngine": "servo-backed native browser",
            "fallbackEngine": "reader/fetch/distill build via --no-default-features",
            "defaultMode": "assisted",
            "modes": browser_modes(),
            "runtimeLoops": browser_runtime_loops(),
            "renderingBoundary": {
                "userDisplay": "temporary frame-capture render bridge",
                "aiObservation": "separate capability-gated DOM/frame observation path",
                "directCompositor": "experimental proof via sextant-servo-direct",
                "directPresentSmoke": true,
                "renderPathBaseline": true,
                "directAndIncognito": "AI observation and content persistence disabled; render bridge remains only for user display until direct compositor integration lands"
            },
            "operatorBridge": {
                "smoke": true,
                "launchPreflight": true,
                "hardeningPreflight": true,
                "realBrowsingSmoke": true,
                "directPresentSmoke": true,
                "renderPathBaseline": true,
                "guardProbe": true,
                "perfProbe": true,
                "perfBaseline": true,
                "perceptionProbe": true,
                "showcaseRun": true,
                "intentRun": true,
                "authorizedIntentRun": true,
                "probe": true,
                "run": ["fill", "click", "submit", "expect"],
                "timeoutSecondsDefault": 120,
            },
            "visibleShellSmoke": {
                "launchAndDraw": true,
                "targetedNavigation": true,
                "showcaseSeeding": true,
                "realBrowsingSeeding": true,
                "shellInteractionSeeding": true,
                "modeSelection": ["agent", "assisted", "observe", "direct", "incognito"],
                "controlWorkModes": ["agent", "assisted"],
                "nonControlModes": ["observe", "direct", "incognito"],
                "timeoutSecondsDefault": 15,
            }
        },
        "mcp": {
            "transport": "stdio",
            "protocolVersion": PROTOCOL_VERSION,
            "tools": [
                "browser_capabilities",
                "browser_authorized_intent_run",
                "browser_operator_smoke",
                "browser_launch_preflight",
                "browser_hardening_preflight",
                "browser_real_browsing_smoke",
                "browser_direct_present_smoke",
                "browser_render_path_baseline",
                "browser_guard_probe",
                "browser_guard_policy_read",
                "browser_guard_policy_write",
                "browser_perception_probe",
                "browser_perf_probe",
                "browser_perf_baseline",
                "browser_showcase_run",
                "browser_window_smoke",
                "browser_intent_run",
                "browser_operator_probe",
                "browser_operator_run",
                "captains_log_recent"
            ],
            "resources": [
                "sextant://browser/capabilities",
                "sextant://browser/operator-workflow",
                "sextant://browser/mcp-tools"
            ]
        }
    })
}

fn tool_success(structured: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&structured).unwrap_or_else(|_| structured.to_string()),
        }],
        "structuredContent": structured,
    })
}

fn tool_error(message: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": message,
        }],
        "isError": true,
    })
}

fn tool_text(result: &Value) -> String {
    result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn tool_result_failed(result: &Value) -> bool {
    result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || result
            .get("structuredContent")
            .and_then(|structured| structured.get("success"))
            .and_then(Value::as_bool)
            == Some(false)
}

fn required_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn operator_timeout(arguments: &Value) -> u64 {
    timeout_seconds(arguments, 120, 600)
}

fn window_smoke_timeout(arguments: &Value) -> u64 {
    timeout_seconds(arguments, 15, 120)
}

fn optional_browser_mode(arguments: &Value) -> Result<Option<String>, String> {
    let Some(mode) = arguments.get("mode").and_then(Value::as_str) else {
        return Ok(None);
    };
    let normalized = match mode.trim().to_ascii_lowercase().as_str() {
        "agent" => "agent",
        "assist" | "assisted" => "assisted",
        "observe" => "observe",
        "direct" => "direct",
        "incog" | "incognito" => "incognito",
        _ => {
            return Err(format!(
                "mode must be one of agent, assisted, observe, direct, or incognito; got {mode}"
            ));
        }
    };
    Ok(Some(normalized.to_string()))
}

fn optional_render_path(arguments: &Value) -> Result<Option<String>, String> {
    let Some(path) = arguments.get("render_path").and_then(Value::as_str) else {
        return Ok(None);
    };
    let normalized = match path.trim().to_ascii_lowercase().as_str() {
        "bridge" | "frame-bridge" | "frame_bridge" | "render-bridge" | "render_bridge" => "bridge",
        "direct" | "direct-servo" | "direct_servo" | "servo-direct" | "servo_direct" => "direct",
        _ => {
            return Err(format!("render_path must be bridge or direct; got {path}"));
        }
    };
    Ok(Some(normalized.to_string()))
}

fn window_smoke_mode_boundary_error(arguments: &Value, mode: Option<&str>) -> Option<String> {
    if browser_mode_allows_control_work(mode) {
        return None;
    }

    let blocked_work = if arguments
        .get("showcase")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        Some("showcase seeding")
    } else if arguments
        .get("real_browsing")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        Some("real-browsing seeding")
    } else if arguments
        .get("shell_interaction")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        Some("shell-interaction seeding")
    } else if required_string(arguments, "target")
        .as_deref()
        .map(is_native_intent_target)
        .unwrap_or(false)
    {
        Some("native intent target")
    } else if arguments
        .get("user_distill")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && matches!(mode.unwrap_or("assisted"), "direct" | "incognito")
    {
        Some("user-visible distillation")
    } else {
        None
    };

    blocked_work.map(|work| {
        format!(
            "browser_window_smoke mode '{}' blocks {}; use agent or assisted mode for browser-control proof workflows",
            mode.unwrap_or("assisted"),
            work
        )
    })
}

fn browser_mode_allows_control_work(mode: Option<&str>) -> bool {
    matches!(mode.unwrap_or("assisted"), "agent" | "assisted")
}

fn is_native_intent_target(target: &str) -> bool {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return false;
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
            return true;
        }
    }

    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("research ")
        || lower.starts_with("summarize ")
        || lower.starts_with("summarise ")
        || lower.starts_with("remember ")
        || lower.starts_with("distill ")
        || (lower.starts_with("open ")
            && (lower.contains(" and distill")
                || lower.contains(" and remember")
                || lower.contains(" and summarize")
                || lower.contains(" and summarise")))
}

fn timeout_seconds(arguments: &Value, default: u64, max: u64) -> u64 {
    arguments
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .filter(|seconds| *seconds > 0)
        .unwrap_or(default)
        .min(max)
}

fn parse_cli_timeout(args: &[String], flag: &str, default: u64) -> Result<u64, String> {
    let Some(value) = args
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| &pair[1])
    else {
        return Ok(default);
    };
    let seconds = value
        .parse::<u64>()
        .map_err(|error| format!("{flag} expects a positive whole number of seconds: {error}"))?;
    if seconds == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(seconds.min(600))
}

fn operator_arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn browser_window_smoke_cli_arguments(args: &[String]) -> Result<Option<Value>, String> {
    let Some(window_smoke_index) = args.iter().position(|arg| arg == "--window-smoke") else {
        return Ok(None);
    };

    let mut arguments = json!({});
    if let Some(target) = operator_arg_value(args, "--target").or_else(|| {
        args.get(window_smoke_index + 1)
            .filter(|value| !value.starts_with("--"))
            .cloned()
    }) {
        arguments["target"] = Value::String(target);
    }

    let window_timeout = parse_cli_timeout(args, "--window-smoke-timeout", 15)?;
    arguments["timeout_seconds"] = Value::from(parse_cli_timeout(
        args,
        "--timeout-seconds",
        window_timeout,
    )?);

    if let Some(mode) =
        operator_arg_value(args, "--mode").or_else(|| operator_arg_value(args, "--browser-mode"))
    {
        if let Some(mode) = optional_browser_mode(&json!({ "mode": mode }))? {
            arguments["mode"] = Value::String(mode);
        }
    }

    if let Some(render_path) = operator_arg_value(args, "--render-path")
        .or_else(|| operator_arg_value(args, "--user-render-path"))
    {
        if let Some(render_path) = optional_render_path(&json!({ "render_path": render_path }))? {
            arguments["render_path"] = Value::String(render_path);
        }
    }

    if args
        .iter()
        .any(|arg| arg == "--showcase" || arg == "--start-showcase")
    {
        arguments["showcase"] = Value::Bool(true);
    }
    if args
        .iter()
        .any(|arg| arg == "--real-browsing" || arg == "--start-real-browsing")
    {
        arguments["real_browsing"] = Value::Bool(true);
    }
    if args
        .iter()
        .any(|arg| arg == "--shell-interaction" || arg == "--start-shell-interaction")
    {
        arguments["shell_interaction"] = Value::Bool(true);
    }
    if args
        .iter()
        .any(|arg| arg == "--user-distill" || arg == "--window-smoke-distill")
    {
        arguments["user_distill"] = Value::Bool(true);
    }
    if args
        .iter()
        .any(|arg| arg == "--pre-size-navigation" || arg == "--pre-size-visible-navigation")
    {
        arguments["pre_size_navigation"] = Value::Bool(true);
    }

    Ok(Some(arguments))
}

fn browser_render_path_baseline_cli_arguments(args: &[String]) -> Result<Option<Value>, String> {
    let Some(baseline_index) = args.iter().position(|arg| arg == "--render-path-baseline") else {
        return Ok(None);
    };

    let mut arguments = json!({});
    if let Some(target) = operator_arg_value(args, "--target").or_else(|| {
        args.get(baseline_index + 1)
            .filter(|value| !value.starts_with("--"))
            .cloned()
    }) {
        arguments["target"] = Value::String(target);
    }
    arguments["timeout_seconds"] = Value::from(parse_cli_timeout(args, "--timeout-seconds", 45)?);
    Ok(Some(arguments))
}

fn browser_direct_present_smoke_cli_arguments(args: &[String]) -> Result<Option<Value>, String> {
    let Some(smoke_index) = args
        .iter()
        .position(|arg| arg == "--direct-present-smoke" || arg == "--servo-direct-smoke")
    else {
        return Ok(None);
    };

    let mut arguments = json!({});
    if let Some(target) = operator_arg_value(args, "--target").or_else(|| {
        args.get(smoke_index + 1)
            .filter(|value| !value.starts_with("--"))
            .cloned()
    }) {
        arguments["target"] = Value::String(target);
    }
    arguments["timeout_seconds"] = Value::from(parse_cli_timeout(args, "--timeout-seconds", 45)?);
    Ok(Some(arguments))
}

fn operator_run_args(arguments: &Value, target: String) -> Result<Vec<String>, String> {
    let mut args = vec!["--operator-run".to_string(), target];
    let Some(steps) = arguments.get("steps") else {
        return Ok(args);
    };
    let Some(steps) = steps.as_array() else {
        return Err("browser_operator_run steps must be an array".to_string());
    };

    for step in steps {
        let action = step
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| "each browser_operator_run step requires an action".to_string())?;
        match action {
            "fill" => {
                let selector = required_string(step, "selector")
                    .ok_or_else(|| "fill steps require selector".to_string())?;
                let value = required_string(step, "value")
                    .ok_or_else(|| "fill steps require value".to_string())?;
                args.extend(["--fill".to_string(), selector, value]);
            }
            "click" => {
                let selector = required_string(step, "selector")
                    .ok_or_else(|| "click steps require selector".to_string())?;
                args.extend(["--click".to_string(), selector]);
            }
            "submit" => {
                let selector = required_string(step, "selector")
                    .ok_or_else(|| "submit steps require selector".to_string())?;
                args.extend(["--submit".to_string(), selector]);
            }
            "expect" => {
                let text = required_string(step, "text")
                    .ok_or_else(|| "expect steps require text".to_string())?;
                args.extend(["--expect".to_string(), text]);
            }
            other => {
                return Err(format!(
                    "unsupported browser_operator_run action '{other}'; expected fill, click, submit, or expect"
                ));
            }
        }
    }

    Ok(args)
}

fn run_browser_operator_tool(
    intent: &str,
    timeout_seconds: u64,
    operator_args: Vec<String>,
    plan: Value,
) -> Result<Value, String> {
    let mut full_args = vec![
        "--operator-timeout".to_string(),
        timeout_seconds.to_string(),
    ];
    full_args.extend(operator_args);

    let output = match run_browser_command(&full_args) {
        Ok(output) => output,
        Err(error) => {
            let _ = record_audit(intent, LogStatus::Failure(error.clone()), plan);
            return Ok(tool_command_launch_error(&full_args, &error));
        }
    };
    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout))
        .trim()
        .to_string();
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr))
        .trim()
        .to_string();
    let exit_code = output.status.code();
    let success = output.status.success();
    let status = if success {
        LogStatus::Success
    } else {
        LogStatus::Failure(format!("exit code {:?}", exit_code))
    };
    let _ = record_audit(intent, status, plan);

    let report_source = command_report_source(&stdout, &stderr);
    let report = operator_report_lines(&report_source);
    let perf_timings = perf_timing_samples(&report);
    let perf_summary = perf_timing_summary(&perf_timings);
    let perception = perception_report(&report);
    let guard = guard_report(&report);
    let structured = json!({
        "success": success,
        "exitCode": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "operatorReport": report,
        "perfTimings": perf_timings,
        "perfSummary": perf_summary,
        "perception": perception,
        "guard": guard,
        "command": {
            "binary": "sextant-browser",
            "args": full_args,
        }
    });

    let text = operator_result_text(success, exit_code, structured.get("operatorReport"));
    let mut result = json!({
        "content": [{
            "type": "text",
            "text": text,
        }],
        "structuredContent": structured,
    });
    if !success {
        result["isError"] = Value::Bool(true);
    }
    Ok(result)
}

fn browser_perf_probe_tool(arguments: Value) -> Result<Value, String> {
    let Some(target) = required_string(&arguments, "target") else {
        let _ = record_audit(
            "browser_perf_probe",
            LogStatus::Failure("missing target".to_string()),
            arguments,
        );
        return Ok(tool_error(
            "browser_perf_probe requires a string target argument",
        ));
    };
    run_browser_operator_tool(
        "browser_perf_probe",
        operator_timeout(&arguments),
        vec!["--perf-probe".to_string(), target],
        json!({ "arguments": arguments }),
    )
}

fn browser_perf_baseline_tool(arguments: Value) -> Value {
    match run_browser_operator_tool(
        "browser_perf_baseline",
        operator_timeout(&arguments),
        vec!["--perf-baseline".to_string()],
        json!({ "arguments": arguments }),
    ) {
        Ok(result) => result,
        Err(error) => tool_error(&error),
    }
}

fn browser_perception_probe_tool(arguments: Value) -> Result<Value, String> {
    let Some(target) = required_string(&arguments, "target") else {
        let _ = record_audit(
            "browser_perception_probe",
            LogStatus::Failure("missing target".to_string()),
            arguments,
        );
        return Ok(tool_error(
            "browser_perception_probe requires a string target argument",
        ));
    };
    run_browser_operator_tool(
        "browser_perception_probe",
        operator_timeout(&arguments),
        vec!["--perception-probe".to_string(), target],
        json!({ "arguments": arguments }),
    )
}

fn browser_guard_probe_tool(arguments: Value) -> Result<Value, String> {
    let Some(target) = required_string(&arguments, "target") else {
        let _ = record_audit(
            "browser_guard_probe",
            LogStatus::Failure("missing target".to_string()),
            arguments,
        );
        return Ok(tool_error(
            "browser_guard_probe requires a string target argument",
        ));
    };
    let mut args = vec!["--guard-probe".to_string(), target];
    if let Some(policy_path) = required_string(&arguments, "policy_path") {
        args.extend(["--guard-policy".to_string(), policy_path]);
    }
    run_browser_operator_tool(
        "browser_guard_probe",
        operator_timeout(&arguments),
        args,
        json!({ "arguments": arguments }),
    )
}

fn browser_guard_policy_read_tool(arguments: Value) -> Value {
    let result = (|| -> Result<Value, String> {
        let path = browser_guard_policy_path();
        if !path.exists() {
            return Ok(json!({
                "path": path,
                "exists": false,
                "policy": Value::Null,
                "summary": {
                    "personaCount": 0,
                    "personaRuleCount": 0,
                    "globalBlacklistCount": 0,
                }
            }));
        }
        let raw = std::fs::read_to_string(&path).map_err(|error| {
            format!("failed to read guard policy {}: {}", path.display(), error)
        })?;
        let policy = serde_json::from_str::<FirewallPolicy>(&raw).map_err(|error| {
            format!("failed to parse guard policy {}: {}", path.display(), error)
        })?;
        Ok(json!({
            "path": path,
            "exists": true,
            "policy": serde_json::to_value(&policy).unwrap_or(Value::Null),
            "summary": firewall_policy_summary(&policy),
        }))
    })();

    match result {
        Ok(structured) => {
            let _ = record_audit("browser_guard_policy_read", LogStatus::Success, arguments);
            tool_success(structured)
        }
        Err(error) => {
            let _ = record_audit(
                "browser_guard_policy_read",
                LogStatus::Failure(error.clone()),
                arguments,
            );
            tool_error(&error)
        }
    }
}

fn browser_guard_policy_write_tool(arguments: Value) -> Value {
    let result = (|| -> Result<Value, String> {
        let policy_value = arguments
            .get("policy")
            .cloned()
            .ok_or_else(|| "browser_guard_policy_write requires a policy object".to_string())?;
        let policy = serde_json::from_value::<FirewallPolicy>(policy_value)
            .map_err(|error| format!("invalid guard policy: {}", error))?;
        let path = browser_guard_policy_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create guard policy directory {}: {}",
                    parent.display(),
                    error
                )
            })?;
        }
        let encoded = serde_json::to_string_pretty(&policy)
            .map_err(|error| format!("failed to encode guard policy: {}", error))?;
        std::fs::write(&path, format!("{encoded}\n")).map_err(|error| {
            format!("failed to write guard policy {}: {}", path.display(), error)
        })?;
        Ok(json!({
            "path": path,
            "exists": true,
            "policy": serde_json::to_value(&policy).unwrap_or(Value::Null),
            "summary": firewall_policy_summary(&policy),
            "source": arguments.get("source").cloned().unwrap_or(Value::Null),
        }))
    })();

    match result {
        Ok(structured) => {
            let _ = record_audit("browser_guard_policy_write", LogStatus::Success, arguments);
            tool_success(structured)
        }
        Err(error) => {
            let _ = record_audit(
                "browser_guard_policy_write",
                LogStatus::Failure(error.clone()),
                arguments,
            );
            tool_error(&error)
        }
    }
}

fn firewall_policy_summary(policy: &FirewallPolicy) -> Value {
    let persona_rule_count = policy.persona_rules.values().map(Vec::len).sum::<usize>();
    json!({
        "personaCount": policy.persona_rules.len(),
        "personaRuleCount": persona_rule_count,
        "globalBlacklistCount": policy.global_blacklist.len(),
    })
}

fn run_browser_window_smoke_tool(arguments: Value) -> Result<Value, String> {
    let timeout_seconds = window_smoke_timeout(&arguments);
    let mode = optional_browser_mode(&arguments)?;
    let render_path = optional_render_path(&arguments)?;
    if let Some(error) = window_smoke_mode_boundary_error(&arguments, mode.as_deref()) {
        let _ = record_audit(
            "browser_window_smoke",
            LogStatus::Failure(error.clone()),
            json!({ "arguments": arguments }),
        );
        return Ok(tool_error(&error));
    }

    let mut full_args = Vec::new();
    if let Some(mode) = mode {
        full_args.extend(["--browser-mode".to_string(), mode]);
    }
    if let Some(render_path) = render_path {
        full_args.extend(["--render-path".to_string(), render_path]);
    }
    if arguments
        .get("showcase")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        full_args.push("--start-showcase".to_string());
    }
    if arguments
        .get("real_browsing")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        full_args.push("--start-real-browsing".to_string());
    }
    if arguments
        .get("shell_interaction")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        full_args.push("--start-shell-interaction".to_string());
    }
    if arguments
        .get("user_distill")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        full_args.push("--window-smoke-distill".to_string());
    }
    if arguments
        .get("pre_size_navigation")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        full_args.push("--pre-size-navigation".to_string());
    }
    full_args.push("--window-smoke".to_string());
    if let Some(target) = required_string(&arguments, "target") {
        full_args.push(target);
    }
    full_args.extend([
        "--window-smoke-timeout".to_string(),
        timeout_seconds.to_string(),
    ]);

    let output = match run_browser_command(&full_args) {
        Ok(output) => output,
        Err(error) => {
            let _ = record_audit(
                "browser_window_smoke",
                LogStatus::Failure(error.clone()),
                json!({ "arguments": arguments }),
            );
            return Ok(tool_command_launch_error(&full_args, &error));
        }
    };

    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout))
        .trim()
        .to_string();
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr))
        .trim()
        .to_string();
    let exit_code = output.status.code();
    let success = output.status.success();
    let status = if success {
        LogStatus::Success
    } else {
        LogStatus::Failure(format!("exit code {:?}", exit_code))
    };
    let _ = record_audit(
        "browser_window_smoke",
        status,
        json!({ "arguments": arguments }),
    );

    let report_source = command_report_source(&stdout, &stderr);
    let report = window_report_lines(&report_source);
    let window_smoke = window_smoke_summary(&report);
    let structured = json!({
        "success": success,
        "exitCode": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "windowSmokeReport": report,
        "windowSmoke": window_smoke,
        "command": {
            "binary": "sextant-browser",
            "args": full_args,
        }
    });

    let text = command_result_text(
        success,
        exit_code,
        structured.get("windowSmokeReport"),
        "sextant-browser visible smoke",
    );
    let mut result = json!({
        "content": [{
            "type": "text",
            "text": text,
        }],
        "structuredContent": structured,
    });
    if !success {
        result["isError"] = Value::Bool(true);
    }
    Ok(result)
}

fn run_browser_direct_present_smoke_tool(arguments: Value) -> Result<Value, String> {
    let timeout_seconds = window_smoke_timeout(&arguments).max(1);
    let target =
        required_string(&arguments, "target").unwrap_or_else(|| "https://example.com".to_string());
    let full_args = vec![
        "--smoke".to_string(),
        "--url".to_string(),
        target,
        "--timeout-seconds".to_string(),
        timeout_seconds.to_string(),
    ];

    let output = match run_direct_servo_command(&full_args) {
        Ok(output) => output,
        Err(error) => {
            let _ = record_audit(
                "browser_direct_present_smoke",
                LogStatus::Failure(error.clone()),
                json!({ "arguments": arguments }),
            );
            return Ok(tool_direct_command_launch_error(&full_args, &error));
        }
    };

    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout))
        .trim()
        .to_string();
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr))
        .trim()
        .to_string();
    let exit_code = output.status.code();
    let success = output.status.success();
    let status = if success {
        LogStatus::Success
    } else {
        LogStatus::Failure(format!("exit code {:?}", exit_code))
    };
    let _ = record_audit(
        "browser_direct_present_smoke",
        status,
        json!({ "arguments": arguments }),
    );

    let report_source = command_report_source(&stdout, &stderr);
    let report = direct_present_report_lines(&report_source);
    let direct_present = direct_present_summary(&report);
    let structured = json!({
        "success": success,
        "exitCode": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "directPresentReport": report,
        "directPresent": direct_present,
        "command": {
            "binary": "sextant-servo-direct",
            "args": full_args,
        }
    });

    let text = command_result_text(
        success,
        exit_code,
        structured.get("directPresentReport"),
        "sextant-servo-direct direct present smoke",
    );
    let mut result = json!({
        "content": [{
            "type": "text",
            "text": text,
        }],
        "structuredContent": structured,
    });
    if !success {
        result["isError"] = Value::Bool(true);
    }
    Ok(result)
}

fn run_browser_render_path_baseline_tool(arguments: Value) -> Result<Value, String> {
    let timeout_seconds = window_smoke_timeout(&arguments).max(1);
    let target =
        required_string(&arguments, "target").unwrap_or_else(|| "https://example.com".to_string());

    let direct_result = run_browser_direct_present_smoke_tool(json!({
        "target": target.clone(),
        "timeout_seconds": timeout_seconds,
    }))?;
    let bridge_result = run_browser_window_smoke_tool(json!({
        "target": target.clone(),
        "timeout_seconds": timeout_seconds,
        "mode": "direct",
        "render_path": "bridge",
    }))?;
    let pre_sized_bridge_result = run_browser_window_smoke_tool(json!({
        "target": target.clone(),
        "timeout_seconds": timeout_seconds,
        "mode": "direct",
        "render_path": "bridge",
        "pre_size_navigation": true,
    }))?;

    let direct_failed = tool_result_failed(&direct_result);
    let bridge_failed = tool_result_failed(&bridge_result);
    let pre_sized_bridge_failed = tool_result_failed(&pre_sized_bridge_result);
    let success = !direct_failed && !bridge_failed && !pre_sized_bridge_failed;
    let direct_present = direct_result
        .get("structuredContent")
        .and_then(|structured| structured.get("directPresent"))
        .cloned()
        .unwrap_or(Value::Null);
    let window_smoke = bridge_result
        .get("structuredContent")
        .and_then(|structured| structured.get("windowSmoke"))
        .cloned()
        .unwrap_or(Value::Null);
    let pre_sized_window_smoke = pre_sized_bridge_result
        .get("structuredContent")
        .and_then(|structured| structured.get("windowSmoke"))
        .cloned()
        .unwrap_or(Value::Null);
    let baseline =
        render_path_baseline_summary(&direct_present, &window_smoke, &pre_sized_window_smoke);
    let _ = record_audit(
        "browser_render_path_baseline",
        if success {
            LogStatus::Success
        } else {
            LogStatus::Failure("one or more render-path checks failed".to_string())
        },
        json!({ "arguments": arguments }),
    );

    let text = render_path_baseline_text(
        &baseline,
        &direct_result,
        &bridge_result,
        &pre_sized_bridge_result,
    );
    let mut result = json!({
        "content": [{
            "type": "text",
            "text": text,
        }],
        "structuredContent": {
            "success": success,
            "target": target,
            "renderPathBaseline": baseline,
            "directPresentResult": direct_result.get("structuredContent").cloned().unwrap_or(Value::Null),
            "bridgeWindowSmokeResult": bridge_result.get("structuredContent").cloned().unwrap_or(Value::Null),
            "preSizedBridgeWindowSmokeResult": pre_sized_bridge_result.get("structuredContent").cloned().unwrap_or(Value::Null),
        },
    });
    if !success {
        result["isError"] = Value::Bool(true);
    }
    Ok(result)
}

fn render_path_baseline_summary(
    direct_present: &Value,
    window_smoke: &Value,
    pre_sized_window_smoke: &Value,
) -> Value {
    let direct_ms = direct_present.get("firstPresentMs").and_then(Value::as_u64);
    let bridge_first_frame_ms = window_smoke.get("firstFrameMs").and_then(Value::as_u64);
    let bridge_frame_ms = window_smoke.get("frameMs").and_then(Value::as_u64);
    let bridge_resize_ms = window_smoke.get("resizeMs").and_then(Value::as_u64);
    let pre_sized_first_frame_ms = pre_sized_window_smoke
        .get("firstFrameMs")
        .and_then(Value::as_u64);
    let pre_sized_navigation_ms = pre_sized_window_smoke
        .get("navigationMs")
        .and_then(Value::as_u64);
    let pre_sized_resize_ms = pre_sized_window_smoke
        .get("resizeMs")
        .and_then(Value::as_u64);
    let pre_sized_frame_ms = pre_sized_window_smoke
        .get("frameMs")
        .and_then(Value::as_u64);
    let gap_ms = match (bridge_first_frame_ms, direct_ms) {
        (Some(bridge), Some(direct)) => Value::from(bridge as i64 - direct as i64),
        _ => Value::Null,
    };
    let pre_sized_gap_ms = match (pre_sized_first_frame_ms, bridge_first_frame_ms) {
        (Some(pre_sized), Some(bridge)) => Value::from(pre_sized as i64 - bridge as i64),
        _ => Value::Null,
    };

    json!({
        "directFirstPresentMs": direct_ms.map(Value::from).unwrap_or(Value::Null),
        "bridgeFirstFrameMs": bridge_first_frame_ms.map(Value::from).unwrap_or(Value::Null),
        "bridgeFrameMs": bridge_frame_ms.map(Value::from).unwrap_or(Value::Null),
        "bridgeResizeMs": bridge_resize_ms.map(Value::from).unwrap_or(Value::Null),
        "firstFrameGapMs": gap_ms,
        "preSizedBridgeFirstFrameMs": pre_sized_first_frame_ms.map(Value::from).unwrap_or(Value::Null),
        "preSizedBridgeNavigationMs": pre_sized_navigation_ms.map(Value::from).unwrap_or(Value::Null),
        "preSizedBridgeResizeMs": pre_sized_resize_ms.map(Value::from).unwrap_or(Value::Null),
        "preSizedBridgeFrameMs": pre_sized_frame_ms.map(Value::from).unwrap_or(Value::Null),
        "preSizedVsDefaultGapMs": pre_sized_gap_ms,
        "bridgeRenderPath": window_smoke.get("renderPath").cloned().unwrap_or(Value::Null),
        "bridgeRenderGap": window_smoke.get("renderGap").cloned().unwrap_or(Value::Null),
        "directFrameReadback": direct_present.get("frameReadback").cloned().unwrap_or(Value::Null),
        "directProductionIntegrated": direct_present.get("productionIntegrated").cloned().unwrap_or(Value::Null),
    })
}

fn render_path_baseline_text(
    baseline: &Value,
    direct_result: &Value,
    bridge_result: &Value,
    pre_sized_bridge_result: &Value,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "[render-path-baseline] direct first present {} | bridge first frame {} | gap {}",
        format_optional_ms(baseline.get("directFirstPresentMs")),
        format_optional_ms(baseline.get("bridgeFirstFrameMs")),
        format_optional_ms(baseline.get("firstFrameGapMs"))
    ));
    lines.push(format!(
        "[render-path-baseline] pre-sized bridge first frame {} | vs default {}",
        format_optional_ms(baseline.get("preSizedBridgeFirstFrameMs")),
        format_optional_ms(baseline.get("preSizedVsDefaultGapMs"))
    ));
    if let Some(gap) = baseline.get("bridgeRenderGap").and_then(Value::as_str) {
        lines.push(format!("[render-path-baseline] bridge render gap {gap}"));
    }
    if tool_result_failed(direct_result) {
        lines.push("[render-path-baseline] direct-present check failed".to_string());
    }
    if tool_result_failed(bridge_result) {
        lines.push("[render-path-baseline] bridge window-smoke check failed".to_string());
    }
    if tool_result_failed(pre_sized_bridge_result) {
        lines.push("[render-path-baseline] pre-sized bridge window-smoke check failed".to_string());
    }
    lines.join("\n")
}

fn format_optional_ms(value: Option<&Value>) -> String {
    match value.and_then(Value::as_i64) {
        Some(ms) => format!("{ms}ms"),
        None => "pending".to_string(),
    }
}

fn browser_launch_preflight_tool(arguments: Value) -> Value {
    let timeout_seconds = operator_timeout(&arguments);
    let visible_timeout_seconds = preflight_visible_timeout(&arguments, 30);
    let checks = vec![
        PreflightCheck {
            name: "operator smoke",
            args: vec![
                "--operator-timeout".to_string(),
                timeout_seconds.to_string(),
                "--operator-smoke".to_string(),
            ],
            report: ReportKind::Operator,
        },
        PreflightCheck {
            name: "intent run",
            args: vec![
                "--operator-timeout".to_string(),
                timeout_seconds.to_string(),
                "--intent-run".to_string(),
                "intent: open https://example.com and distill".to_string(),
                "--expect".to_string(),
                "Example Domain".to_string(),
            ],
            report: ReportKind::Operator,
        },
        PreflightCheck {
            name: "showcase run",
            args: vec![
                "--operator-timeout".to_string(),
                timeout_seconds.to_string(),
                "--showcase-run".to_string(),
            ],
            report: ReportKind::Operator,
        },
        PreflightCheck {
            name: "visible showcase smoke",
            args: vec![
                "--start-showcase".to_string(),
                "--window-smoke".to_string(),
                "--window-smoke-timeout".to_string(),
                visible_timeout_seconds.to_string(),
            ],
            report: ReportKind::Window,
        },
    ];

    run_preflight_sequence(
        "browser_launch_preflight",
        "launch preflight",
        "launch preflight failed",
        arguments,
        checks,
    )
}

fn browser_hardening_preflight_tool(arguments: Value) -> Value {
    let timeout_seconds = operator_timeout(&arguments);
    let visible_timeout_seconds = preflight_visible_timeout(&arguments, 60);
    let checks = vec![
        PreflightCheck {
            name: "operator smoke",
            args: vec![
                "--operator-timeout".to_string(),
                timeout_seconds.to_string(),
                "--operator-smoke".to_string(),
            ],
            report: ReportKind::Operator,
        },
        PreflightCheck {
            name: "intent run",
            args: vec![
                "--operator-timeout".to_string(),
                timeout_seconds.to_string(),
                "--intent-run".to_string(),
                "intent: open https://example.com and distill".to_string(),
                "--expect".to_string(),
                "Example Domain".to_string(),
            ],
            report: ReportKind::Operator,
        },
        PreflightCheck {
            name: "showcase run",
            args: vec![
                "--operator-timeout".to_string(),
                timeout_seconds.to_string(),
                "--showcase-run".to_string(),
            ],
            report: ReportKind::Operator,
        },
        PreflightCheck {
            name: "real browsing smoke",
            args: vec![
                "--operator-timeout".to_string(),
                timeout_seconds.to_string(),
                "--real-browsing-smoke".to_string(),
            ],
            report: ReportKind::Operator,
        },
        PreflightCheck {
            name: "visible showcase smoke",
            args: vec![
                "--start-showcase".to_string(),
                "--window-smoke".to_string(),
                "--window-smoke-timeout".to_string(),
                visible_timeout_seconds.to_string(),
            ],
            report: ReportKind::Window,
        },
        PreflightCheck {
            name: "visible real browsing smoke",
            args: vec![
                "--start-real-browsing".to_string(),
                "--window-smoke".to_string(),
                "--window-smoke-timeout".to_string(),
                visible_timeout_seconds.to_string(),
            ],
            report: ReportKind::Window,
        },
        PreflightCheck {
            name: "visible shell interaction smoke",
            args: vec![
                "--start-shell-interaction".to_string(),
                "--window-smoke".to_string(),
                "--window-smoke-timeout".to_string(),
                visible_timeout_seconds.to_string(),
            ],
            report: ReportKind::Window,
        },
    ];

    run_preflight_sequence(
        "browser_hardening_preflight",
        "hardening preflight",
        "hardening preflight failed",
        arguments,
        checks,
    )
}

fn preflight_visible_timeout(arguments: &Value, default: u64) -> u64 {
    arguments
        .get("visible_timeout_seconds")
        .and_then(Value::as_u64)
        .filter(|seconds| *seconds > 0)
        .unwrap_or(default)
        .min(120)
}

fn run_preflight_sequence(
    audit_name: &str,
    label: &str,
    failure_message: &str,
    arguments: Value,
    checks: Vec<PreflightCheck>,
) -> Value {
    let started_at = Instant::now();
    let mut results = Vec::new();
    let mut success = true;
    for check in checks {
        let check_result = run_preflight_check(&check);
        success &= check_result
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        results.push(check_result);
    }

    let status = if success {
        LogStatus::Success
    } else {
        LogStatus::Failure(failure_message.to_string())
    };
    let structured = json!({
        "success": success,
        "durationMs": started_at.elapsed().as_millis() as u64,
        "checks": results,
    });
    let _ = record_audit(
        audit_name,
        status,
        json!({ "arguments": arguments, "result": structured }),
    );

    let text = preflight_result_text(&structured, label);
    let mut result = json!({
        "content": [{
            "type": "text",
            "text": text,
        }],
        "structuredContent": structured,
    });
    if !success {
        result["isError"] = Value::Bool(true);
    }
    result
}

fn browser_real_browsing_smoke_tool(arguments: Value) -> Value {
    match run_browser_operator_tool(
        "browser_real_browsing_smoke",
        operator_timeout(&arguments),
        vec!["--real-browsing-smoke".to_string()],
        json!({ "arguments": arguments }),
    ) {
        Ok(result) => result,
        Err(error) => tool_error(&error),
    }
}

struct PreflightCheck {
    name: &'static str,
    args: Vec<String>,
    report: ReportKind,
}

#[derive(Clone, Copy)]
enum ReportKind {
    Operator,
    Window,
}

fn run_preflight_check(check: &PreflightCheck) -> Value {
    let started_at = Instant::now();
    let output = match run_browser_command(&check.args) {
        Ok(output) => output,
        Err(error) => {
            return json!({
                "name": check.name,
                "success": false,
                "durationMs": started_at.elapsed().as_millis() as u64,
                "error": error,
                "command": {
                    "binary": "sextant-browser",
                    "args": check.args,
                }
            });
        }
    };
    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout))
        .trim()
        .to_string();
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr))
        .trim()
        .to_string();
    let report_source = command_report_source(&stdout, &stderr);
    let report = match check.report {
        ReportKind::Operator => operator_report_lines(&report_source),
        ReportKind::Window => window_report_lines(&report_source),
    };
    json!({
        "name": check.name,
        "success": output.status.success(),
        "exitCode": output.status.code(),
        "durationMs": started_at.elapsed().as_millis() as u64,
        "report": report,
        "stdout": stdout,
        "stderr": stderr,
        "command": {
            "binary": "sextant-browser",
            "args": check.args,
        }
    })
}

fn preflight_result_text(structured: &Value, label: &str) -> String {
    let success = structured
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut lines = vec![format!(
        "Sextant {label} {}",
        if success { "passed" } else { "failed" }
    )];
    if let Some(duration_ms) = structured.get("durationMs").and_then(Value::as_u64) {
        lines.push(format!("Duration: {}", format_duration_ms(duration_ms)));
    }
    if let Some(checks) = structured.get("checks").and_then(Value::as_array) {
        if let Some((name, duration_ms)) = slowest_preflight_check(checks) {
            lines.push(format!(
                "Slowest: {} ({})",
                name,
                format_duration_ms(duration_ms)
            ));
        }
        for check in checks {
            let name = check.get("name").and_then(Value::as_str).unwrap_or("check");
            let ok = check
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let duration = check
                .get("durationMs")
                .and_then(Value::as_u64)
                .map(format_duration_ms)
                .map(|duration| format!(" ({duration})"))
                .unwrap_or_default();
            lines.push(format!(
                "{}: {}{}",
                if ok { "PASS" } else { "FAIL" },
                name,
                duration
            ));
            if !ok {
                if let Some(detail) = preflight_failure_detail(check) {
                    lines.push(format!("  {detail}"));
                }
            }
        }
    }
    lines.join("\n")
}

fn preflight_failure_detail(check: &Value) -> Option<String> {
    if let Some(report) = check.get("report").and_then(Value::as_array) {
        if let Some(line) = report.iter().filter_map(Value::as_str).find(|line| {
            let lower = line.to_ascii_lowercase();
            line.starts_with("[sextant-browser]")
                || lower.contains("failed")
                || lower.contains("error")
        }) {
            return Some(line.to_string());
        }
        if let Some(line) = report.first().and_then(Value::as_str) {
            return Some(line.to_string());
        }
    }
    check
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn slowest_preflight_check(checks: &[Value]) -> Option<(String, u64)> {
    checks
        .iter()
        .filter_map(|check| {
            let name = check.get("name").and_then(Value::as_str)?;
            let duration_ms = check.get("durationMs").and_then(Value::as_u64)?;
            Some((name.to_string(), duration_ms))
        })
        .max_by_key(|(_, duration_ms)| *duration_ms)
}

fn format_duration_ms(duration_ms: u64) -> String {
    if duration_ms < 1_000 {
        format!("{duration_ms}ms")
    } else {
        let seconds = duration_ms as f64 / 1_000.0;
        format!("{seconds:.1}s")
    }
}

fn operator_report_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("[operator-")
                || line.starts_with("[intent-run]")
                || line.starts_with("[consent-run]")
                || line.starts_with("[guard-probe]")
                || line.starts_with("[perf-probe]")
                || line.starts_with("[perf-baseline]")
                || line.starts_with("[perception-probe]")
                || line.starts_with("[real-browsing-smoke]")
                || line.starts_with("[showcase-run]")
        })
        .map(str::to_string)
        .collect()
}

fn perf_timing_samples(report: &[String]) -> Vec<Value> {
    report
        .iter()
        .filter_map(|line| perf_timing_sample(line))
        .collect()
}

fn perf_timing_sample(line: &str) -> Option<Value> {
    let perf_index = line.find("perf ")?;
    let target = perf_line_target(line, perf_index);
    let sample = &line[perf_index + "perf ".len()..];
    let (label, timings) = sample.split_once(':')?;
    let mut value = json!({
        "label": label.trim(),
        "target": target,
        "raw": line,
        "navigationMs": Value::Null,
        "distillMs": Value::Null,
        "wakeMs": Value::Null,
        "resizeMs": Value::Null,
        "frameMs": Value::Null,
    });

    for part in timings.split('|') {
        let mut pieces = part.split_whitespace();
        let Some(name) = pieces.next() else {
            continue;
        };
        let Some(duration) = pieces.next() else {
            continue;
        };
        let key = match name {
            "nav" => "navigationMs",
            "distill" => "distillMs",
            "wake" => "wakeMs",
            "resize" => "resizeMs",
            "frame" => "frameMs",
            _ => continue,
        };
        value[key] = parse_perf_duration_ms(duration)
            .map(Value::from)
            .unwrap_or(Value::Null);
    }

    Some(value)
}

fn perf_line_target(line: &str, perf_index: usize) -> Value {
    let prefix = &line[..perf_index];
    let target = if let Some((target, _)) = prefix.split_once(": [") {
        target.trim()
    } else if let Some((_, rest)) = prefix.split_once("] ") {
        rest.trim().trim_end_matches(':').trim()
    } else {
        ""
    };
    if target.is_empty() {
        Value::Null
    } else {
        Value::String(target.to_string())
    }
}

fn parse_perf_duration_ms(value: &str) -> Option<u64> {
    if value == "pending" {
        return None;
    }
    if let Some(ms) = value.strip_suffix("ms") {
        return ms.parse::<u64>().ok();
    }
    if let Some(seconds) = value.strip_suffix('s') {
        return seconds
            .parse::<f64>()
            .ok()
            .map(|seconds| (seconds * 1_000.0).round() as u64);
    }
    None
}

fn perf_timing_summary(samples: &[Value]) -> Value {
    let phases = [
        ("navigationMs", "navigation", "maxNavigationMs"),
        ("distillMs", "distill", "maxDistillMs"),
        ("wakeMs", "wake", "maxWakeMs"),
        ("resizeMs", "resize", "maxResizeMs"),
        ("frameMs", "frame", "maxFrameMs"),
    ];
    let mut summary = json!({
        "sampleCount": samples.len(),
        "maxNavigationMs": Value::Null,
        "maxDistillMs": Value::Null,
        "maxWakeMs": Value::Null,
        "maxResizeMs": Value::Null,
        "maxFrameMs": Value::Null,
        "slowestPhase": Value::Null,
    });
    let mut slowest: Option<(&str, &str, u64, String, Option<String>, String)> = None;

    for (field, phase, max_field) in phases {
        let mut max_value: Option<u64> = None;
        for sample in samples {
            let Some(duration_ms) = sample.get(field).and_then(Value::as_u64) else {
                continue;
            };
            max_value = Some(max_value.map_or(duration_ms, |max| max.max(duration_ms)));
            let label = sample
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let raw = sample
                .get("raw")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let target = sample
                .get("target")
                .and_then(Value::as_str)
                .map(str::to_string);
            if slowest
                .as_ref()
                .map(|(_, _, current_ms, _, _, _)| duration_ms > *current_ms)
                .unwrap_or(true)
            {
                slowest = Some((field, phase, duration_ms, label, target, raw));
            }
        }
        summary[max_field] = max_value.map(Value::from).unwrap_or(Value::Null);
    }

    if let Some((field, phase, duration_ms, label, target, raw)) = slowest {
        summary["slowestPhase"] = json!({
            "field": field,
            "phase": phase,
            "durationMs": duration_ms,
            "label": label,
            "target": target,
            "raw": raw,
        });
    }

    summary
}

fn perception_report(report: &[String]) -> Value {
    let mut summary = Value::Null;
    let mut counts = Value::Null;
    let mut nodes = Vec::new();
    let mut source = Value::Null;

    for line in report {
        if let Some(value) = line.split("perception summary:").nth(1) {
            summary = Value::String(value.trim().to_string());
        } else if let Some(value) = line.split("perception counts:").nth(1) {
            counts = perception_counts(value.trim());
        } else if let Some((index, value)) = perception_node_line(line) {
            nodes.push(json!({
                "index": index,
                "type": value.get(0).copied().unwrap_or_default(),
                "selector": value.get(1).copied().unwrap_or_default(),
                "text": value.get(2).copied().unwrap_or_default(),
                "raw": line,
            }));
        } else if let Some(value) = line.split("perception source:").nth(1) {
            source = Value::String(value.trim().to_string());
        }
    }

    json!({
        "summary": summary,
        "counts": counts,
        "nodes": nodes,
        "source": source,
    })
}

fn guard_report(report: &[String]) -> Value {
    let mut persona = Value::Null;
    let mut policy = Value::Null;
    let mut rules = Value::Null;
    let mut airgap = Value::Null;
    let mut firewall = Value::Null;
    let mut privacy = Value::Null;
    let mut action = Value::Null;
    let mut blocked = false;
    let mut block = Value::Null;
    let mut lines = Vec::new();

    for line in report {
        if let Some(value) = line.split("guard persona:").nth(1) {
            persona = Value::String(value.trim().to_string());
            lines.push(line.clone());
        } else if let Some(value) = line.split("guard policy:").nth(1) {
            policy = Value::String(value.trim().to_string());
            lines.push(line.clone());
        } else if let Some(value) = line.split("guard rules:").nth(1) {
            rules = guard_rule_counts(value.trim());
            lines.push(line.clone());
        } else if let Some(value) = line.split("airgap:").nth(1) {
            airgap = Value::String(value.trim().to_string());
            lines.push(line.clone());
        } else if let Some(value) = line.split("firewall:").nth(1) {
            let value = value.trim();
            firewall = Value::String(value.to_string());
            if let Some(first) = value.split_whitespace().next() {
                action = Value::String(first.to_string());
                blocked |= first == "BLOCK";
            }
            lines.push(line.clone());
        } else if let Some(value) = line.split("privacy:").nth(1) {
            privacy = Value::String(value.trim().to_string());
            lines.push(line.clone());
        } else if line.contains("privacy sample:") {
            lines.push(line.clone());
        } else if line.contains("Local guard blocked") {
            blocked = true;
            block = Value::String(line.clone());
            lines.push(line.clone());
        } else if line.contains("browser guard probe blocked navigation") {
            blocked = true;
            lines.push(line.clone());
        }
    }

    json!({
        "persona": persona,
        "policy": policy,
        "rules": rules,
        "airgap": airgap,
        "firewall": firewall,
        "action": action,
        "blocked": blocked,
        "block": block,
        "privacy": privacy,
        "lines": lines,
    })
}

fn guard_rule_counts(value: &str) -> Value {
    let mut counts = json!({
        "persona": Value::Null,
        "globalBlacklist": Value::Null,
    });
    for part in value.split_whitespace() {
        let Some((name, raw)) = part.split_once('=') else {
            continue;
        };
        match name {
            "persona" => {
                counts["persona"] = raw.parse::<u64>().map(Value::from).unwrap_or(Value::Null);
            }
            "global_blacklist" => {
                counts["globalBlacklist"] =
                    raw.parse::<u64>().map(Value::from).unwrap_or(Value::Null);
            }
            _ => {}
        }
    }
    counts
}

fn perception_counts(value: &str) -> Value {
    let mut counts = json!({
        "headings": Value::Null,
        "links": Value::Null,
        "inputs": Value::Null,
        "images": Value::Null,
        "text": Value::Null,
        "buttons": Value::Null,
    });
    for part in value.split_whitespace() {
        let Some((name, raw)) = part.split_once('=') else {
            continue;
        };
        let key = match name {
            "headings" => "headings",
            "links" => "links",
            "inputs" => "inputs",
            "images" => "images",
            "text" => "text",
            "buttons" => "buttons",
            _ => continue,
        };
        counts[key] = raw.parse::<u64>().map(Value::from).unwrap_or(Value::Null);
    }
    counts
}

fn perception_node_line(line: &str) -> Option<(u64, Vec<&str>)> {
    let marker = "perception node ";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let (index, value) = rest.split_once(':')?;
    let pieces = value.split('|').map(str::trim).collect::<Vec<_>>();
    Some((index.trim().parse().ok()?, pieces))
}

fn window_report_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("[window-smoke]")
                || line.starts_with("[window-start]")
                || line.starts_with("[window-user]")
                || line.starts_with("[sextant-browser]")
        })
        .map(str::to_string)
        .collect()
}

fn direct_present_report_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("[servo-direct]"))
        .map(str::to_string)
        .collect()
}

fn direct_present_summary(report: &[String]) -> Value {
    let mut summary = json!({
        "target": Value::Null,
        "firstPresentMs": Value::Null,
        "renderingContext": "Servo WindowRenderingContext",
        "frameReadback": false,
        "productionIntegrated": false,
    });

    for line in report {
        if let Some(value) = line.split("opening ").nth(1) {
            if let Some((target, _)) = value.split_once(" with Servo WindowRenderingContext") {
                summary["target"] = Value::String(target.trim().to_string());
            }
        } else if let Some(value) = line.split("first direct present in ").nth(1) {
            summary["firstPresentMs"] = duration_token_to_value(value.trim());
        }
    }

    summary
}

fn window_smoke_summary(report: &[String]) -> Value {
    let mut summary = json!({
        "mode": Value::Null,
        "modeStatus": Value::Null,
        "storage": "profile",
        "ephemeralDataDir": Value::Null,
        "renderPath": Value::Null,
        "renderPathStatus": Value::Null,
        "renderGap": Value::Null,
        "preSizeNavigation": Value::Null,
        "firstDrawMs": Value::Null,
        "startupWorkMs": Value::Null,
        "drawTotalMs": Value::Null,
        "frameBlitMs": Value::Null,
        "presentMs": Value::Null,
        "navigationMs": Value::Null,
        "distillMs": Value::Null,
        "wakeMs": Value::Null,
        "resizeMs": Value::Null,
        "frameMs": Value::Null,
        "slowestPerfEvent": Value::Null,
        "slowPerfEvents": [],
        "latestFrameWidth": Value::Null,
        "latestFrameHeight": Value::Null,
        "latestFramePixels": Value::Null,
        "firstFrameMs": Value::Null,
    });

    for line in report {
        if let Some((mode, status)) = window_mode_line(line) {
            summary["mode"] = Value::String(mode);
            summary["modeStatus"] = Value::String(status);
        } else if let Some((path, status)) = window_render_path_line(line) {
            summary["renderPath"] = Value::String(path);
            summary["renderPathStatus"] = Value::String(status);
        } else if let Some(gap) = line.split("render gap ").nth(1) {
            summary["renderGap"] = Value::String(gap.trim().to_string());
        } else if let Some(value) = line.split("pre-size navigation ").nth(1) {
            summary["preSizeNavigation"] = Value::Bool(value.trim() == "true");
        } else if let Some(path) = line.split("ephemeral browser data dir ").nth(1) {
            summary["storage"] = Value::String("ephemeral".to_string());
            summary["ephemeralDataDir"] = Value::String(path.trim().to_string());
        } else if let Some(value) = line.split("visible shell draw passed in ").nth(1) {
            summary["firstDrawMs"] = duration_token_to_value(value.trim());
        } else if let Some(value) = line.split("startup/navigation work ").nth(1) {
            summary["startupWorkMs"] = duration_token_to_value(value.trim());
        } else if let Some(value) = line.split("draw cost total ").nth(1) {
            apply_window_draw_costs(&mut summary, value);
        } else if let Some(value) = line.strip_prefix("[window-smoke] slowest perf ") {
            summary["slowestPerfEvent"] = window_slowest_perf_event(value).unwrap_or(Value::Null);
        } else if let Some(value) = line.strip_prefix("[window-smoke] slow perf ") {
            summary["slowPerfEvents"] = Value::Array(window_perf_events(value));
        } else if let Some(value) = line.strip_prefix("[window-smoke] perf ") {
            apply_window_perf_summary(&mut summary, value);
        } else if let Some(frame) = window_latest_frame(line) {
            summary["latestFrameWidth"] = Value::from(frame.0);
            summary["latestFrameHeight"] = Value::from(frame.1);
            summary["latestFramePixels"] = Value::from(frame.2);
        } else if let Some(value) = line.split("first Servo frame in ").nth(1) {
            summary["firstFrameMs"] = duration_token_to_value(value.trim());
        }
    }

    summary
}

fn window_render_path_line(line: &str) -> Option<(String, String)> {
    let value = line.split("render path ").nth(1)?;
    let (path, rest) = value.split_once(" (")?;
    Some((
        path.trim().to_string(),
        rest.trim_end_matches(')').trim().to_string(),
    ))
}

fn window_mode_line(line: &str) -> Option<(String, String)> {
    let value = line.split("browser mode ").nth(1)?;
    let (mode, rest) = value.split_once(" (")?;
    Some((
        mode.trim().to_string(),
        rest.trim_end_matches(')').trim().to_string(),
    ))
}

fn apply_window_draw_costs(summary: &mut Value, value: &str) {
    for part in value.split('|') {
        let part = part.trim();
        if let Some(duration) = part.strip_prefix("frame blit ") {
            summary["frameBlitMs"] = duration_token_to_value(duration.trim());
        } else if let Some(duration) = part.strip_prefix("present ") {
            summary["presentMs"] = duration_token_to_value(duration.trim());
        } else {
            summary["drawTotalMs"] = duration_token_to_value(part);
        }
    }
}

fn apply_window_perf_summary(summary: &mut Value, value: &str) {
    for part in value.split('|') {
        let mut pieces = part.split_whitespace();
        let Some(name) = pieces.next() else {
            continue;
        };
        let Some(duration) = pieces.next() else {
            continue;
        };
        let key = match name {
            "nav" => "navigationMs",
            "distill" => "distillMs",
            "wake" => "wakeMs",
            "resize" => "resizeMs",
            "frame" => "frameMs",
            _ => continue,
        };
        summary[key] = duration_token_to_value(duration);
    }
}

fn window_perf_events(value: &str) -> Vec<Value> {
    value.split('|').filter_map(window_perf_event).collect()
}

fn window_slowest_perf_event(value: &str) -> Option<Value> {
    let (timing, label) = value.trim().split_once(" | ")?;
    let mut event = window_perf_event(timing)?;
    event["label"] = Value::String(label.trim().to_string());
    Some(event)
}

fn window_perf_event(value: &str) -> Option<Value> {
    let mut pieces = value.trim().splitn(3, char::is_whitespace);
    let phase = pieces.next()?.trim();
    let duration = pieces.next()?.trim();
    let label = pieces.next().unwrap_or("").trim();
    if phase.is_empty() || duration.is_empty() {
        return None;
    }
    Some(json!({
        "phase": phase,
        "durationMs": duration_token_to_value(duration),
        "label": label,
    }))
}

fn window_latest_frame(line: &str) -> Option<(u64, u64, u64)> {
    let value = line.split("latest Servo frame ").nth(1)?;
    let (size, rest) = value.split_once(" (")?;
    let (width, height) = size.split_once('x')?;
    let pixels = rest
        .trim_end_matches(')')
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some((
        width.trim().parse().ok()?,
        height.trim().parse().ok()?,
        pixels,
    ))
}

fn duration_token_to_value(value: &str) -> Value {
    parse_perf_duration_ms(value)
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn command_report_source(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{stdout}\n{stderr}")
    }
}

fn tool_command_launch_error(args: &[String], error: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": format!("failed to launch sextant-browser command: {error}"),
        }],
        "isError": true,
        "structuredContent": {
            "success": false,
            "error": error,
            "command": {
                "binary": "sextant-browser",
                "args": args,
            }
        },
    })
}

fn tool_direct_command_launch_error(args: &[String], error: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": format!("failed to launch sextant-servo-direct command: {error}"),
        }],
        "isError": true,
        "structuredContent": {
            "success": false,
            "error": error,
            "command": {
                "binary": "sextant-servo-direct",
                "args": args,
            }
        },
    })
}

fn operator_result_text(success: bool, exit_code: Option<i32>, report: Option<&Value>) -> String {
    command_result_text(success, exit_code, report, "sextant-browser")
}

fn command_result_text(
    success: bool,
    exit_code: Option<i32>,
    report: Option<&Value>,
    label: &str,
) -> String {
    let lines = report
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if lines.is_empty() {
        return format!("{label} exited with code {exit_code:?}; success={success}");
    }

    lines.join("\n")
}

fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for code in chars.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn run_browser_command(args: &[String]) -> Result<Output, String> {
    let runner = BrowserRunner::locate()?;
    match runner {
        BrowserRunner::Binary(path) => Command::new(path)
            .args(args)
            .output()
            .map_err(|error| error.to_string()),
        BrowserRunner::Cargo { workspace } => {
            let mut cargo_args = vec![
                "run".to_string(),
                "-p".to_string(),
                "sextant-hull".to_string(),
                "--bin".to_string(),
                "sextant-browser".to_string(),
                "--".to_string(),
            ];
            cargo_args.extend(args.iter().cloned());
            Command::new("cargo")
                .current_dir(workspace)
                .args(cargo_args)
                .output()
                .map_err(|error| error.to_string())
        }
    }
}

fn run_direct_servo_command(args: &[String]) -> Result<Output, String> {
    let runner = DirectServoRunner::locate()?;
    match runner {
        DirectServoRunner::Binary(path) => Command::new(path)
            .args(args)
            .output()
            .map_err(|error| error.to_string()),
        DirectServoRunner::Cargo { workspace } => {
            let mut cargo_args = vec![
                "run".to_string(),
                "-p".to_string(),
                "sextant-hull".to_string(),
                "--bin".to_string(),
                "sextant-servo-direct".to_string(),
                "--".to_string(),
            ];
            cargo_args.extend(args.iter().cloned());
            Command::new("cargo")
                .current_dir(workspace)
                .args(cargo_args)
                .output()
                .map_err(|error| error.to_string())
        }
    }
}

enum BrowserRunner {
    Binary(PathBuf),
    Cargo { workspace: PathBuf },
}

impl BrowserRunner {
    fn locate() -> Result<Self, String> {
        let prefer_installed_binary = env::var("SEXTANT_MCP_USE_INSTALLED_BROWSER")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);

        if !prefer_installed_binary {
            if let Some(workspace) = find_rust_workspace() {
                return Ok(Self::Cargo { workspace });
            }
        }

        if let Ok(exe) = env::current_exe() {
            if let Some(parent) = exe.parent() {
                let name = if cfg!(windows) {
                    "sextant-browser.exe"
                } else {
                    "sextant-browser"
                };
                let sibling = parent.join(name);
                if sibling.is_file() {
                    return Ok(Self::Binary(sibling));
                }
            }
        }

        if let Some(workspace) = find_rust_workspace() {
            return Ok(Self::Cargo { workspace });
        }

        Err("could not find sextant-browser binary or Rust workspace".to_string())
    }
}

enum DirectServoRunner {
    Binary(PathBuf),
    Cargo { workspace: PathBuf },
}

impl DirectServoRunner {
    fn locate() -> Result<Self, String> {
        let prefer_installed_binary = env::var("SEXTANT_MCP_USE_INSTALLED_BROWSER")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);

        if !prefer_installed_binary {
            if let Some(workspace) = find_rust_workspace() {
                return Ok(Self::Cargo { workspace });
            }
        }

        if let Ok(exe) = env::current_exe() {
            if let Some(parent) = exe.parent() {
                let name = if cfg!(windows) {
                    "sextant-servo-direct.exe"
                } else {
                    "sextant-servo-direct"
                };
                let sibling = parent.join(name);
                if sibling.is_file() {
                    return Ok(Self::Binary(sibling));
                }
            }
        }

        if let Some(workspace) = find_rust_workspace() {
            return Ok(Self::Cargo { workspace });
        }

        Err("could not find sextant-servo-direct binary or Rust workspace".to_string())
    }
}

fn find_rust_workspace() -> Option<PathBuf> {
    let mut seeds = Vec::new();
    if let Ok(current_dir) = env::current_dir() {
        seeds.push(current_dir);
    }
    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            seeds.push(parent.to_path_buf());
        }
    }

    for seed in seeds {
        for ancestor in seed.ancestors() {
            if is_rust_workspace(ancestor) {
                return Some(ancestor.to_path_buf());
            }
        }
    }
    None
}

fn is_rust_workspace(path: &Path) -> bool {
    path.join("Cargo.toml").is_file() && path.join("sextant-hull").join("Cargo.toml").is_file()
}

fn captains_log_recent_tool(arguments: Value) -> Value {
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 50) as usize;
    let persona_id = arguments
        .get("persona_id")
        .and_then(Value::as_str)
        .unwrap_or(PERSONA_ID);

    let result = (|| -> Result<Value, String> {
        let log = CaptainsLog::new(browser_data_dir().join("captains-log.db"))
            .map_err(|error| error.to_string())?;
        let entries = log
            .get_entries(persona_id, limit)
            .map_err(|error| error.to_string())?;
        Ok(json!({
            "personaId": persona_id,
            "limit": limit,
            "entries": entries.into_iter().map(|entry| {
                json!({
                    "id": entry.id,
                    "timestamp": entry.timestamp,
                    "intent": entry.intent,
                    "signature": entry.signature,
                    "status": entry.status,
                    "plan": serde_json::from_str::<Value>(&entry.plan_json).unwrap_or(Value::Null),
                })
            }).collect::<Vec<_>>(),
        }))
    })();

    match result {
        Ok(structured) => {
            let _ = record_audit("captains_log_recent", LogStatus::Success, arguments);
            tool_success(structured)
        }
        Err(error) => {
            let _ = record_audit(
                "captains_log_recent",
                LogStatus::Failure(error.clone()),
                arguments,
            );
            tool_error(&error)
        }
    }
}

fn record_audit(intent: &str, status: LogStatus, plan: Value) -> Result<(), String> {
    let data_dir = browser_data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    let log =
        CaptainsLog::new(data_dir.join("captains-log.db")).map_err(|error| error.to_string())?;
    log.record(&LogEntry {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        persona_id: PERSONA_ID.to_string(),
        intent: intent.to_string(),
        plan_json: serde_json::to_string(&plan).unwrap_or_else(|_| "{}".to_string()),
        signature: "sextant-mcp-stdio".to_string(),
        consent_signature: None,
        status,
    })
    .map_err(|error| error.to_string())
}

fn browser_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sextant")
        .join("browser")
}

fn browser_guard_policy_path() -> PathBuf {
    browser_data_dir().join("guard-policy.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_browser_tools() {
        let tools = tools();
        let names = tools
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(names.contains(&"browser_capabilities"));
        assert!(names.contains(&"browser_authorized_intent_run"));
        assert!(names.contains(&"browser_direct_present_smoke"));
        assert!(names.contains(&"browser_render_path_baseline"));
        assert!(names.contains(&"browser_guard_probe"));
        assert!(names.contains(&"browser_guard_policy_read"));
        assert!(names.contains(&"browser_guard_policy_write"));
        assert!(names.contains(&"browser_hardening_preflight"));
        assert!(names.contains(&"browser_intent_run"));
        assert!(names.contains(&"browser_launch_preflight"));
        assert!(names.contains(&"browser_operator_run"));
        assert!(names.contains(&"browser_perception_probe"));
        assert!(names.contains(&"browser_perf_probe"));
        assert!(names.contains(&"browser_perf_baseline"));
        assert!(names.contains(&"browser_real_browsing_smoke"));
        assert!(names.contains(&"browser_showcase_run"));
        assert!(names.contains(&"browser_window_smoke"));

        let window_smoke = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("browser_window_smoke"))
            .unwrap();
        assert_eq!(
            window_smoke["inputSchema"]["properties"]["mode"]["enum"][3],
            "direct"
        );
        assert_eq!(
            window_smoke["inputSchema"]["properties"]["render_path"]["enum"][0],
            "bridge"
        );
    }

    #[test]
    fn translates_ordered_operator_steps() {
        let args = operator_run_args(
            &json!({
                "steps": [
                    {"action": "fill", "selector": "#q", "value": "sextant"},
                    {"action": "submit", "selector": "form"},
                    {"action": "expect", "text": "sextant"}
                ]
            }),
            "https://example.com".to_string(),
        )
        .unwrap();

        assert_eq!(
            args,
            vec![
                "--operator-run",
                "https://example.com",
                "--fill",
                "#q",
                "sextant",
                "--submit",
                "form",
                "--expect",
                "sextant"
            ]
        );
    }

    #[test]
    fn reads_static_resource_text() {
        assert!(resource_text("sextant://browser/capabilities")
            .unwrap()
            .contains("sextant-browser"));
        assert!(resource_text("sextant://browser/capabilities")
            .unwrap()
            .contains("Direct and Incognito block AI DOM/frame observation"));
        assert!(resource_text("sextant://browser/operator-workflow")
            .unwrap()
            .contains("browser_authorized_intent_run"));
    }

    #[test]
    fn capabilities_advertise_browser_modes_and_loop_boundaries() {
        let capabilities = browser_capabilities();
        let modes = capabilities["browser"]["modes"].as_array().unwrap();

        assert_eq!(modes.len(), 5);
        assert_eq!(modes[0]["id"], "agent");
        assert_eq!(modes[1]["id"], "assisted");
        assert_eq!(modes[2]["id"], "observe");
        assert_eq!(modes[3]["id"], "direct");
        assert_eq!(modes[4]["id"], "incognito");
        assert_eq!(
            modes[3]["capabilities"]["aiObserveFrame"],
            Value::Bool(false)
        );
        assert_eq!(
            modes[3]["capabilities"]["directRenderRequired"],
            Value::Bool(true)
        );
        assert_eq!(
            capabilities["browser"]["runtimeLoops"]["renderBridgeLoop"]["status"],
            "temporary"
        );
        assert_eq!(
            capabilities["browser"]["runtimeLoops"]["viewportInputLane"]["status"],
            "implemented"
        );
        assert_eq!(
            capabilities["browser"]["runtimeLoops"]["directPresentationLoop"]["status"],
            "experimental proof"
        );
        assert_eq!(
            capabilities["browser"]["renderingBoundary"]["directCompositor"],
            "experimental proof via sextant-servo-direct"
        );
        assert_eq!(
            capabilities["browser"]["operatorBridge"]["directPresentSmoke"],
            Value::Bool(true)
        );
        assert_eq!(
            capabilities["browser"]["operatorBridge"]["renderPathBaseline"],
            Value::Bool(true)
        );
        assert_eq!(
            capabilities["browser"]["runtimeLoops"]["aiObservationLoop"]["modeBoundary"],
            "disabled in Direct and Incognito"
        );
        assert_eq!(
            capabilities["browser"]["visibleShellSmoke"]["modeSelection"][4],
            "incognito"
        );
        assert_eq!(
            capabilities["browser"]["visibleShellSmoke"]["controlWorkModes"][1],
            "assisted"
        );
    }

    #[test]
    fn normalizes_optional_browser_mode_arguments() {
        assert_eq!(
            optional_browser_mode(&json!({"mode": "DIRECT"})).unwrap(),
            Some("direct".to_string())
        );
        assert_eq!(
            optional_browser_mode(&json!({"mode": "incog"})).unwrap(),
            Some("incognito".to_string())
        );
        assert_eq!(optional_browser_mode(&json!({})).unwrap(), None);
        assert!(optional_browser_mode(&json!({"mode": "mystery"})).is_err());
    }

    #[test]
    fn parses_window_smoke_cli_arguments() {
        let args = vec![
            "sextant-mcp".to_string(),
            "--window-smoke".to_string(),
            "https://example.com".to_string(),
            "--mode".to_string(),
            "incog".to_string(),
            "--timeout-seconds".to_string(),
            "30".to_string(),
            "--render-path".to_string(),
            "bridge".to_string(),
        ];
        let arguments = browser_window_smoke_cli_arguments(&args).unwrap().unwrap();

        assert_eq!(arguments["target"], "https://example.com");
        assert_eq!(arguments["mode"], "incognito");
        assert_eq!(arguments["render_path"], "bridge");
        assert_eq!(arguments["timeout_seconds"], 30);
    }

    #[test]
    fn parses_window_smoke_cli_workflow_flags() {
        let args = vec![
            "sextant-mcp".to_string(),
            "--window-smoke".to_string(),
            "--start-showcase".to_string(),
            "--browser-mode".to_string(),
            "direct".to_string(),
            "--window-smoke-timeout".to_string(),
            "12".to_string(),
            "--user-distill".to_string(),
            "--pre-size-navigation".to_string(),
        ];
        let arguments = browser_window_smoke_cli_arguments(&args).unwrap().unwrap();

        assert_eq!(arguments["showcase"], Value::Bool(true));
        assert_eq!(arguments["user_distill"], Value::Bool(true));
        assert_eq!(arguments["pre_size_navigation"], Value::Bool(true));
        assert_eq!(arguments["mode"], "direct");
        assert_eq!(arguments["timeout_seconds"], 12);
        assert!(arguments.get("target").is_none());
    }

    #[test]
    fn ignores_window_smoke_cli_arguments_when_absent() {
        let args = vec![
            "sextant-mcp".to_string(),
            "--perf-probe".to_string(),
            "https://example.com".to_string(),
        ];

        assert!(browser_window_smoke_cli_arguments(&args).unwrap().is_none());
    }

    #[test]
    fn parses_direct_present_smoke_cli_arguments() {
        let args = vec![
            "sextant-mcp".to_string(),
            "--direct-present-smoke".to_string(),
            "https://example.com".to_string(),
            "--timeout-seconds".to_string(),
            "30".to_string(),
        ];
        let arguments = browser_direct_present_smoke_cli_arguments(&args)
            .unwrap()
            .unwrap();

        assert_eq!(arguments["target"], "https://example.com");
        assert_eq!(arguments["timeout_seconds"], 30);
    }

    #[test]
    fn parses_render_path_baseline_cli_arguments() {
        let args = vec![
            "sextant-mcp".to_string(),
            "--render-path-baseline".to_string(),
            "https://example.com".to_string(),
            "--timeout-seconds".to_string(),
            "30".to_string(),
        ];
        let arguments = browser_render_path_baseline_cli_arguments(&args)
            .unwrap()
            .unwrap();

        assert_eq!(arguments["target"], "https://example.com");
        assert_eq!(arguments["timeout_seconds"], 30);
    }

    #[test]
    fn detects_native_intent_window_smoke_targets() {
        assert!(is_native_intent_target(
            "intent: open https://example.com and distill"
        ));
        assert!(is_native_intent_target(
            "open https://example.com and remember"
        ));
        assert!(is_native_intent_target("research Rust ownership docs"));
        assert!(!is_native_intent_target("https://example.com"));
        assert!(!is_native_intent_target("plain web search"));
    }

    #[test]
    fn window_smoke_blocks_control_work_in_non_control_modes() {
        let showcase = json!({"mode": "direct", "showcase": true});
        let intent = json!({
            "mode": "incognito",
            "target": "intent: open https://example.com and distill"
        });
        let normal_page = json!({"mode": "direct", "target": "https://example.com"});
        let assisted_showcase = json!({"mode": "assisted", "showcase": true});
        let observe_distill = json!({"mode": "observe", "user_distill": true});
        let direct_distill = json!({"mode": "direct", "user_distill": true});

        assert!(window_smoke_mode_boundary_error(&showcase, Some("direct"))
            .unwrap()
            .contains("showcase seeding"));
        assert!(window_smoke_mode_boundary_error(&intent, Some("incognito"))
            .unwrap()
            .contains("native intent target"));
        assert!(window_smoke_mode_boundary_error(&normal_page, Some("direct")).is_none());
        assert!(window_smoke_mode_boundary_error(&assisted_showcase, Some("assisted")).is_none());
        assert!(window_smoke_mode_boundary_error(&observe_distill, Some("observe")).is_none());
        assert!(
            window_smoke_mode_boundary_error(&direct_distill, Some("direct"))
                .unwrap()
                .contains("user-visible distillation")
        );
    }

    #[test]
    fn browser_window_smoke_rejects_disallowed_mode_work_before_launch() {
        let result = run_browser_window_smoke_tool(json!({
            "mode": "direct",
            "showcase": true,
            "timeout_seconds": 30
        }))
        .unwrap();

        assert_eq!(result["isError"], Value::Bool(true));
        assert!(tool_text(&result).contains("mode 'direct' blocks showcase seeding"));
    }

    #[test]
    fn strips_ansi_from_operator_output() {
        let raw = "\u{1b}[2m2026\u{1b}[0m WARN\n[operator-smoke] passed";
        assert_eq!(strip_ansi(raw), "2026 WARN\n[operator-smoke] passed");
    }

    #[test]
    fn responds_to_empty_capability_lists_and_ping() {
        let ping = handle_message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "ping"
        }))
        .unwrap();
        assert_eq!(ping["result"], json!({}));

        let prompts = handle_message(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "prompts/list"
        }))
        .unwrap();
        assert_eq!(prompts["result"]["prompts"], json!([]));
    }

    #[test]
    fn initialize_advertises_browser_modes() {
        let response = handle_message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize"
        }))
        .unwrap();

        assert_eq!(response["result"]["browserModes"][0]["id"], "agent");
        assert_eq!(response["result"]["browserModes"][4]["id"], "incognito");
    }

    #[test]
    fn launch_errors_include_structured_command() {
        let result = tool_command_launch_error(
            &[
                "--operator-timeout".to_string(),
                "90".to_string(),
                "--operator-smoke".to_string(),
            ],
            "missing binary",
        );

        assert_eq!(result["isError"], Value::Bool(true));
        assert_eq!(
            result["structuredContent"]["command"]["binary"],
            Value::String("sextant-browser".to_string())
        );
    }

    #[test]
    fn extracts_window_smoke_lines() {
        let output =
            "noise\n[window-start] running launch showcase\n[window-user] Distilling active page...\n[window-smoke] visible shell draw passed\n[sextant-browser] failed: window start failed\n[operator-run] ignored";
        assert_eq!(
            window_report_lines(output),
            vec![
                "[window-start] running launch showcase",
                "[window-user] Distilling active page...",
                "[window-smoke] visible shell draw passed",
                "[sextant-browser] failed: window start failed"
            ]
        );
    }

    #[test]
    fn parses_window_smoke_summary() {
        let report = vec![
            "[window-smoke] browser mode INCOG (Incognito Mode: human-only, no AI observation or content persistence.)".to_string(),
            "[window-smoke] render path BRIDGE (temporary frame-capture render bridge feeding Softbuffer)".to_string(),
            "[window-smoke] render gap direct compositor pending".to_string(),
            "[window-smoke] pre-size navigation true".to_string(),
            "[window-smoke] ephemeral browser data dir C:\\Temp\\sextant-browser-incognito-123".to_string(),
            "[window-smoke] visible shell draw passed in 38ms".to_string(),
            "[window-smoke] startup/navigation work 414ms".to_string(),
            "[window-smoke] draw cost total 30ms | frame blit 9ms | present 0ms".to_string(),
            "[window-smoke] perf nav 414ms | distill pending | wake pending | resize 390ms | frame 438ms".to_string(),
            "[window-smoke] slowest perf frame 438ms | capture render bridge".to_string(),
            "[window-smoke] slow perf frame 438ms capture render bridge | nav-phase 414ms open servo navigation | resize 390ms viewport render bridge async".to_string(),
            "[window-smoke] latest Servo frame 776x292 (226592 pixels)".to_string(),
            "[window-smoke] first Servo frame in 950ms".to_string(),
        ];

        let summary = window_smoke_summary(&report);

        assert_eq!(summary["mode"], "INCOG");
        assert_eq!(summary["renderPath"], "BRIDGE");
        assert_eq!(
            summary["renderPathStatus"],
            "temporary frame-capture render bridge feeding Softbuffer"
        );
        assert_eq!(summary["renderGap"], "direct compositor pending");
        assert_eq!(summary["preSizeNavigation"], Value::Bool(true));
        assert_eq!(summary["storage"], "ephemeral");
        assert_eq!(
            summary["ephemeralDataDir"],
            "C:\\Temp\\sextant-browser-incognito-123"
        );
        assert_eq!(summary["firstDrawMs"], Value::from(38));
        assert_eq!(summary["startupWorkMs"], Value::from(414));
        assert_eq!(summary["drawTotalMs"], Value::from(30));
        assert_eq!(summary["frameBlitMs"], Value::from(9));
        assert_eq!(summary["presentMs"], Value::from(0));
        assert_eq!(summary["navigationMs"], Value::from(414));
        assert_eq!(summary["distillMs"], Value::Null);
        assert_eq!(summary["slowestPerfEvent"]["phase"], "frame");
        assert_eq!(summary["slowestPerfEvent"]["durationMs"], Value::from(438));
        assert_eq!(
            summary["slowestPerfEvent"]["label"],
            "capture render bridge"
        );
        assert_eq!(summary["slowPerfEvents"][1]["phase"], "nav-phase");
        assert_eq!(
            summary["slowPerfEvents"][1]["label"],
            "open servo navigation"
        );
        assert_eq!(summary["latestFrameWidth"], Value::from(776));
        assert_eq!(summary["latestFrameHeight"], Value::from(292));
        assert_eq!(summary["latestFramePixels"], Value::from(226592));
        assert_eq!(summary["firstFrameMs"], Value::from(950));
    }

    #[test]
    fn parses_direct_present_summary() {
        let output = "[servo-direct] opening https://example.com/ with Servo WindowRenderingContext\nnoise\n[servo-direct] first direct present in 127ms";
        let report = direct_present_report_lines(output);
        let summary = direct_present_summary(&report);

        assert_eq!(report.len(), 2);
        assert_eq!(summary["target"], "https://example.com/");
        assert_eq!(summary["firstPresentMs"], Value::from(127));
        assert_eq!(summary["frameReadback"], Value::Bool(false));
        assert_eq!(summary["productionIntegrated"], Value::Bool(false));
    }

    #[test]
    fn summarizes_render_path_baseline() {
        let direct = json!({
            "firstPresentMs": 149,
            "frameReadback": false,
            "productionIntegrated": false,
        });
        let bridge = json!({
            "firstFrameMs": 1000,
            "frameMs": 40,
            "resizeMs": 408,
            "renderPath": "BRIDGE",
            "renderGap": "direct compositor pending",
        });
        let pre_sized_bridge = json!({
            "firstFrameMs": 1200,
            "navigationMs": 980,
            "frameMs": 35,
            "resizeMs": 0,
        });

        let summary = render_path_baseline_summary(&direct, &bridge, &pre_sized_bridge);

        assert_eq!(summary["directFirstPresentMs"], 149);
        assert_eq!(summary["bridgeFirstFrameMs"], 1000);
        assert_eq!(summary["bridgeFrameMs"], 40);
        assert_eq!(summary["bridgeResizeMs"], 408);
        assert_eq!(summary["firstFrameGapMs"], 851);
        assert_eq!(summary["preSizedBridgeFirstFrameMs"], 1200);
        assert_eq!(summary["preSizedBridgeResizeMs"], 0);
        assert_eq!(summary["preSizedVsDefaultGapMs"], 200);
        assert_eq!(summary["bridgeRenderPath"], "BRIDGE");
        assert_eq!(summary["directFrameReadback"], Value::Bool(false));
    }

    #[test]
    fn extracts_intent_run_lines_with_operator_reports() {
        let output =
            "noise\n[intent-run] native intent run passed\n[consent-run] native authorized intent run passed\n[guard-probe] firewall: AUDIT https://example.com/ | No specific rule matched\n[operator-run] scripted run passed\n[perception-probe] perception summary: PERCEPTION simple document\n[perf-probe] perf after navigation: nav 409ms | distill pending\n[perf-baseline] browser performance baseline complete\n[real-browsing-smoke] real browsing smoke suite passed\n[showcase-run] Sextant launch showcase run passed";
        assert_eq!(
            operator_report_lines(output),
            vec![
                "[intent-run] native intent run passed",
                "[consent-run] native authorized intent run passed",
                "[guard-probe] firewall: AUDIT https://example.com/ | No specific rule matched",
                "[operator-run] scripted run passed",
                "[perception-probe] perception summary: PERCEPTION simple document",
                "[perf-probe] perf after navigation: nav 409ms | distill pending",
                "[perf-baseline] browser performance baseline complete",
                "[real-browsing-smoke] real browsing smoke suite passed",
                "[showcase-run] Sextant launch showcase run passed"
            ]
        );
    }

    #[test]
    fn parses_perception_report() {
        let report = perception_report(&[
            "[perception-probe] perception summary: PERCEPTION interactive form page with 3 semantic node(s), 1 link(s), 1 input(s), 1 heading(s)".to_string(),
            "[perception-probe] perception counts: headings=1 links=1 inputs=1 images=0 text=0 buttons=1".to_string(),
            "[perception-probe] perception node 1: INPUT | input[name=q] | Search".to_string(),
            "[perception-probe] perception source: servo-live-dom".to_string(),
        ]);

        assert_eq!(
            report["summary"],
            "PERCEPTION interactive form page with 3 semantic node(s), 1 link(s), 1 input(s), 1 heading(s)"
        );
        assert_eq!(report["counts"]["inputs"], 1);
        assert_eq!(report["counts"]["buttons"], 1);
        assert_eq!(report["nodes"][0]["type"], "INPUT");
        assert_eq!(report["nodes"][0]["selector"], "input[name=q]");
        assert_eq!(report["nodes"][0]["text"], "Search");
        assert_eq!(report["source"], "servo-live-dom");
    }

    #[test]
    fn parses_guard_report() {
        let report = guard_report(&[
            "[guard-probe] guard persona: browser-persona".to_string(),
            "[guard-probe] guard policy: overlay C:\\Temp\\guard-policy.json".to_string(),
            "[guard-probe] guard rules: persona=1 global_blacklist=2".to_string(),
            "[guard-probe] airgap: Online | ONLINE network allowed".to_string(),
            "[guard-probe] firewall: AUDIT https://example.com/ | No specific rule matched"
                .to_string(),
            "[guard-probe] privacy: STANDARD redaction active".to_string(),
            "[guard-probe] privacy sample: Contact [REDACTED NAME] at [REDACTED EMAIL]".to_string(),
        ]);

        assert_eq!(report["persona"], "browser-persona");
        assert_eq!(report["policy"], "overlay C:\\Temp\\guard-policy.json");
        assert_eq!(report["rules"]["persona"], 1);
        assert_eq!(report["rules"]["globalBlacklist"], 2);
        assert_eq!(report["airgap"], "Online | ONLINE network allowed");
        assert_eq!(
            report["firewall"],
            "AUDIT https://example.com/ | No specific rule matched"
        );
        assert_eq!(report["action"], "AUDIT");
        assert_eq!(report["blocked"], false);
        assert_eq!(report["privacy"], "STANDARD redaction active");
        assert_eq!(report["lines"].as_array().unwrap().len(), 7);
    }

    #[test]
    fn parses_guard_block_status() {
        let report = guard_report(&[
            "[guard-probe] firewall: BLOCK https://malicious-site.net/path | Global blacklist match"
                .to_string(),
            "[guard-probe] Local guard blocked navigate to https://malicious-site.net/path: BLOCK | network allowed | Global blacklist match".to_string(),
            "[guard-probe] browser guard probe blocked navigation".to_string(),
        ]);

        assert_eq!(report["action"], "BLOCK");
        assert_eq!(report["blocked"], true);
        assert!(report["block"]
            .as_str()
            .unwrap()
            .contains("Local guard blocked navigate"));
        assert_eq!(report["lines"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn summarizes_firewall_policy() {
        let policy = FirewallPolicy {
            persona_rules: std::collections::HashMap::from([
                (
                    "browser-persona".to_string(),
                    vec![sextant_firewall::FirewallRule {
                        domain_pattern: "example.com".to_string(),
                        action: sextant_firewall::FirewallAction::Block,
                        reason: "qa".to_string(),
                    }],
                ),
                ("Work".to_string(), Vec::new()),
            ]),
            global_blacklist: vec!["ads.example.net".to_string()],
        };
        let summary = firewall_policy_summary(&policy);

        assert_eq!(summary["personaCount"], 2);
        assert_eq!(summary["personaRuleCount"], 1);
        assert_eq!(summary["globalBlacklistCount"], 1);
    }

    #[test]
    fn parses_perf_timing_samples() {
        let samples = perf_timing_samples(&[
            "[perf-probe] perf after navigation: nav 409ms | distill pending | wake pending | resize pending | frame pending".to_string(),
            "https://example.com: [perf-probe] perf after warm frame capture: nav 1.2s | distill 0ms | wake 8ms | resize 0ms | frame 25ms".to_string(),
            "[perf-baseline] https://www.rust-lang.org/: perf after navigation: nav 1.0s | distill pending | wake pending | resize pending | frame pending".to_string(),
        ]);

        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0]["label"], "after navigation");
        assert_eq!(samples[0]["navigationMs"], 409);
        assert!(samples[0]["distillMs"].is_null());
        assert_eq!(samples[1]["label"], "after warm frame capture");
        assert_eq!(samples[1]["target"], "https://example.com");
        assert_eq!(samples[1]["navigationMs"], 1200);
        assert_eq!(samples[1]["distillMs"], 0);
        assert_eq!(samples[1]["wakeMs"], 8);
        assert_eq!(samples[1]["resizeMs"], 0);
        assert_eq!(samples[1]["frameMs"], 25);
        assert_eq!(samples[2]["target"], "https://www.rust-lang.org/");
        assert_eq!(samples[2]["navigationMs"], 1000);
    }

    #[test]
    fn summarizes_perf_timing_samples() {
        let samples = perf_timing_samples(&[
            "[perf-probe] perf after navigation: nav 409ms | distill pending | wake pending | resize pending | frame pending".to_string(),
            "[perf-probe] perf after distillation: nav 409ms | distill 303ms | wake 3ms | resize pending | frame pending".to_string(),
            "[perf-probe] perf after warm frame capture: nav 409ms | distill 0ms | wake 7ms | resize 0ms | frame 15ms".to_string(),
        ]);
        let summary = perf_timing_summary(&samples);

        assert_eq!(summary["sampleCount"], 3);
        assert_eq!(summary["maxNavigationMs"], 409);
        assert_eq!(summary["maxDistillMs"], 303);
        assert_eq!(summary["maxWakeMs"], 7);
        assert_eq!(summary["maxResizeMs"], 0);
        assert_eq!(summary["maxFrameMs"], 15);
        assert_eq!(summary["slowestPhase"]["phase"], "navigation");
        assert_eq!(summary["slowestPhase"]["durationMs"], 409);
        assert!(summary["slowestPhase"]["target"].is_null());
    }

    #[test]
    fn summarizes_baseline_targets() {
        let samples = perf_timing_samples(&[
            "https://example.com: [perf-probe] perf after warm frame capture: nav 400ms | distill 0ms | wake 7ms | resize 0ms | frame 15ms".to_string(),
            "https://developer.mozilla.org/en-US/docs/Web/HTML: [perf-probe] perf after distillation: nav 1.2s | distill 755ms | wake 9ms | resize pending | frame pending".to_string(),
        ]);
        let summary = perf_timing_summary(&samples);

        assert_eq!(summary["sampleCount"], 2);
        assert_eq!(summary["slowestPhase"]["phase"], "navigation");
        assert_eq!(summary["slowestPhase"]["durationMs"], 1200);
        assert_eq!(
            summary["slowestPhase"]["target"],
            "https://developer.mozilla.org/en-US/docs/Web/HTML"
        );
    }

    #[test]
    fn extracts_browser_failure_lines_from_stderr_source() {
        let source = command_report_source(
            "",
            "cargo noise\n[real-browsing-smoke] failed: stale navigation",
        );
        assert_eq!(
            operator_report_lines(&source),
            vec!["[real-browsing-smoke] failed: stale navigation"]
        );
    }

    #[test]
    fn formats_launch_preflight_summary() {
        let text = preflight_result_text(
            &json!({
                "success": false,
                "durationMs": 1534,
                "checks": [
                    {"name": "operator smoke", "success": true, "durationMs": 42},
                    {"name": "visible showcase smoke", "success": false, "durationMs": 1492, "report": ["[window-start] running showcase", "[sextant-browser] failed: draw failed"]}
                ]
            }),
            "launch preflight",
        );
        assert!(text.contains("Sextant launch preflight failed"));
        assert!(text.contains("Duration: 1.5s"));
        assert!(text.contains("Slowest: visible showcase smoke (1.5s)"));
        assert!(text.contains("PASS: operator smoke (42ms)"));
        assert!(text.contains("FAIL: visible showcase smoke (1.5s)"));
        assert!(text.contains("[sextant-browser] failed: draw failed"));
    }

    #[test]
    fn formats_hardening_preflight_summary() {
        let text = preflight_result_text(
            &json!({
                "success": true,
                "durationMs": 24_500,
                "checks": [
                    {"name": "real browsing smoke", "success": true, "durationMs": 18_300},
                    {"name": "visible shell interaction smoke", "success": true, "durationMs": 6_200}
                ]
            }),
            "hardening preflight",
        );
        assert!(text.contains("Sextant hardening preflight passed"));
        assert!(text.contains("Duration: 24.5s"));
        assert!(text.contains("Slowest: real browsing smoke (18.3s)"));
        assert!(text.contains("PASS: real browsing smoke (18.3s)"));
        assert!(text.contains("PASS: visible shell interaction smoke (6.2s)"));
    }

    #[test]
    fn formats_short_duration_ms() {
        assert_eq!(format_duration_ms(999), "999ms");
        assert_eq!(format_duration_ms(1_250), "1.2s");
    }

    #[test]
    fn selects_slowest_preflight_check() {
        let checks = vec![
            json!({"name": "fast", "durationMs": 25}),
            json!({"name": "slow", "durationMs": 250}),
            json!({"name": "unknown"}),
        ];
        assert_eq!(
            slowest_preflight_check(&checks),
            Some(("slow".to_string(), 250))
        );
    }

    #[test]
    fn extracts_tool_text_for_cli_output() {
        let text = tool_text(&json!({
            "content": [{"type": "text", "text": "short summary"}],
            "structuredContent": {"success": true}
        }));
        assert_eq!(text, "short summary");
    }
}
