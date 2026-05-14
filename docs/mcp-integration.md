# MCP Integration

Sextant now has a local stdio MCP server in the Rust workspace:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-mcp
```

This server advertises the native browser capabilities to MCP clients such as Codex and Claude without opening a network listener. It speaks newline-delimited JSON-RPC over stdin/stdout and keeps logs on stderr only.

## Current Tool Scope

The first layer is intentionally truthful and narrow:

- `browser_capabilities` reports the current native browser, operator bridge, tools, and resources.
- `browser_operator_smoke` runs the deterministic `sextant-browser --operator-smoke` check.
- `browser_launch_preflight` runs operator smoke, Intent Bar, showcase, and visible showcase smoke checks in sequence.
- `browser_showcase_run` runs the launch-demo proof across Intent Bar, Servo navigation, Wake, Captain's Log, form interaction, and frame capture.
- `browser_window_smoke` launches the visible `sextant-browser` shell, optionally navigates to a target or runs the showcase first, draws once, and exits.
- `browser_intent_run` drives the native Intent Bar loop, for example `intent: open https://example.com and distill`, with optional expected text validation.
- `browser_operator_probe` opens a URL or search phrase, distills it, and reports the native operator output.
- `browser_operator_run` executes ordered `fill`, `click`, `submit`, and `expect` selector steps through the native browser operator bridge.
- `captains_log_recent` reads recent Captain's Log entries for the browser persona.

During development, operator tools prefer the local Cargo workspace so MCP calls exercise the current source instead of a stale sibling executable. Packaged installs can set `SEXTANT_MCP_USE_INSTALLED_BROWSER=1` to prefer the canonical `sextant-browser` binary beside `sextant-mcp`.

The development fallback command is:

```bash
cargo run -p sextant-hull --bin sextant-browser -- ...
```

## Resources

The server also exposes static capability resources:

- `sextant://browser/capabilities`
- `sextant://browser/operator-workflow`
- `sextant://browser/mcp-tools`

Use these resources as the "what can this browser do right now?" context surface for agents.

## Development Checks

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-mcp -- --self-test
cargo run -p sextant-mcp -- --list-tools
cargo check -p sextant-mcp
cargo test -p sextant-mcp
```

When testing against a real MCP client, initialize first, then call `tools/list`. For quick manual protocol checks, send one JSON-RPC message per line.

Use `browser_window_smoke` when the question is "does the actual user-facing window launch and draw?" Use `browser_operator_*` tools when the question is "can the native browser automation path navigate, interact, distill, and record state?"

Use `browser_intent_run` when the question is "can the browser perform the user-facing Intent Bar workflow?" A typical call is:

```json
{"intent":"intent: open https://example.com and distill","expect":"Example Domain","timeout_seconds":45}
```

Use `browser_showcase_run` for a bounded pre-demo proof that the launch path still works end to end.

Use `browser_window_smoke` with `{"showcase": true}` when the question is "does the actual visible showcase window draw?"

Use the CLI preflight before live demo work:

```bash
cargo run -p sextant-mcp -- --launch-preflight --timeout-seconds 60
```

Add `--json` when a script needs the full structured check report.

## Next Integration Step

This MCP layer currently delegates browser work to the native operator binary. The next deeper integration should extract the browser session/operator runtime from `sextant-hull/src/browser.rs` into reusable Rust code so the visible browser and MCP server can share a long-lived session path.
