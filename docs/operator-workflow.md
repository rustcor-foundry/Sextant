# Operator Workflow

This is the normal development flow for working in Sextant without losing the thread of the product.

## Primary Rule

Follow [next-work.md](next-work.md) as the active execution guide unless the team explicitly decides to re-prioritize.

## Read Order At Session Start

1. [current-state.md](current-state.md)
2. [next-work.md](next-work.md)
3. [architecture.md](architecture.md)
4. [development-guardrails.md](development-guardrails.md)
5. [completed.md](completed.md)

## Normal Development Loop

1. Confirm which lane in [next-work.md](next-work.md) you are advancing.
2. Inspect the relevant Rust crates before changing anything.
3. Prefer the native Rust hull and supporting Rust crates over the TypeScript simulator for real product work.
4. Make the smallest coherent change that advances the actual product path.
5. Verify with the most relevant command:
   - `cargo check -p sextant-hull`
   - `cargo test -p sextant-pilot --lib`
   - `cargo test -p sextant-engine --lib`
   - `cargo run -p sextant-hull --bin sextant-browser -- --operator-smoke`
   - `cargo run -p sextant-hull --bin sextant-browser -- --showcase-run`
   - `cargo run -p sextant-mcp -- --launch-preflight --timeout-seconds 60`
   - `cargo run -p sextant-hull --bin sextant-browser -- --operator-probe https://example.com`
   - `cargo run -p sextant-hull --bin sextant-browser -- --guard-probe https://example.com --operator-timeout 120`
   - `cargo run -p sextant-hull --bin sextant-browser -- --guard-probe https://example.com --guard-policy path/to/guard-policy.json --operator-timeout 120`
   - `cargo run -p sextant-mcp -- --guard-policy-read --json`
   - `cargo run -p sextant-mcp -- --guard-policy-write path/to/guard-policy.json --json`
   - `cargo run -p sextant-hull --bin sextant-browser -- --perception-probe https://example.com --operator-timeout 120`
   - `cargo run -p sextant-hull --bin sextant-browser -- --perf-probe https://developer.mozilla.org/en-US/docs/Web/HTML --operator-timeout 180`
   - `cargo run -p sextant-hull --bin sextant-browser -- --operator-run https://example.com --expect "Example Domain"`
   - `cargo run -p sextant-hull --bin sextant-browser -- --intent-run "intent: open https://example.com and distill" --expect "Example Domain"`
   - `cargo run -p sextant-hull --bin sextant-browser -- --consent-run "intent: buy https://example.com and checkout" --expect "Example Domain"`
   - `cargo run -p sextant-hull --bin sextant-browser -- --window-smoke https://example.com --window-smoke-timeout 15`
   - `cargo run -p sextant-hull --bin sextant-browser`
6. Update docs if the current state, workflow, or recovery story changed.
7. Commit in coherent units and push to the canonical Gitea remote.
8. Watch CI on Gitea and treat failures as part of the task, not a separate later chore.

## Preferred Verification Commands

### Fast Native Sanity Check

```bash
cd rust
cargo check -p sextant-hull
```

### Pilot Regression Coverage

```bash
cd rust
cargo test -p sextant-pilot --lib
```

### Engine Regression Coverage

```bash
cd rust
cargo test -p sextant-engine --lib
```

### Real Hull Launch

```bash
cd rust
cargo run -p sextant-hull --bin sextant-browser
```

### Xilem/Vello Diagnostic Ladder

The parked Xilem hull can now be launched in bounded smoke modes while investigating the 2D rendering path:

```bash
cd rust
cargo run -p sextant-hull --bin sextant-hull -- --xilem-modes
cargo run -p sextant-hull --bin sextant-hull -- --xilem-smoke minimal --xilem-timeout 5
cargo run -p sextant-hull --bin sextant-hull -- --xilem-smoke safe --xilem-timeout 5
cargo run -p sextant-hull --bin sextant-hull -- --xilem-smoke interactive-smoke --xilem-timeout 5
cargo run -p sextant-hull --bin sextant-hull -- --xilem-smoke full --xilem-timeout 5
```

These modes write breadcrumbs to `xilem-diagnostics.log` under the Sextant app data directory. Passing first-paint smoke does not prove interactive stability; it narrows the next investigation to click, text, focus, accessibility, or longer widget lifecycle paths.

### Native Operator Bridge

