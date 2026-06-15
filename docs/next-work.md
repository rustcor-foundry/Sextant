# Next Work

Short active execution list. Older detailed command inventories are archived in [archive/2026-05-24-next-work-raw-direct-long.md](archive/2026-05-24-next-work-raw-direct-long.md).

## Current Priority

Focus on Servo/direct real-window browsing until it is first-class and performant.

The direct lane now proves fast first present, fast early interaction, verified controlled click/type input, controlled form submit, a representative live search on DuckDuckGo Lite, local appliance TLS handling, and a repeatable MCP baseline. The next gap is making that raw lane feel like the real browser: broader live-site interaction, direct chrome/tab/address overlay, and stable normal browsing.

**Active thrust (2026-06-14): web-platform feature ungating.** Servo ships many stable features gated off by conservative defaults; real sites silently break on them. We probe-verify and enable the safe ones in the direct lane. Enabled so far: IntersectionObserver, CSS Grid, FontFace, VisualViewport, adoptedStyleSheets, CompositionEvent. The prioritized work/reward roadmap and per-feature status live in [web-platform-gating.md](web-platform-gating.md). Findings from the first pass: the gated CSS *layout* features beyond Grid (container queries, multi-column) are **parse-only / unimplemented** (no benefit); `layout_writing_mode_enabled` **panics**. **IndexedDB** is now enabled — it required profile-scoping the direct lane's `config_dir` (persistent `browser/direct-servo` for Direct; ephemeral temp dir swept on close for Incognito, since the embedded child is hard-killed and cannot self-clean). That also profile-scopes cookies/HSTS/auth-cache. **WebVTT** is now enabled too (`VTTCue`/`VTTRegion` constructible). **execCommand** is left off — partial (Servo supports only 5 commands; common editor commands like `bold`/`italic`/`insertText` no-op). The high-reward implemented-DOM-API queue is now worked through; remaining gated features are niche/perf, privacy-gated (need consent UX), or heavy (service worker, WebRTC, WebGPU/WebGL2, worklets) — see [web-platform-gating.md](web-platform-gating.md). Good point to shift focus back to direct-lane polish (HiDPI chrome/content alignment, tab-strip icons, port appliance-cert trust to the egui chrome) and keeping the baselines green. Recent landings also overhauled the hosted-direct chrome render path (adaptive wgpu/softbuffer, idle CPU ~343%→0%, egui redraw-loop fix, 60fps animation cap); see [performance-log.md](performance-log.md).

**Strategic direction toward alpha (2026-06-14).** Direct/server browsing is now solid and ahead of plan (fresh-build baseline green, see [performance-log.md](performance-log.md)). Keep working through the gated features opportunistically — the cheap/high-value ones are done; the rest are niche/perf, privacy-gated, or heavy. The next major thrust is **rounding out full browser mode with the AI-enabled UI** (the bridge shell: chrome/tabs + Wake/Log/Guard/Sense/Perf + AI observe/control). Goal: make the full UI genuinely usable for AI-driven workflows; once that bar is met, ship a **solid alpha**. The `Local` AI provider is now wired to a real model: the app defaults to the on-box llama.cpp server at `http://127.0.0.1:8101` (Qwen2.5-Coder-32B-Instruct on the P40), and `brain_probe` confirms `LocalBrain` produces a clean `Navigate → Distill → Analyze` plan against it. Remaining local-AI follow-ups: (a) make the local backend selectable in-app (Ollama vs llama.cpp — `build_brain` currently hardcodes `LlamaCpp`); (b) wire and test the full `process_intent → execute` loop in the live browser; (c) resolve the native-widget crash blocking the live agent UI. Near-term bridge-shell work: re-validate the Agent/Assisted shell against the matured direct lane, keep its smokes green, and close the gap between the polished direct chrome and the AI shell. Media/video is a separate future track (no playback today — `<video>` needs the GStreamer backend; YouTube needs MSE, which Servo lacks; see [current-state.md](current-state.md)).

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
   - `--live-form-smoke` now drives `https://httpbin.org/forms/post`, DOM-locates `input[name="custname"]`, types through raw Servo input, clicks the page submit button, and verifies `https://httpbin.org/post`; latest hosted run reported `liveFormFrameMs=484`.
   - MCP now reports `directPresentMs`, `liveSearchAttempts`, `liveSearchDomTarget`, `liveSearchDomFormTargetUrl`, `liveFormFrameMs`, `liveForm`, `liveFormUrl`, `liveFormAttempts`, `liveFormDomTarget`, `liveFormSubmitted`, `liveFormBlocked`, `liveFormBlockReason`, `timeoutTitle`, `timeoutUrl`, `liveSearchSubmitted`, `liveSearchBlocked`, and `liveSearchBlockReason`; no-query Google `/webhp` timeouts are classified as `input-not-submitted-no-query`, while other home-page no-submit failures are classified as `input-not-submitted`; hosted form failures now distinguish `input-not-submitted`, `submitted-without-navigation`, and `submitted-without-post-verification`.
   - `--live-link-smoke` now proves real hyperlink navigation, wired end-to-end through the standalone `sextant-servo-direct` bin, `sextant-browser --window-smoke`, MCP `browser_window_smoke`, and the `browser_direct_browsing_baseline` (as an optional `live_link` case). It DOM-locates the first visible cross-document `a[href]`, clicks it through raw Servo hit-testing, and verifies the resulting cross-host navigation (accepting apex/`www` redirects within the registrable domain). Latest baseline run drove `example.com`'s `iana.org` link through the retained-navigation swap to `https://www.iana.org/help/example-domains`; MCP surfaces `liveLink`, `liveLinkUrl`, `liveLinkTitle`, and `liveLinkFrameMs` (`916ms`-`1.3s`).
   - Next: keep Google, Bing, and the hosted form in the regular direct smoke rotation while adding more representative live-site controls (multi-field forms, scroll-into-view). Promote the `live_link` baseline case from optional to required once it proves stable across runs.
   - Track `verifiedInputFrameMs`, `searchSubmitFrameMs`, `liveSearchFrameMs`, `liveFormFrameMs`, `locationUrl`, `historyFinalUrl`, `loadUrl`, `reloadUrl`, `tabUrl`, `firstInteractionFrameMs`, load state, and any focus/hit-test failure mode.

