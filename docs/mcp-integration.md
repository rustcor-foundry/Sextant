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
- `browser_window_smoke` launches the visible `sextant-browser` shell, optionally navigates to a target, draws once, and exits.
- `browser_operator_probe` opens a URL or search phrase, distills it, and reports the native operator output.
- `browser_operator_run` executes ordered `fill`, `click`, `submit`, and `expect` selector steps through the native browser operator bridge.
- `captains_log_recent` reads recent Captain's Log entries for the browser persona.

Operator tools call the canonical `sextant-browser` binary when it is available beside `sextant-mcp`. During development, they fall back to:

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

## Next Integration Step

This MCP layer currently delegates browser work to the native operator binary. The next deeper integration should extract the browser session/operator runtime from `sextant-hull/src/browser.rs` into reusable Rust code so the visible browser and MCP server can share a long-lived session path.