```bash
cd rust
cargo run -p sextant-hull --bin sextant-browser -- --operator-smoke
cargo run -p sextant-hull --bin sextant-browser -- --showcase-run
cargo run -p sextant-mcp -- --launch-preflight --timeout-seconds 60
cargo run -p sextant-mcp -- --hardening-preflight --timeout-seconds 240 --visible-timeout-seconds 60
cargo run -p sextant-mcp -- --real-browsing-smoke --timeout-seconds 240
cargo run -p sextant-hull --bin sextant-browser -- --operator-probe https://example.com
cargo run -p sextant-hull --bin sextant-browser -- --guard-probe https://example.com --operator-timeout 120
cargo run -p sextant-hull --bin sextant-browser -- --guard-probe https://example.com --guard-policy path/to/guard-policy.json --operator-timeout 120
cargo run -p sextant-hull --bin sextant-browser -- --perception-probe https://example.com --operator-timeout 120
cargo run -p sextant-hull --bin sextant-browser -- --perf-probe https://developer.mozilla.org/en-US/docs/Web/HTML --operator-timeout 180
cargo run -p sextant-hull --bin sextant-browser -- --perf-baseline --operator-timeout 240
cargo run -p sextant-mcp -- --guard-probe https://example.com --timeout-seconds 120
cargo run -p sextant-mcp -- --guard-probe https://example.com --guard-policy path/to/guard-policy.json --timeout-seconds 120
cargo run -p sextant-mcp -- --guard-policy-read --json
cargo run -p sextant-mcp -- --guard-policy-write path/to/guard-policy.json --json
cargo run -p sextant-mcp -- --perception-probe https://example.com --timeout-seconds 120
cargo run -p sextant-mcp -- --perf-probe https://example.com --timeout-seconds 120
cargo run -p sextant-hull --bin sextant-browser -- --operator-run https://example.com --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --operator-timeout 45 --operator-run https://example.com --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --operator-timeout 45 --intent-run "intent: open https://example.com and distill" --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --operator-timeout 120 --consent-run "intent: buy https://example.com and checkout" --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --window-smoke https://example.com --window-smoke-timeout 15
cargo run -p sextant-hull --bin sextant-browser -- --window-smoke "intent: open https://example.com and distill" --window-smoke-timeout 20
cargo run -p sextant-hull --bin sextant-browser -- --start-showcase --window-smoke --window-smoke-timeout 30
cargo run -p sextant-hull --bin sextant-browser -- --start-real-browsing --window-smoke --window-smoke-timeout 60
cargo run -p sextant-hull --bin sextant-browser -- --start-shell-interaction --window-smoke --window-smoke-timeout 45
cargo run -p sextant-hull --bin sextant-browser -- --demo
cargo run -p sextant-hull --bin sextant-browser -- --start "intent: open https://example.com and distill"
```

`--operator-smoke` runs a deterministic native browser workflow without opening the visible event loop. It validates Servo navigation, native DOM fill/click, live DOM distillation, Wake, Captain's Log, and frame capture.

`--showcase-run` runs the current launch-demo proof in one bounded command: Intent Bar, Servo navigation, Wake, Captain's Log, native form interaction, tab creation, and frame capture.

`sextant-mcp --launch-preflight` runs the launch-critical pre-demo sequence: operator smoke, Intent Bar run, showcase run, and visible showcase smoke. Use it before showing the product live. It prints a short pass/fail summary with total and per-check timings by default; add `--json` for the full structured report.

`sextant-mcp --hardening-preflight` runs the broader confidence pass: operator smoke, Intent Bar, showcase, real browsing, visible showcase, visible real browsing, and visible shell-interaction checks. Use it when hardening the product or preparing a higher-stakes demo. It also prints total and per-check timings; add `--json` for the full structured report.

`sextant-mcp --real-browsing-smoke` runs the broader user-browsing suite: example.com navigation, real Google search form fill/submit, MDN documentation distillation, reload, IANA navigation, back/forward, Wake, Captain's Log, and Servo frame capture.

The visible browser also has a `SHOWCASE` control in the main toolbar. Use it during manual demo prep to seed the app into the same proof state without leaving the window; it switches to the Validation tab and shows the recent passed showcase steps.

`--operator-probe <url-or-search>` runs the same native browser navigation/distill/Wake/frame path against a target page. Use it before manual click-through when hardening heavy browsing. Probe output includes navigation, distillation, Wake, viewport-resize, and frame-capture timings.

`--guard-probe <url-or-search>` runs a local trust-boundary check around a target page. It reports the active persona, policy source, rule counts, air-gap mode, firewall action/reason, privacy redaction status, and a redacted sample before and after navigation when navigation is allowed. If the target is blocked, the probe reports the policy block and exits successfully as a guard finding. Add `--guard-policy <json-path>` or set `SEXTANT_GUARD_POLICY` to test a JSON firewall overlay; the visible browser also checks `<browser-data-dir>/guard-policy.json`. `sextant-mcp --guard-probe <url-or-search>` and MCP tool `browser_guard_probe` expose the same report as structured `guard` output for Codex and Claude.

