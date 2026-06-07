# Next Work

Short active execution list. Older detailed command inventories are archived in [archive/2026-05-24-next-work-raw-direct-long.md](archive/2026-05-24-next-work-raw-direct-long.md).

## Current Priority

Focus on Servo/direct real-window browsing until it is first-class and performant.

The direct lane now proves fast first present, fast early interaction, verified controlled click/type input, controlled form submit, a representative live search on DuckDuckGo Lite, local appliance TLS handling, and a repeatable MCP baseline. The next gap is making that raw lane feel like the real browser: broader live-site interaction, direct chrome/tab/address overlay, and stable normal browsing.

## Immediate Tasks

1. **Add user-facing appliance certificate flow**
   - Keep strict TLS as the default for normal browsing.
   - Use an interstitial for local appliance certificate failures with `Go Back`, `Trust Once`, and `Trust This Appliance`.
   - Persist only fingerprint-bound origin exceptions in the normal profile; if the certificate changes, warn again.
   - Incognito can allow `Trust Once` for the current session but must not persist certificate exceptions.
   - Keep the dev/test `--allow-insecure-local-tls` path limited to localhost, `.local`, private IP, and link-local targets.
   - The direct lane now decorates Servo's bad-certificate page as the first Sextant local-appliance warning and wires `Trust This Appliance` to the native profile trust store.
   - Hosted direct chrome now parses the child certificate fingerprint, shows native `ONCE` / `TRUST` controls, sends trust commands to the child, and relaunches the trusted replacement child embedded in the same parent shell.
   - `browser_hosted_direct_smoke` now gives MCP/Codex/Claude a bounded parent-shell proof with embedded-child first-present and certificate fingerprint telemetry.
   - Hosted direct warning chrome now includes a native `BACK` escape that goes back or falls back to `about:blank`, and `--list-local-appliance-certs` / `--forget-local-appliance-cert <url|all>` plus MCP `browser_local_appliance_cert_list` / `browser_local_appliance_cert_forget` provide the settings foundation.
   - The Guard tab now lists persisted local appliance certificate trust entries, shows compact SHA-256 fingerprints and timestamps, supports refresh, and can forget a selected trust entry from the native shell.
   - Hosted direct smoke can now drive native certificate actions with `--hosted-direct-smoke-cert-action back|once|trust`; Pylon `BACK`, `ONCE`, and isolated-profile `TRUST` proofs are green.
   - Next: manual QA this against the Pylon appliance and polish warning/Guard settings copy and states.

2. **Extend direct interaction proof to live pages**
   - Controlled `--verified-input-smoke` now clicks/focuses/types and verifies page-observed text.
   - Controlled `--search-submit-smoke` now types into a form, submits with Enter, and reports the final `/search?q=sextant` URL/title.
   - `--live-search-smoke` now drives DuckDuckGo Lite with keyboard focus traversal and reports final title/URL.
   - Google live search now uses a DOM-located search-box click before normal Servo text/Enter input; latest smoke reached `/search?q=Sextant` with `6.1s` live-search frame.
   - Bing live search now passes without the `NodeList` panic after applying the local Servo checkout patch captured in `docs/patches/servo-nodelist-bing-panic.patch`: the unpatched backtrace hit `NodeList.hasOwn` via `Array.prototype.slice.call(nodeList)`, while the patched rebuild reached a normal Bing search URL with a `6.3s` live-search frame and no `nodelist.rs` panic. `rust/scripts/apply-servo-patches.ps1` can apply/verify the patch against the Cargo-locked Servo checkout; next durability step is to carry this as an upstream Servo PR or pin/fork patch.
   - MCP now reports `directPresentMs`, `liveSearchAttempts`, `liveSearchDomTarget`, `liveSearchDomFormTargetUrl`, `timeoutTitle`, `timeoutUrl`, `liveSearchSubmitted`, `liveSearchBlocked`, and `liveSearchBlockReason`; no-query Google `/webhp` timeouts are classified as `input-not-submitted-no-query`, while other home-page no-submit failures are classified as `input-not-submitted`.
   - Next: keep Google and Bing in the regular direct smoke rotation, then add one more non-search live input representative so the lane is not tuned only around search pages.
   - Track `verifiedInputFrameMs`, `searchSubmitFrameMs`, `liveSearchFrameMs`, `locationUrl`, `historyFinalUrl`, `loadUrl`, `reloadUrl`, `tabUrl`, `firstInteractionFrameMs`, load state, and any focus/hit-test failure mode.