3. **Keep the direct browsing baseline green**
   - `browser_direct_browsing_baseline` covers simple load, complex page, optional local appliance load, live search, and hosted live form.
   - Latest QA follow-up: example.com `162ms` first present / `477ms` complete, DuckDuckGo `165ms` first present / `2.2s` complete with a `1.3s` loading paint, Pylon `143ms` first present / `262ms` complete, DuckDuckGo Lite search `841ms`, and hosted `httpbin` form submit `492ms`.
   - Direct ready-frame pacing is active for non-input frame-ready floods; the baseline now records frame counts and slow-frame phases.
   - `browser_direct_viewport_baseline` now checks DuckDuckGo at `640x420`, `960x620`, and `1180x760`; latest max paints landed at `891ms`, `833ms`, and `837ms` with about `780` elements, `2,904` CSS rules, `48` scripts, and `74` resources, so the remaining heavy frame is still likely Servo/WebRender scene/content work rather than raw pixel fill.
   - `browser_direct_complexity_baseline` compares simple, lite, complex, and article pages at `960x620`; latest run separates audit-only paint from true loading paint and shows DuckDuckGo full `818ms` / MDN `903ms` loading paint.
   - Treat required baseline failures as release blockers for the direct lane.

4. **Direct chrome overlay (egui)**
   - Interactive hosted launches use `run_hosted_direct_app_egui`: an egui 0.27 chrome in the parent window hosting the embedded Servo child below. Address bar, back/forward/reload, new/previous/next/close tab, load progress, and title/cert status are real egui widgets wired to the stdin child commands. The softbuffer `run_hosted_direct_app` is retained for `--hosted-direct-smoke`.
   - GUI toolkit decision: egui (not Bevy) — Bevy's frame ownership would force Servo content back through a capture-bridge; egui only draws the parent chrome strip while the Servo child HWND keeps the content surface.
   - Render backend is adaptive (`init_chrome_backend`): wgpu when a hardware GPU adapter is present (`device_type != Cpu`), else a CPU softbuffer software rasterizer (`rasterize_chrome`). On GPU-less hosts the old wgpu/WARP path spun a rasterizer thread pool (~3.4 cores idle); software is ~0%. The web content is always rendered by the Servo child (GPU when available). See [performance-log.md](performance-log.md).
   - Fixed: an egui `RedrawRequested→repaint→request_redraw` feedback loop redrew the static chrome ~37x/s (masked by the WARP pool) — guarded against re-requesting a redraw for `RedrawRequested`. Also fixed the embedded child render stall via the Servo `EventLoopWaker` bridge (`DirectServoWaker`).
   - Preserve Direct/Incognito AI exclusion.
   - Next: HiDPI chrome/content boundary alignment (chrome in logical points vs child embedded at 72 physical px), icon/tab-strip polish, and port the appliance-cert trust→relaunch flow from the softbuffer shell.

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
   - `record_direct_frame_timing` now co-records the worst-total direct frame's spin/paint/present (`keep_worst_total_frame`), so `maxDirectSpinMs/PaintMs/PresentMs` are a truthful decomposition of `maxDirectFrameMs` rather than independent maxima.
   - Attribution (2026-06-11): the heavy loading frame is ~100% WebRender `webview.paint()` — `spin`≈`0ms`, `present`≈`1ms`, paint `1.1s` (DDG full) / `794ms` (MDN); paint is flat across a `3.3x` pixel-area range, so it is content/scene-bound, not GPU fill-rate. See [performance-log.md](performance-log.md).
   - Debug-tax sizing done (2026-06-11): rebuilding deps with `debug-assertions`/`overflow-checks` off did not move the heavy paint (DDG full `1.1s` unchanged). Deps are already `opt-level 3`, so the paint is genuine WebRender cost, not a debug artifact; the profile change was reverted.
   - Next: treat the heavy single-frame paint as an upstream/structural WebRender cost (no cheap our-side lever left); keep `loadCompleteMs` as the separate network/subresource tail metric and focus tuning effort on in-our-control areas (chrome composition, pacing, input latency).

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
cargo run -p sextant-hull --bin sextant-servo-direct -- --live-link-smoke --timeout-seconds 30
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
| `--live-link-smoke` | Opens `example.com` by default, DOM-locates the first visible cross-document `a[href]`, clicks it through raw Servo hit-testing, and verifies the resulting navigation by registrable-domain host match (accepts apex/`www` redirects); reports `liveLink`, `liveLinkUrl`, `liveLinkTitle`, and `liveLinkFrameMs`. Wired through the standalone bin, `--window-smoke`, MCP, and the direct browsing baseline (optional `live_link` case). |
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
- Update the docs before every push (`current-state.md`, `performance-log.md`, this file, and any feature roadmap like [web-platform-gating.md](web-platform-gating.md)). Do not let them go stale between pushes.
- Never enable a Servo feature gate blind — probe-verify it functions first; some gates protect incomplete code that panics (e.g. `layout_writing_mode_enabled`). See [web-platform-gating.md](web-platform-gating.md).