`sextant-mcp --guard-policy-read --json` reads the canonical browser `guard-policy.json`. `sextant-mcp --guard-policy-write <json-path> --json` validates that file as a firewall overlay and writes it to the canonical browser data directory. The equivalent MCP tools are `browser_guard_policy_read` and `browser_guard_policy_write`.

`--perception-probe <url-or-search>` runs the same native page pass, then reports page perception: classified page type, semantic counts, key semantic nodes, distillation source, and timings. `sextant-mcp --perception-probe <url-or-search>` and MCP tool `browser_perception_probe` expose the same report as structured `perception` output for Codex and Claude.

`--perf-probe <url-or-search>` is the same timed page pass labeled for performance work, plus a second warm frame capture to separate first-capture resize cost from steady-state readback. `--perf-baseline` runs a small representative set of simple and heavier pages so normal browsing tuning has a repeatable baseline.

`sextant-mcp --perf-probe <url-or-search>` and MCP tool `browser_perf_probe` expose the same timing path to Codex and Claude. Use `browser_perf_baseline` for the built-in page set through MCP.

`--operator-run <url-or-search>` adds a tiny scripted layer for native browser interaction. Supported steps are `--fill <selector> <value>`, `--click <selector>`, `--submit <selector>`, and `--expect <text>`.

`--intent-run "<intent>"` drives the native Intent Bar loop without opening the visible event loop. It currently validates deterministic planning, Servo navigation, live DOM distillation, Wake search, Captain's Log recording, and frame capture. Pair it with `--expect <text>` when the distilled page should contain a known string.

`--consent-run "<sensitive intent>"` drives the same Intent Bar path, expects a pending Captain's Key request, authorizes it, and resumes the bounded browser continuation. It is the headless regression path for AUTHORIZE/DENY work before doing a visible click-through.

`--operator-timeout <seconds>` applies to smoke, probe, and scripted runs. The default is 120 seconds; timeout exits use code `124` so stalled hard-site probes are visible to scripts instead of relying on an outer shell kill.

`--window-smoke [url-search-or-intent] --window-smoke-timeout <seconds>` opens the actual visible shell, optionally navigates or runs a native intent, draws once, reports whether a Servo frame was captured, and exits. Use it when validating the user-facing browser window rather than the operator bridge.

`--start "<url-search-or-intent>"` launches the visible browser, runs that URL/search/intent, and leaves the window open for demo or manual testing.

`--start-real-browsing` seeds the visible browser with the same normal-browsing proof used by `--real-browsing-smoke`, then leaves the window open. Pair it with `--window-smoke --window-smoke-timeout 60` for a bounded visible check that the user-facing shell still draws after the workflow.

`--start-shell-interaction` clicks through the visible chrome controls in-process: address/run, distill, Wake, view tabs, viewport focus, new tab, and close tab. Pair it with `--window-smoke --window-smoke-timeout 45` for a bounded control-surface check.

`--demo` is the shortest visible launch path. It runs the launch showcase, opens the proof panel, and leaves the browser window open. `--start-showcase` does the same seeding and can be combined with `--window-smoke` for bounded visible verification.

### Full Workspace Guardrail

```bash
cd rust
cargo check --workspace
cargo test --workspace
```

## Documentation Rules

Update docs when any of these changed:

- the real product status
- the recommended work order
- the system shape
- the repo workflow
- the recovery/onboarding path

Usually that means touching one or more of:

- [current-state.md](current-state.md)
- [next-work.md](next-work.md)
- [architecture.md](architecture.md)
- [completed.md](completed.md)
- [archive](archive)

## What Counts As The Product Path

The product path is:

- user enters intent
- Pilot reasons about it
- engine navigates and distills
- Wake records it
- Captain's Log reflects it
- hull shows truthful state

Prefer changes that make this loop more real, more observable, or more reliable.

## What To Avoid

- polishing one panel too long while the core loop is still incomplete
- hiding failures behind fake success states
- growing giant inline Xilem view trees
- putting orchestration logic directly inside widget callbacks
- treating the TypeScript simulator as the main product path

## Commit And Push

The canonical remote is `rustcor/Sextant` on Gitea.

The repo also has a Gitea workflow at `.gitea/workflows/rust-ci.yml` that runs the Rust workspace check/test guardrail on pushes and pull requests.

Normal finish:

```bash
git status
git add -A
git commit -m "Your concise message"
git push
```