3. **Keep the direct browsing baseline green**
   - `browser_direct_browsing_baseline` covers simple load, complex page, optional local appliance load, and live search.
   - Latest QA follow-up: example.com `270ms` first present / `581ms` complete, DuckDuckGo `114ms` first present / `1.6s` complete with a `799ms` loading paint, Pylon `110ms` first present / `249ms` complete, DuckDuckGo Lite search `703ms`.
   - Direct ready-frame pacing is active for non-input frame-ready floods; the baseline now records frame counts and slow-frame phases.
   - `browser_direct_viewport_baseline` now checks DuckDuckGo at `640x420`, `960x620`, and `1180x760`; latest max paints landed at `891ms`, `833ms`, and `837ms` with about `780` elements, `2,904` CSS rules, `48` scripts, and `74` resources, so the remaining heavy frame is still likely Servo/WebRender scene/content work rather than raw pixel fill.
   - `browser_direct_complexity_baseline` compares simple, lite, complex, and article pages at `960x620`; latest run separates audit-only paint from true loading paint and shows DuckDuckGo full `818ms` / MDN `903ms` loading paint.
   - Treat required baseline failures as release blockers for the direct lane.

4. **Start direct chrome overlay design**
   - Hosted direct composition now runs as a parent Sextant chrome window with an embedded Servo child window.
   - Parent chrome currently owns address input, load progress, back/forward/reload, new/previous/next/close tab controls, active tab count/title, child PID, native appliance-certificate trust controls, and log-derived perf state.
   - Hosted shell now has a bounded MCP smoke: `browser_hosted_direct_smoke` / `--hosted-direct-smoke`.
   - Preserve Direct/Incognito AI exclusion.
   - Next: manual QA the hosted shell, polish button labels/states, and keep Agent/Assisted/Observe on the bridge until overlay/observation boundaries are explicit.

5. **Keep bridge shell stable**
   - Re-run assisted user-DISTILL and normal window-smoke checks after direct changes.
   - Latest assisted bridge smoke passed on `example.com`: first shell draw `4ms`, first Servo frame `758ms`, nav `277ms`, distill `409ms`, Wake `60ms`.
   - Do not regress the existing chrome/tabs/Wake/Log/Guard/Sense/Perf surfaces.

6. **Track remaining Servo dependency advisories**
   - Current repo-controlled dependency cleanup removed the legacy npm surface, `reqwest 0.11`, `readability`, `xml5ever 0.16`, `time 0.1`, and the older `rustls-webpki` advisory path.
   - `cargo audit` still reports `ml-dsa 0.0.4` and `rsa 0.9.10` through Servo's `servo-script`.
   - Do not force a local cryptography patch into Servo without a focused compatibility pass; prefer a Servo upstream update or an explicit audited patch strategy.

7. **Measure complex-page tail**
   - Keep `loadCompleteMs` for Google and other heavy pages.
   - Treat first present and first interaction as user-speed metrics; treat load completion as tail health.

## High-Signal Commands

