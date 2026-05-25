# Next Work

Short active execution list. Older detailed command inventories are archived in [archive/2026-05-24-next-work-raw-direct-long.md](archive/2026-05-24-next-work-raw-direct-long.md).

## Current Priority

Focus on Servo/direct real-window browsing until it is first-class and performant.

The direct lane now proves fast first present, fast early interaction, verified controlled click/type input, controlled form submit, and a representative live search on DuckDuckGo Lite. The next gap is making that raw lane feel like the real browser: broader live-site interaction, direct chrome/tab/address overlay, and stable normal browsing.

## Immediate Tasks

1. **Extend direct interaction proof to live pages**
   - Controlled `--verified-input-smoke` now clicks/focuses/types and verifies page-observed text.
   - Controlled `--search-submit-smoke` now types into a form, submits with Enter, and reports the final `/search?q=sextant` URL/title.
   - `--live-search-smoke` now drives DuckDuckGo Lite with keyboard focus traversal and reports final title/URL.
   - Google live search currently reaches different anti-automation/unsupported-browser outcomes; MCP now reports `directPresentMs`, `liveSearchAttempts`, `timeoutTitle`, `timeoutUrl`, `liveSearchSubmitted`, and `liveSearchBlocked` when available.
   - Next: debug Google focus/input routing, then try another representative complex live page.
   - Track `verifiedInputFrameMs`, `searchSubmitFrameMs`, `liveSearchFrameMs`, `locationUrl`, `historyFinalUrl`, `loadUrl`, `reloadUrl`, `tabUrl`, `firstInteractionFrameMs`, load state, and any focus/hit-test failure mode.

2. **Start direct chrome overlay design**
   - Decide how the shell chrome/address/tab strip composes over or alongside Servo's direct window context.
   - Preserve Direct/Incognito AI exclusion.
   - Keep Agent/Assisted/Observe on the bridge until overlay/observation boundaries are explicit.

3. **Keep bridge shell stable**
   - Re-run assisted user-DISTILL and normal window-smoke checks after direct changes.
   - Do not regress the existing chrome/tabs/Wake/Log/Guard/Sense/Perf surfaces.

4. **Measure complex-page tail**
   - Keep `loadCompleteMs` for Google and other heavy pages.
   - Treat first present and first interaction as user-speed metrics; treat load completion as tail health.

## High-Signal Commands

Run from:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
```

Direct path:

```bash
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode direct --render-path direct --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode direct --render-path direct --load-smoke --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode direct --render-path direct --reload-smoke --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode direct --render-path direct --shell-interaction --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke https://www.google.com --mode direct --render-path direct --load-smoke --timeout-seconds 60 --json
cargo run -p sextant-mcp -- --window-smoke --mode direct --render-path direct --first-interaction-smoke --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke --mode direct --render-path direct --verified-input-smoke --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke --mode direct --render-path direct --search-submit-smoke --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke --mode direct --render-path direct --live-search-smoke --timeout-seconds 60 --json
cargo run -p sextant-mcp -- --window-smoke https://www.google.com --mode direct --render-path direct --first-interaction-smoke --timeout-seconds 60 --json
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode incognito --render-path direct --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke "data:text/html,%3C!doctype%20html%3E%3Ctitle%3EDirect%20Resize%20Smoke%3C%2Ftitle%3E%3Cbody%3EDirect%20resize%20smoke%3C%2Fbody%3E" --mode direct --render-path direct --resize-smoke --timeout-seconds 45 --json
```

Bridge shell:

```bash
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode assisted --user-distill --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --real-browsing-smoke --timeout-seconds 240
cargo run -p sextant-mcp -- --hardening-preflight --timeout-seconds 240 --visible-timeout-seconds 60
```

Build/test:

```bash
cargo fmt --all --check
cargo check -p sextant-hull --bin sextant-browser
cargo check -p sextant-hull --bin sextant-servo-direct
cargo check -p sextant-mcp
cargo test -p sextant-mcp parses_window_smoke_summary -- --nocapture
cargo test -p sextant-mcp parses_window_smoke_cli_arguments -- --nocapture
cargo test -p sextant-mcp window_smoke_blocks_control_work_in_non_control_modes -- --nocapture
```

## Current Direct Smoke Meanings

| Flag | Meaning |
|---|---|
| `--render-path direct` | Raw Servo native window presentation; Direct/Incognito only. |
| `--load-smoke` | Waits for active WebView `LoadStatus::Complete`; reports `loadCompleteMs`, `loadUrl`, and `loadTitle`. |
| `--first-interaction-smoke` | Sends input immediately after first present; reports early interaction frame timing. |
| `--verified-input-smoke` | Serves a controlled localhost page, clicks/focuses an input, types through Servo, and reports `verifiedInputFrameMs`. |
| `--search-submit-smoke` | Serves a controlled localhost form, types, submits with Enter, verifies `/search?q=sextant`, and reports `searchSubmitFrameMs`, `searchSubmitUrl`, and `searchSubmitTitle`. |
| `--live-search-smoke` | Opens DuckDuckGo Lite by default, focuses with keyboard traversal, types/submits through raw Servo input, and reports `liveSearchFrameMs`, `liveSearchUrl`, `liveSearchTitle`, attempts, timeout URL/title, and submitted/blocked diagnostics. |
| `--location-smoke <target>` | Drives the direct title-bar location/search path and reports `locationUrl` / `locationTitle`. |
| `--history-smoke <target>` | Navigates to target, then back/forward, and reports `historyFinalUrl` / `historyFinalTitle`. |
| `--reload-smoke` | Calls direct Servo reload and reports follow-up frame plus `reloadUrl` / `reloadTitle`. |
| `--resize-smoke` | Requests native window resize and reports post-resize frame. |
| `--start-shell-interaction` | Direct tab smoke when paired with `--render-path direct`; opens a second tab, switches away/back, waits for the selected tab page, and reports `tabUrl` / `tabTitle`. |

## Guardrails

- Keep Direct and Incognito free of AI DOM/frame observation and content persistence.
- Do not call first present "loaded"; use `loadCompleteMs` for that.
- Keep current bridge smokes green while direct matures.
- Record any timing movement in [performance-log.md](performance-log.md).
