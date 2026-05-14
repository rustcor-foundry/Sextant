use chrono::Utc;
use serde_json::{json, Value};
use sextant_log::{CaptainsLog, LogEntry, LogStatus};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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
                "tools": tools(),
                "resources": resources(),
            }))
            .map_err(|error| error.to_string())?
        );
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
            let requested = message
                .get("params")
                .and_then(|params| params.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION);
            response(
                id,
                json!({
                    "protocolVersion": requested,
                    "capabilities": server_capabilities(),
                    "serverInfo": server_info(),
                    "instructions": "Sextant exposes local browser automation through the native sextant-browser operator bridge. Start with browser_capabilities, then use browser_operator_probe or browser_operator_run for bounded real browsing checks.",
                }),
            )
        }),
        "notifications/initialized" => None,
        "tools/list" => id.map(|id| response(id, json!({ "tools": tools() }))),
        "tools/call" => id.map(|id| handle_tool_call(id, message.get("params").cloned())),
        "resources/list" => id.map(|id| response(id, json!({ "resources": resources() }))),
        "resources/read" => id.map(|id| handle_resource_read(id, message.get("params").cloned())),
        "resources/templates/list" => id.map(|id| response(id, json!({ "resourceTemplates": [] }))),
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
            "name": "browser_operator_probe",
            "title": "Browser Operator Probe",
            "description": "Navigate the native browser to a URL or search phrase, distill the page, and report operator results.",
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
                "Use `browser_operator_probe` for page-level navigation and distillation checks.",
                "Use `browser_operator_run` for ordered selector scripts:",
                "",
                "```json",
                r#"{"target":"https://www.google.com","steps":[{"action":"fill","selector":"textarea[name=q]","value":"Sextant native browser test"},{"action":"submit","selector":"form"},{"action":"expect","text":"Sextant native browser test"}]}"#,
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
                "- Operator tools are bounded by `timeout_seconds` and return captured stdout/stderr.",
                "- Tool calls are recorded with the `sextant-mcp-stdio` signature when Captain's Log is writable.",
                "- Long-lived shared browser sessions are intentionally deferred until the browser runtime is extracted into reusable session code.",
            ]
            .join("\n"),
        ),
        _ => None,
    }
}

fn browser_capabilities() -> Value {
    json!({
        "browser": {
            "canonicalBinary": "sextant-browser",
            "compatibilityAlias": "sextant-hull-lite",
            "defaultEngine": "servo-backed native browser",
            "fallbackEngine": "reader/fetch/distill build via --no-default-features",
            "operatorBridge": {
                "smoke": true,
                "probe": true,
                "run": ["fill", "click", "submit", "expect"],
                "timeoutSecondsDefault": 120,
            }
        },
        "mcp": {
            "transport": "stdio",
            "protocolVersion": PROTOCOL_VERSION,
            "tools": [
                "browser_capabilities",
                "browser_operator_smoke",
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

fn required_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn operator_timeout(arguments: &Value) -> u64 {
    arguments
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .filter(|seconds| *seconds > 0)
        .unwrap_or(120)
        .min(600)
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

    let output = run_browser_command(&full_args)?;
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

    let report = operator_report_lines(&stdout);
    let structured = json!({
        "success": success,
        "exitCode": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "operatorReport": report,
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

fn operator_report_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("[operator-"))
        .map(str::to_string)
        .collect()
}

fn operator_result_text(success: bool, exit_code: Option<i32>, report: Option<&Value>) -> String {
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
        return format!("sextant-browser exited with code {exit_code:?}; success={success}");
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

enum BrowserRunner {
    Binary(PathBuf),
    Cargo { workspace: PathBuf },
}

impl BrowserRunner {
    fn locate() -> Result<Self, String> {
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
        assert!(names.contains(&"browser_operator_run"));
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
    }

    #[test]
    fn strips_ansi_from_operator_output() {
        let raw = "\u{1b}[2m2026\u{1b}[0m WARN\n[operator-smoke] passed";
        assert_eq!(strip_ansi(raw), "2026 WARN\n[operator-smoke] passed");
    }
}