Run from:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
```

Direct path:

```bash
cargo run -p sextant-mcp -- --direct-browsing-baseline --timeout-seconds 75 --json
cargo run -p sextant-mcp -- --direct-viewport-baseline --timeout-seconds 75 --json
cargo run -p sextant-mcp -- --direct-complexity-baseline --timeout-seconds 75 --json
cargo run -p sextant-mcp -- --hosted-direct-smoke https://example.com --timeout-seconds 30 --json
cargo run -p sextant-mcp -- --hosted-direct-smoke https://192.0.2.130:8080/app/login --hosted-direct-smoke-cert-action back --timeout-seconds 60 --json
cargo run -p sextant-mcp -- --hosted-direct-smoke https://192.0.2.130:8080/app/login --hosted-direct-smoke-cert-action once --timeout-seconds 75 --json
cargo run -p sextant-mcp -- --hosted-direct-smoke https://192.0.2.130:8080/app/login --hosted-direct-smoke-cert-action trust --hosted-direct-smoke-isolated-profile --timeout-seconds 90 --json
cargo run -p sextant-mcp -- --list-local-appliance-certs --json
cargo run -p sextant-mcp -- --forget-local-appliance-cert https://192.0.2.130:8080/app/login --json
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
cargo run -p sextant-mcp -- --window-smoke https://duckduckgo.com/ --mode direct --render-path direct --load-smoke --resource-audit --timeout-seconds 75 --json
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode incognito --render-path direct --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke "data:text/html,%3C!doctype%20html%3E%3Ctitle%3EDirect%20Resize%20Smoke%3C%2Ftitle%3E%3Cbody%3EDirect%20resize%20smoke%3C%2Fbody%3E" --mode direct --render-path direct --resize-smoke --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke https://192.0.2.130:8080/app/login --mode direct --render-path direct --load-smoke --allow-insecure-local-tls --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke https://192.0.2.130:8080/app/login --mode direct --render-path direct --load-smoke --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke https://192.0.2.130:8080/app/login --mode direct --render-path direct --load-smoke --remember-local-appliance-cert <sha256-fingerprint> --timeout-seconds 45 --json
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
| `--live-search-smoke` | Opens DuckDuckGo Lite by default, and can also target live search pages such as Google; it tries keyboard focus, a DOM-located search-box click with normal Servo text/Enter input, fixed click fallback, and focus sweep, then reports `liveSearchFrameMs`, `liveSearchUrl`, `liveSearchTitle`, attempts, timeout URL/title, and submitted/blocked diagnostics. |
| `--resource-audit` | Direct load-smoke add-on that captures page resource health after load; reports image, broken image, SVG, stylesheet, script, and canvas evidence. |
| `--location-smoke <target>` | Drives the direct title-bar location/search path and reports `locationUrl` / `locationTitle`. |
| `--history-smoke <target>` | Navigates to target, then back/forward, and reports `historyFinalUrl` / `historyFinalTitle`. |
| `--reload-smoke` | Calls direct Servo reload and reports follow-up frame plus `reloadUrl` / `reloadTitle`. |
| `--resize-smoke` | Requests native window resize and reports post-resize frame. |
| `--start-shell-interaction` | Direct tab smoke when paired with `--render-path direct`; opens a second tab, switches away/back, waits for the selected tab page, and reports `tabUrl` / `tabTitle`. |
| `--allow-insecure-local-tls` | Direct-render dev/test path for local appliances with bootstrap/self-signed certs. It is limited to localhost, `.local`, private, and link-local targets and should evolve into user-visible certificate exceptions. |
| Strict local appliance load | Without a bypass, local appliance certificate errors report `certificateFingerprintSha256` in the MCP/window-smoke summary so the UI can offer fingerprint-bound trust. |
| `--remember-local-appliance-cert <sha256>` | Stores a profile-scoped local appliance exception for the target origin and fingerprint, then allows the current direct smoke. Disabled in Incognito. |
| `--local-appliance-cert-fingerprint <sha256>` | Checks the profile-scoped trust store for an existing origin/fingerprint match and allows the direct smoke only when it matches. |
| `--hosted-direct-smoke-cert-action back\|once\|trust` | Hosted-direct smoke add-on that waits for local appliance certificate telemetry, drives the native certificate chrome action, and reports requested/completed/relaunched evidence. Pair `trust` with `--hosted-direct-smoke-isolated-profile` when the proof should not mutate the normal profile trust store. |
| `--hosted-direct-smoke-isolated-profile` | Runs hosted-direct smoke with a temporary `SEXTANT_BROWSER_DATA_DIR` root and removes it after the command exits. |
| `--list-local-appliance-certs` | Lists persisted profile-scoped local appliance certificate exceptions. |
| `--forget-local-appliance-cert <url|all>` | Removes a profile-scoped local appliance certificate exception for one origin, or clears all persisted local appliance exceptions. |

## Guardrails

- Keep Direct and Incognito free of AI DOM/frame observation and content persistence.
- Keep appliance TLS trust explicit and scoped; never silently disable TLS validation for public browsing.
- Do not call first present "loaded"; use `loadCompleteMs` for that.
- Keep current bridge smokes green while direct matures.
- Record any timing movement in [performance-log.md](performance-log.md).
