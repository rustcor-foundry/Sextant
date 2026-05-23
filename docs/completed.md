# Completed Work

Audit trail of completed work sessions. Newest first.

---

## 2026-05-22 — Visible Navigation Phase Telemetry And First-Frame Resize Removal

- Added async navigation timing fields for firewall checks, Servo navigation/control work, Servo inspect, and reader fallback
- Recorded those timings in the browser perf history as `nav-phase` events without overwriting the coarse `nav/distill/wake/resize/frame` summary
- Extended visible window smoke output with the slowest perf event and top perf events so bridge resize, navigation sub-phases, and frame capture costs are visible in one bounded run
- Extended MCP window-smoke parsing with structured `slowestPerfEvent` and `slowPerfEvents`, and tightened perf-summary parsing so human-readable slowest lines do not overwrite the coarse summary fields
- Started the first post-navigation bridge capture immediately when async visible navigation completes, removing avoidable refresh cadence slack before the first page frame
- Added `frame-total` perf events for async frame workers so queue/wall time can be compared with resize and capture costs
- Added an initial Servo viewport hint for the visible browser shell and taught Servo tab sessions to skip same-size resize requests, so a fresh Servo runtime can be born at the browser viewport size without using the rejected pre-size-navigation path
- Folded the first bridge frame into the async navigation result, so the visible shell can apply the real Servo frame as soon as navigation completes and skip the extra first-frame capture worker
- Confirmed `https://example.com` window smoke now first-frames through the bridge in about 520ms, with first shell draw around 22ms, navigation around 432ms, Servo navigation around 405ms, bridge resize at 0ms, and frame capture around 23ms

---

## 2026-05-19 — Direct Servo Presentation Proof

- Added `sextant-servo-direct`, an experimental Servo-backed proof binary that uses Servo's `WindowRenderingContext`, `WebViewBuilder`, direct `paint()`, and `present()` path instead of the current frame-capture-to-Softbuffer render bridge
- Added the required `winit 0.30` and direct Servo optional dependencies behind the existing `servo-backend` feature without changing the production `sextant-browser` event loop yet
- Shared the Windows ANGLE runtime bootstrap with the direct proof so Servo/surfman can find `libEGL.dll` and `libGLESv2.dll` before context creation
- Confirmed `cargo check -p sextant-hull --bin sextant-servo-direct` passes
- Confirmed `cargo run -p sextant-hull --bin sextant-servo-direct -- --smoke --url https://example.com --timeout-seconds 45` reaches first direct present in about 165ms
- Added MCP `browser_direct_present_smoke` and CLI `sextant-mcp --direct-present-smoke <url> --json` so agents can run the direct Servo presentation proof and receive structured `directPresent` timing data
- Confirmed `cargo run -p sextant-mcp -- --direct-present-smoke https://example.com --timeout-seconds 45 --json` returns `directPresent.firstPresentMs` around 163ms with `frameReadback=false`
- Added a production user render-path boundary in `sextant-browser`: `BRIDGE` is the integrated frame-capture-to-Softbuffer path, while parsed `DIRECT` requests fail with a clear production-integration-incomplete message
- Added window-smoke render-path reporting and MCP parsing for `renderPath`, `renderPathStatus`, and `renderGap`
- Confirmed Direct-mode window smoke reports `renderPath=BRIDGE`, `renderGap=direct compositor pending`, and still captures a real Servo frame through the temporary bridge
- Confirmed `sextant-browser --render-path direct --window-smoke` exits honestly instead of silently falling back
- Added MCP `browser_render_path_baseline` and CLI `sextant-mcp --render-path-baseline <url> --json` to compare direct Servo presentation against the production bridge-backed window smoke
- Added MCP `browser_window_smoke.render_path` pass-through so agents can explicitly request `bridge` or prove that `direct` is still rejected in the production shell
- Confirmed the first example.com render-path baseline: direct present 177ms, production bridge first frame 1.0s, bridge resize 395ms, bridge capture 41ms, first-frame gap 823ms
- Added opt-in `--pre-size-navigation` for visible browser smoke so pre-navigation viewport sizing can be measured without changing the production default
- Expanded `browser_render_path_baseline` to include the pre-sized bridge variant; example.com confirmed pre-sizing removes resize cost but regresses first frame to 8.3s, so the default bridge path remains better for now
- Updated active docs to treat direct Servo presentation as the next render-bridge integration milestone instead of more frame-readback tuning

---

## 2026-05-17 — Normal Browsing Performance Timing

- Added a visible `GUARD` tab to the native browser for local trust-boundary status
- Wired browser guard reports through `sextant-airgap`, `sextant-firewall`, and `sextant-privacy` so the active persona/page can show air-gap, firewall, and redaction state
- Added `--guard-probe <url-or-search>` for bounded native browser guard reports before and after navigation/distillation
- Exposed guard probing through MCP as `browser_guard_probe`, with parsed `structuredContent.guard` output for agents
- Added the first GUARD enforcement pass: normal navigation and scripted browser interactions ask the local guard path, blocked targets are audited and stopped before engine navigation, and guard probes report policy blocks as successful findings
- Added JSON firewall policy overlays for GUARD: `guard-policy.json`, `SEXTANT_GUARD_POLICY`, browser `--guard-policy <path>`, and MCP `policy_path`/`--guard-policy` can add persona-specific rules while preserving the built-in global blocklist
- Added MCP guard policy management: `browser_guard_policy_read`, `browser_guard_policy_write`, `sextant-mcp --guard-policy-read`, and `sextant-mcp --guard-policy-write <json-path>` inspect and update the canonical browser policy overlay
- Added a visible `SENSE` tab to the native browser for page perception over distilled semantic maps
- Added page perception summaries with page type, semantic counts, key semantic nodes, and distillation source metadata
- Routed Pilot `Perceive` actions to the page perception surface after distillation
- Added `--perception-probe <url-or-search>` for bounded native browser semantic perception reports
- Exposed perception probing through MCP as `browser_perception_probe`, with parsed `structuredContent.perception` output for agents
- Split visible navigation from immediate Servo frame capture so normal browsing can report navigation completion sooner and queue frame warmup separately
- Added browser performance telemetry for navigation, live distillation, Wake search, viewport resize, and Servo frame capture
- Surfaced the latest timing summary in the visible status bar with truncation so it does not collide with the right status label
- Added a visible `PERF` tab to the native browser with latest timings, slowest phase, and capped recent phase history for manual QA
- Added timing lines to operator probe output for page-level baselines
- Added `--perf-probe <url-or-search>` for a timed single-page native browsing pass with a second warm frame capture
- Added `--perf-baseline` for a repeatable simple/heavier page set
- Cached the last Servo viewport size so repeated frame refreshes skip unnecessary resize/spin work
- Reused cached Servo distillation for repeat DISTILL runs on an unchanged tab URL, relying on existing navigation/input invalidation to keep live DOM changes fresh
- Reduced visible frame warmup refresh pressure after page loads and raised idle refresh spacing for heavy pages
- Captured an initial MDN baseline: navigation 1.2s, distillation 755ms, Wake 6ms, frame capture 863ms
- Exposed browser performance timing through MCP as `browser_perf_probe` and `browser_perf_baseline`, with matching `sextant-mcp --perf-probe` and `--perf-baseline` CLI shims
- Added parsed `structuredContent.perfTimings` to MCP performance responses so agents can compare phase durations without scraping text
- Added `structuredContent.perfSummary` to MCP performance responses with max phase timings and the single slowest phase
- Added target extraction for MCP perf baseline timing samples so multi-page baselines identify which page produced each slow phase
- Routed native browser tracing to stderr at WARN level so stdout stays focused on operator/perf report lines for MCP parsing
- Tightened targeted `--window-smoke` so URL/intent/workflow smokes wait for a captured Servo frame before passing instead of accepting the first chrome redraw
- Cached the Softbuffer surface size in the visible shell draw loop so ordinary redraws do not call resize every frame
- Added first shell draw and first Servo frame timing output to visible-window smoke checks; latest debug baseline showed chrome-only draw around 33ms and `https://example.com` first Servo frame around 1.1s
- Optimized visible Servo frame blitting by replacing per-pixel float division with fixed-point stepping and row-slice writes, plus an explicit truncated-frame warning path
- Confirmed heavier visible targets still draw real Servo frames: `https://developer.mozilla.org/en-US/docs/Web/HTML` around 1.2s and `https://www.google.com` around 2.1s to first frame in debug smoke checks
- Split visible smoke performance output into startup work, first shell draw, final draw cost, Servo frame blit, Softbuffer present, and first Servo frame timing
- Confirmed the current visible bottleneck is mostly startup/navigation before the event-loop draw: `https://example.com` showed about 845ms startup work with 30ms final draw and 9ms frame blit; Google showed about 2.1s startup work with 29ms final draw and 8ms frame blit
- Queued visible URL/search/intent startup until after the first shell paint so the native browser window appears before Servo page navigation completes
- Confirmed targeted debug smokes now show fast first shell paint before real page frames: `https://example.com` first shell draw around 31ms with first Servo frame around 1.1s, and `https://www.google.com` first shell draw around 48ms with first Servo frame around 2.3s
- Added an intermediate "Opening ..." status draw before queued startup navigation runs, so the user sees truthful loading feedback before the synchronous Servo navigation chunk
- Extended the same deferred "Opening ..." path to live user Enter/RUN navigation in the visible event loop while keeping operator/showcase pre-event-loop workflows immediate
- Added regression coverage for deferred visible navigation state
- Extended deferred visible loading feedback to reload/back/forward controls and added regression coverage for deferred reload state
- Confirmed `--start-shell-interaction --window-smoke --window-smoke-timeout 45` still passes with immediate operator/showcase navigation preserved
- Extended deferred visible loading feedback to DISTILL so the core Servo-to-Wake action paints a status before live DOM distillation/Wake work begins
- Added regression coverage for deferred DISTILL state
- Changed viewport wheel input to queue frame warmup instead of synchronously capturing a Servo frame in the wheel handler
- Added an enqueue-only Servo wheel command and switched the visible browser wheel path to it so wheel input no longer waits for Servo acknowledgement
- Added regression coverage for viewport wheel frame-warmup scheduling
- Added a smaller interaction frame-warmup budget for viewport mouse/key/wheel input so input bursts do not rearm the full navigation capture window
- Added async Servo frame-capture requests for the visible event loop so frame refresh can poll a background worker instead of blocking the window loop
- Moved the visible event-loop resize-plus-capture path onto a Servo worker so expensive viewport resize/layout work no longer blocks mouse/key/wheel handling while the next frame is being captured
- Added viewport mouse-move backpressure so passive hover movement is forwarded to Servo at a bounded rate with small-move suppression, while clicks still force the final pointer position before button dispatch
- Lowered viewport input frame warmup to a single follow-up capture so typing/scrolling does not keep the capture loop hot after every input burst
- Trimmed browser text input overhead by forwarding printable characters only on key press, not on key release, so Servo no longer receives no-op character release commands
- Added Servo service-side coalescing for adjacent printable text commands so fast typing can commit runs of text with fewer service-loop spins
- Debounced interaction frame capture by resetting the dirty-frame timer on viewport input, allowing typing bursts to settle before the next captured frame competes with key delivery
- Added a shell-side printable-text debounce queue so fast typing sends one compact Servo text commit after a short pause, then schedules the next frame immediately after the commit instead of waiting the full dirty-frame interval
- Made small-window layout use the real window dimensions instead of fixed 900x620 assumptions; narrow windows now hide the right rail and keep the browser viewport inside the visible window
- Added regression coverage for debounced viewport text input and small-window viewport bounds
- Added the first browser mode boundary in the active shell: Agent, Assisted, Observe, Direct, and Incognito
- Added per-mode capability flags for AI control, DOM/frame observation, Wake reads/writes, content logging, MCP exposure, and direct-render intent
- Added visible mode controls in the top bar plus mode status in the bottom bar
- Routed DOM distillation, Wake reads/writes, and Captain's Log content persistence through mode capabilities; Direct and Incognito now block AI observation/persistence while preserving the temporary frame-capture render bridge
- Added regression coverage proving Direct mode disables AI observation/persistence and Agent mode restores full capability
- Moved normal visible URL/search navigation onto a Servo navigation worker so the real window can keep pumping events while page load runs, instead of blocking the UI thread after the loading status paints
- Paused visible frame refresh while navigation is pending so capture/resize work does not stack up behind an in-flight page load
- Made Servo frame capture skip an explicit `webview.paint()` when the WebView delegate has already painted a newly-ready frame, avoiding one redundant render step on fresh frames
- Added visible smoke PERF summary output so first-draw/frame checks report navigation, distill, Wake, resize, and frame timings alongside draw/blit/present costs
- Captured fresh heavy-page performance probes: Google loaded with Servo in about 2.1s with frame resize/capture around 10ms/27ms, while MDN loaded in about 1.1s with frame resize/capture around 31ms/17ms on the latest debug run
- Confirmed visible Google smoke now reports an immediate `Opening ... with Servo...` status, first shell draw around 29ms, first Servo frame around 2.3s, and navigation work around 2.1s without doing that navigation work on the UI thread
- Tested early viewport pre-sizing before navigation and rejected it for now: it removed the post-load resize cost but made Google navigation jump to about 8.1s, so the live path keeps the faster default navigation shape while deeper Servo layout instrumentation is planned
- Kept synchronous frame refresh available for operator/proof workflows that require immediate captured evidence
- Split frame capture by purpose: user-visible render bridge captures stay available in every browser mode, while AI observation/proof captures are capability-gated and blocked in Direct/Incognito
- Added perf labels that distinguish render-bridge frame capture from AI-observation frame capture
- Updated tab switching and close-tab refreshes to use the render-bridge path, preserving user display in Direct/Incognito while AI observation remains excluded
- Extended MCP browser discovery so `browser_capabilities`, initialize metadata, and static resources advertise Agent/Assisted/Observe/Direct/Incognito mode boundaries
- Added structured MCP runtime-loop metadata for the user interaction loop, Servo service loop, temporary render bridge, AI observation path, and persistence path
- Added `--browser-mode <agent|assisted|observe|direct|incognito>` for visible browser launches and window-smoke runs
- Exposed visible mode selection through MCP `browser_window_smoke.mode`
- Proved Direct-mode visible smoke against `https://example.com`: AI observation disabled, temporary render bridge still captured and displayed a real Servo frame
- Made Incognito visible launches and runtime mode switches use an ephemeral browser data directory for Wake/Log storage instead of the normal browser profile
- Leaving Incognito now reopens the normal browser profile storage and removes the temp directory
- Proved Incognito visible smoke against `https://example.com`: ephemeral profile reported, real Servo frame displayed, temp directory removed after bounded smoke exit
- Limited native `intent:` execution to Agent and Assisted modes so Observe, Direct, and Incognito stop Pilot/browser-control work at the mode boundary
- Proved Direct-mode visible intent blocking with `intent: open https://example.com and distill`, which failed intentionally before navigation/distillation with a clear mode-boundary message
- Limited visible proof workflows to Agent and Assisted modes so Showcase, real-browsing seed, and shell-interaction seed cannot bypass Direct/Incognito boundaries
- Proved Direct-mode showcase blocking with `--start-showcase --window-smoke`, which failed intentionally before proof automation began
- Added MCP-side prevalidation for `browser_window_smoke.mode` so Direct/Observe/Incognito reject proof seeders and native-intent targets before launching the browser
- Proved MCP Direct-mode showcase rejection over stdio returned an immediate tool error instead of spawning `sextant-browser`
- Added mode-aware disabled action messages for Showcase, Distill, and Wake controls
- Updated validation rows to mark AI observation, Wake, and Captain's Log as mode-disabled when Direct/Incognito/Observe boundaries apply
- Added AI posture and profile/ephemeral storage state to the visible status bar
- Added structured `windowSmoke` output to MCP browser window smoke results, including mode, storage, timing, perf, and latest-frame fields
- Proved Incognito MCP window smoke returns structured mode/storage/frame data and still removes its ephemeral profile after exit
- Added `sextant-mcp --window-smoke` as a CLI shim for the MCP visible-smoke path, with `--mode`/`--browser-mode`, browser workflow flags, and `--json`
- Added parser coverage for MCP window-smoke CLI mode, timeout, target, and proof-workflow arguments
- Added an async distillation worker/apply path in `sextant-engine` so visible browser DOM distillation can run off the UI event loop
- Routed deferred user DISTILL actions in the visible shell through async distillation polling while keeping operator/proof workflows synchronous for deterministic assertions
- Added regression coverage proving visible distillation can complete from the worker and attach the distilled page back to the active tab
- Added visible smoke support for user-triggered async DISTILL with `--window-smoke-distill` / MCP `user_distill`, including structured `distillMs` and `wakeMs`
- Proved assisted user-DISTILL visible smoke against `https://example.com`: `distillMs=381`, `wakeMs=30`, first draw around 33ms, first Servo frame around 974ms
- Split render-bridge resize and frame-capture timing so `frameMs` no longer includes resize time and async worker round-trip time
- Added a combined Servo service `resize + capture` command for the render bridge, removing the extra resize-then-capture service request pair
- Re-ran assisted user-DISTILL visible smoke after the split: first draw around 22ms, first Servo frame around 874ms, resize around 18ms, frame readback around 36ms
- Added the first dedicated Persistence lane for visible async DISTILL completion. It owns Wake/Log handles on a worker thread, records the distilled page, searches Wake, writes the Captain's Log entry, and returns results to the UI loop.
- Persistence lane shutdown now joins the worker before profile switches, preserving Incognito temp-profile cleanup.
- Proved assisted user-DISTILL visible smoke through the Persistence lane: distill around 26ms, Wake/Log persistence around 46ms, resize around 22ms, frame readback around 27ms, first Servo frame around 895ms.
- Extended the Persistence lane to visible Wake searches, including Wake query, Captain's Log search entry, recent-log refresh, and UI-loop polling.
- Added regression coverage proving visible Wake search can complete from the Persistence lane and return both Wake rows and Log rows.
- Routed ordinary visible Captain's Log writes through the Persistence lane as fire-and-poll work, so visible navigation/tab/control logs no longer need to write SQLite on the UI path.
- Added regression coverage proving visible log writes complete through the Persistence lane and refresh recent Log rows.
- Moved visible recent-log refreshes onto the Persistence lane too, so the visible shell no longer needs to perform synchronous Captain's Log reads after navigation, mode changes, or UI actions.
- Added regression coverage proving visible recent-log refresh can complete from the Persistence lane.
- Added viewport-input scheduling inside the Servo service: queued key/mouse/wheel input can run ahead of pending frame captures, while navigation/inspection/control commands preserve order.
- Added Servo-service mouse-move coalescing so queued hover movement collapses to the latest position across deferred frame captures without crossing click, wheel, or key boundaries.
- Added Servo-feature regression coverage for input-over-frame scheduling and mouse-move coalescing.
- Re-ran assisted user-DISTILL visible smoke against `https://example.com` after the lane/scheduler work: first shell draw around 35ms, navigation around 406ms, distill around 310ms, Wake/persistence around 50ms, resize around 20ms, frame readback around 24ms, and first Servo frame around 862ms.
- Added a shell-side viewport input lane so visible mouse moves, wheel deltas, and printable text are queued/coalesced before engine enqueue instead of being sent directly from high-frequency winit handlers.
- Added regression coverage proving the shell-side viewport input lane coalesces mouse moves, adjacent wheel deltas, and text batches before engine enqueue.
- Pinned the shell-side viewport input lane to the active tab and clear it on tab changes, preventing queued input from landing on a different tab if the user switches pages before the next lane flush.
- Added regression coverage proving stale queued viewport input is dropped when the active tab changes before flush.
- Moved visible tab-switch and close-tab render refresh onto the async render-bridge capture path, removing another synchronous Servo resize/capture call from user tab controls while preserving synchronous operator/proof refreshes.
- Added async Servo navigation-control workers for visible reload, back, and forward actions, so those deferred user controls no longer run synchronous Servo navigation on the UI loop after the loading notice paints.
- Added regression coverage for the visible navigation-control guard that prevents reload/back/forward async workers from starting on blank tabs.
- Added Servo-backed regression coverage proving visible reload can complete through the async navigation-control worker and pending-navigation polling path.
- Updated MCP browser capability metadata and static resources to advertise the implemented viewport input lane.
- Re-ran assisted user-DISTILL visible smoke after the shell input lane landed: first shell draw around 25ms, navigation around 407ms, distill around 346ms, Wake/persistence around 48ms, resize around 30ms, frame readback around 51ms, and first Servo frame around 918ms.
- Re-ran assisted user-DISTILL visible smoke after async tab-control refreshes: first shell draw around 38ms, navigation around 488ms, distill around 448ms, Wake/persistence around 90ms, resize around 27ms, frame readback around 36ms, and first Servo frame around 1.1s.
- Re-ran assisted user-DISTILL visible smoke after async reload/back/forward workers: first shell draw around 19ms, navigation around 477ms, distill around 370ms, Wake/persistence around 58ms, resize around 31ms, frame readback around 33ms, and first Servo frame around 1.0s.

---

## 2026-05-17 — Native Captain's Key Consent Surface

- Added first-class pending consent state to the active native browser shell
- Added Context Vault rail AUTHORIZE/DENY controls for sensitive Pilot action requests
- Wired consent denial to clear pending state and record an aborted Captain's Log audit entry
- Wired consent authorization to sign the pending request through the browser's Captain's Key vault and record the consent signature in Captain's Log
- Added resumable browser-owned Pilot continuations behind pending consent, so AUTHORIZE resumes the gated navigation/distill/perception plan instead of only closing the consent gate
- Added `--consent-run "<sensitive intent>"` and MCP `browser_authorized_intent_run` for headless Captain's Key authorize/resume coverage
- Kept the lighter `--no-default-features` lane working with a deterministic fallback consent signature
- Added focused tests for deny, authorize, and visible consent button hit regions

---

## 2026-05-16 — Pilot Action Intent Bridge

- Revisited the original architecture and restored a deeper slice of the planned Intent -> Pilot -> Engine -> Wake loop in the active native browser shell
- Added a default-feature Pilot action lane for explicit browser intents using `sextant-pilot::PilotAction`
- Mapped safe explicit intents to `Navigate`, optional `Distill`, and `Analyze` actions against the live browser engine, Wake, and Captain's Log state
- Mapped sensitive intents to `RequestConsent` so they stop at `AWAITING CONSENT` instead of running through the shell fallback path
- Kept the deterministic native intent runner as the `--no-default-features` fallback lane
- Updated the `--intent-run` verifier to accept the new `pilot intent` audit entries while remaining compatible with fallback `native intent` entries
- Added focused regression coverage for safe Pilot action planning and sensitive consent planning
- Confirmed `--intent-run "intent: open https://example.com and distill" --expect "Example Domain"` passes through the Pilot action lane with Servo frame capture

---

## 2026-05-14 — Real Browsing Smoke Suite

- Added a user-facing page-tab strip to the native browser shell so each engine tab has visible space and can be clicked to switch tabs
- Stabilized visible tab order so switching tabs no longer reorders the page-tab strip
- Added page-tab overflow paging controls and mouse-wheel paging so hidden older tabs stay reachable after opening more tabs than fit in the strip
- Made active-tab close fallback select the adjacent tab in stable strip order instead of an arbitrary map entry
- Made the page-tab overflow range label layout-aware so it is skipped instead of overlapping visible tabs or the pager on tight widths
- Added browser chrome keyboard shortcuts for normal browsing: Ctrl+L, Ctrl+T, Ctrl+W, Ctrl+R, Alt+Left, and Alt+Right
- Added focused regressions for stable tab order, adjacent close fallback, active overflow visibility, hidden-tab pager reachability, and range-label overlap
- Extended visible shell-interaction smoke to create a second tab, navigate it, switch between page tabs through the tab strip, close back to the first tab, overflow the tab strip, and page back to the hidden first tab through visible controls
- Added `sextant-browser --real-browsing-smoke` as a broader normal-browsing hardening path covering example.com, Google search form fill/submit, MDN distillation, reload, IANA navigation, back/forward, Wake, Captain's Log, and Servo frame capture
- Exposed the suite through MCP as `browser_real_browsing_smoke` and CLI `sextant-mcp --real-browsing-smoke --timeout-seconds 240`
- Fixed stale Servo navigation success by retrying requested navigation once and failing if the WebView remains on the previous URL
- Added `--start-real-browsing` so the visible browser can seed into the same proof state, including bounded `--start-real-browsing --window-smoke --window-smoke-timeout 60`
- Updated MCP `browser_window_smoke` with `real_browsing: true` for the visible real-browsing proof
- Improved MCP report extraction so bracketed browser failure lines emitted on stderr appear in tool/preflight summaries
- Added `--start-shell-interaction` to click through visible chrome controls for address/run, distill, Wake, view tabs, viewport focus, new tab, and close tab before drawing
- Updated MCP `browser_window_smoke` with `shell_interaction: true` for the visible chrome-click proof
- Added `sextant-mcp --hardening-preflight --timeout-seconds 240 --visible-timeout-seconds 60` and MCP `browser_hardening_preflight`
- Added total and per-check timing telemetry to launch and hardening preflight summaries and structured output
- QA pass caught a visible real-browsing flake where a delayed Google interstitial redirect could contaminate the following MDN navigation; the workflow now isolates post-Google documentation/history checks in a fresh tab
- Improved MCP preflight failure summaries so browser-side `[sextant-browser] failed: ...` lines are preserved and preferred over generic startup breadcrumbs
- Confirmed `cargo run -p sextant-mcp -- --real-browsing-smoke --timeout-seconds 240` passes
- Confirmed visible real-browsing smoke passes directly and through MCP with a captured Servo frame
- Confirmed visible shell-interaction smoke passes directly and through MCP with a captured Servo frame
- Confirmed hardening preflight passes operator smoke, Intent Bar, showcase, real browsing, visible showcase, visible real browsing, and visible shell-interaction checks

---

## 2026-05-14 — Launch Showcase Path

- Added `sextant-browser --showcase-run` as a bounded launch-demo proof across Intent Bar, Servo navigation, live DOM distillation, Wake, Captain's Log, tab creation, native form fill/click, validation progress, and frame capture
- Added a visible `SHOWCASE` toolbar control that runs the same launch-demo workflow in-window and shows passed-step evidence in the Validation tab
- Added `sextant-browser --demo` for one-command visible launch into the showcase proof state
- Added `--start-showcase` for visible launch/showcase seeding, including bounded `--start-showcase --window-smoke` checks
- Added visible browser launch seeding with `--start "<url-search-or-intent>"`
- Updated MCP browser command resolution to prefer the current Cargo workspace during development, avoiding stale sibling binaries for demo checks
- Added `sextant-mcp --launch-preflight --timeout-seconds 60` and MCP `browser_launch_preflight` for the launch-critical pre-demo sequence
- Confirmed launch preflight passes operator smoke, Intent Bar, showcase, and visible showcase smoke checks
- Made launch preflight print a short human summary by default, with `--json` available for structured reports
- Updated `--window-smoke` so it can run URL/search targets or native Intent Bar commands before drawing and exiting
- Exposed the showcase proof through MCP as `browser_showcase_run`
- Confirmed `--showcase-run` passes and `--window-smoke "intent: open https://example.com and distill"` captures a Servo frame

---

## 2026-05-14 — Xilem Smoke Ladder And MCP Intent Tool

- Added bounded parked-Xilem launch modes: `--xilem-smoke minimal|safe|interactive-smoke|full --xilem-timeout <seconds>`
- Added `--xilem-modes` discovery and `xilem-diagnostics.log` breadcrumbs for future Vello/Masonry crash isolation
- Confirmed minimal, safe, interactive-smoke, and full Xilem shells survive first paint for five seconds on Windows
- Added MCP `browser_intent_run` so Codex/Claude can drive the native Intent Bar loop through `sextant-browser --intent-run`
- Updated MCP docs/resources to advertise the Intent Bar workflow alongside operator probe/run/window smoke tools

---

## 2026-05-14 — Native Intent Bar Loop

- Reintroduced the original Intent Bar/Context Vault direction inside the active `sextant-browser` shell
- Kept normal URL/search behavior intact while adding explicit native intents such as `intent: open https://example.com and distill`
- Added deterministic shell-level intent planning that resolves a target, navigates, distills into Wake when requested, records Captain's Log, and updates Pilot status/result text in the right rail
- Added `--intent-run "<intent>" --expect "<text>"` for headless regression coverage of the Intent Bar loop through Servo navigation, live DOM distillation, Wake, Captain's Log, and frame capture
- Confirmed `--intent-run "intent: open https://example.com and distill" --expect "Example Domain"` passes with a Servo frame capture

---

## 2026-05-14 — MCP And Visible Shell Hardening

- Hardened `sextant-mcp` initialization and compatibility responses with stable protocol reporting, `ping`, empty prompt lists, and structured launch failures
- Added `--window-smoke [target] --window-smoke-timeout <seconds>` to `sextant-browser` for bounded visible shell launch/draw/navigation validation
- Exposed the visible shell check through MCP as `browser_window_smoke`
- Confirmed MCP `browser_operator_run` passes against `https://example.com`
- Confirmed `--window-smoke` passes for chrome-only startup, default-feature Servo navigation to `https://example.com`, and reader/fallback navigation to `https://example.com`
- Confirmed MCP `browser_window_smoke` passes against the default Servo binary and reports a captured frame

---

## 2026-05-14 — Local MCP Browser Layer

- Added `sextant-mcp`, a local stdio MCP server for Codex/Claude integration
- Implemented MCP `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`, and empty `resources/templates/list` handling over newline-delimited JSON-RPC
- Exposed truthful browser tools for capability discovery, native operator smoke/probe/run, and recent Captain's Log reads
- Added static `sextant://browser/...` resources that describe current browser capabilities, operator workflow, and MCP tool scope
- Routed operator tools through the canonical `sextant-browser` binary when available, with a Cargo fallback for development
- Audited MCP tool calls to Captain's Log with the `sextant-mcp-stdio` signature
- Confirmed MCP initialize/tool/resource round trips, native operator smoke through MCP, and `captains_log_recent` reads pass

---

## 2026-05-14 — Native Browser Naming

- Added `sextant-browser` as the canonical binary for the owned `winit` + `softbuffer` Servo browser shell
- Kept `sextant-hull-lite` as a compatibility alias for older scripts while new work moves to `sextant-browser`
- Updated the visible window title, operator status text, validation heading, log signature, and active workflow docs away from the old "lite" naming
- Marked the reader path as a fallback build mode of `sextant-browser --no-default-features`, not the identity of the product lane

---

## 2026-05-14 — Google Search Browser Test

- Ran a real native browser Google flow: loaded `https://www.google.com`, filled `textarea[name=q]`, submitted the form, and reached a Google Search results URL for `Sextant native browser test`
- Hardened operator expectations to include decoded URL query pairs, so scripted checks can verify search/navigation state even when a site does not echo the query as plain page text
- Confirmed the Google scripted run distills, updates Wake and Captain's Log, captures a Servo frame, and passes `--expect "Sextant native browser test"`
- Confirmed the reader/fallback browser build still checks after the shared expectation matcher change

---

## 2026-05-14 — Native Browser Validation Surface

- Added a first-party `VALIDATION` tab to `sextant-browser`
- Added session-aware validation tracking for navigation, viewport/frame capture, real forwarded browser input, distillation, Wake results, Captain's Log rows, tab controls, and visible error surfacing
- Added validation progress to the native browser status bar so manual click-through coverage is visible while working
- Added `RESET CHECKS` so a fresh in-window validation pass can start without restarting the app or clearing runtime state
- Confirmed default-feature and reader/fallback browser checks pass
- Confirmed `--operator-timeout 45 --operator-smoke` still passes after the validation UI changes

---

## 2026-05-14 — Operator Timeout Guard

- Added a shared bounded worker for native browser `--operator-smoke`, `--operator-probe`, and `--operator-run`
- Added `--operator-timeout <seconds>` with a 120-second default and timeout exit code `124`
- Confirmed bounded operator smoke and scripted data-URL runs pass with `--operator-timeout 30`
- Confirmed invalid timeout values exit cleanly with code `2`
- Confirmed a deliberately stalled script exits with code `124` instead of hanging indefinitely

---

## 2026-05-13 — Stability And Polish Pass

- Reworked the visible browser startup path so event-loop, window, Softbuffer, and app initialization failures report clean `[sextant-browser] failed: ...` errors instead of panicking
- Reconnected the Xilem full-shell Wake/Audit and Validation panels so the parked reference hull keeps its richer diagnostic surface when `SEXTANT_FULL_SHELL` is enabled
- Marked parked provider-setting mutators as intentionally retained until the native editor surface uses them
- Confirmed `cargo check --workspace --manifest-path rust/Cargo.toml` passes with only the known `xml5ever v0.16.2` future-incompatibility notice
- Confirmed focused native browser checking and direct binary operator runs still pass for `--operator-smoke`, data-URL `--operator-run`, data-URL `--operator-probe`, and `https://example.com --expect "Example Domain"`

---

## 2026-05-13 — Native Operator Bridge

- Added `sextant-hull-lite --operator-smoke` for deterministic native-lite automation without opening the visible event loop
- Smoke mode validates Servo navigation, native DOM fill/click, live DOM distillation, Wake recording/search, Captain's Log writes, and Servo frame capture
- Added `sextant-hull-lite --operator-probe <url-or-search>` to aim the same native-lite workflow at representative HTTP/HTTPS pages
- Added `sextant-hull-lite --operator-run <url-or-search>` with `--fill`, `--click`, `--submit`, and `--expect` scripted steps
- Confirmed `--operator-smoke` passes against a data URL with a live input/button interaction
- Confirmed `--operator-run` can fill and click a scripted data-URL form, verify distilled content, update Wake/Log, and capture a Servo frame
- Confirmed `--operator-probe https://example.com` passes, distills `Example Domain`, writes Wake/Log state, and captures a Servo frame
- Found and fixed a real probe blocker on `https://www.rust-lang.org`: Servo distillation timeout no longer marks the whole service failed, and distillation can fall back to the reader path
- Confirmed `--operator-probe https://www.rust-lang.org` now passes, distills `Rust Programming Language`, writes Wake/Log state, and captures a Servo frame
- Found and fixed form-state observability: live DOM distillation now includes current `input`, `textarea`, and `select` values in the semantic map
- Confirmed `--operator-run https://httpbin.org/forms/post --fill "input[name=custname]" "Sextant Operator" --expect "Sextant Operator"` now passes
- Found and fixed click-driven navigation drift: native browser interactions now return the current Servo URL and update the engine tab after click/submit navigation
- Confirmed `--operator-run https://example.com --click "a" --expect "IANA"` now passes after navigating away from the original page
- Improved distillation quality for harder sites by adding paragraph/list/code-style text nodes to live DOM and reader semantic maps
- Added weak-live-DOM quality fallback so low-structure HTTP/HTTPS snapshots can switch to the richer reader distiller
- Confirmed higher-difficulty probes pass for Wikipedia, MDN, GitHub, docs.rs, rust-lang.org, `neverssl`, `example.com`, and the `httpbin` form workflow

---

## 2026-05-13 — Heavy Browsing Reality Check

- Verified the code still contains the Servo-backed heavy browsing path in `sextant-engine` and `sextant-hull-lite`
- Confirmed `cargo test -p sextant-engine --lib --features servo-backend` passes 9 Servo-focused tests
- Confirmed the lite shell has address input, tab controls, browser/Wake/Log views, Servo frame painting, and viewport input forwarding
- Updated active docs so the default-feature `sextant-hull-lite` lane is documented as the heavy browsing path and `--no-default-features` is documented as the reader/fallback lane
- Captured that real-window Servo browsing is still buggy and needs manual hardening rather than being treated as future/unstarted work

---

## 2026-05-01 — Native-Lite Shell Pivot

- ✅ Added `sextant-hull-lite`, a first-party `winit` + `softbuffer` shell that avoids the crashing Xilem/Masonry widget lifecycle
- ✅ Wired the first native-lite smoke actions into `SextantEngine`, Digital Wake, and Captain's Log
- ✅ Made the hull's Servo backend an explicit feature so the lite shell can build with `--no-default-features`
- ✅ Confirmed `cargo check -p sextant-hull --bin sextant-hull-lite --no-default-features` passes
- ✅ Confirmed `cargo build -p sextant-hull --bin sextant-hull-lite --no-default-features` passes
- ✅ Launched `sextant-hull-lite.exe` and verified it stayed responsive in a timed Windows smoke run

---

## 2026-05-01 — Bottom-Up Stability Pass

- ✅ Hardened Captain's Log row loading so corrupt persisted UUID/timestamp/status data returns an error instead of panicking
- ✅ Added bio, privacy, firewall, bridge, sync, inference, Wake, and engine regression coverage around low-level failure modes
- ✅ Made Neural Bridge perception tolerate poisoned privacy locks by falling back to standard privacy
- ✅ Fixed standalone `sextant-inference` builds by enabling `url/serde` in that crate directly
- ✅ Made Vault import tolerant of exported identities without persisted secret-key material
- ✅ Fixed `perceive_all_tabs` so failed tab distillation cannot attach another tab's page to the wrong tab
- ✅ Confirmed `cargo check --workspace` and `cargo test --workspace` pass after the lower-stack sweep

---

## 2026-05-01 — Core Browser Guardrail Pass

- ✅ Re-exposed native hull intent, provider, Wake, audit, mesh placeholder, and validation surfaces after the Servo/stability-mode narrowing
- ✅ Restored default blank-tab startup semantics in `SextantEngine`
- ✅ Made newly opened tabs become active so navigation and distillation target the intended tab
- ✅ Normalized Servo blank-page distillation to match the stable `Blank Page` engine semantics
- ✅ Confirmed `cargo check --workspace`, `cargo test --workspace`, focused Pilot/Engine tests, and a timed hull binary launch all pass

---

## 2026-04-19 — Validation Transcript And Activity Summary Pass

- ✅ Added fresh-session audit validation so Captain's Log coverage only completes after a new post-reset audit entry
- ✅ Added top-level validation focus and prioritized remaining-check ordering
- ✅ Added row-level completion badges across the checklist
- ✅ Tagged recent validation transcript entries by workflow and added grouped latest-activity summaries
- ✅ Kept `cargo check -p sextant-hull` green through the batch

---

## 2026-04-19 — Validation Panel Guidance And Transcript Pass

- ✅ Added top-level validation coverage rollups and per-workflow totals in the native hull
- ✅ Prioritized remaining checks into blockers, active flow, workflow gaps, and evidence gaps
- ✅ Added row-level completion badges across provider, consent, tab, Wake, control, and audit checks
- ✅ Structured the recent validation trail with workflow tags and added grouped workflow activity summaries
- ✅ Kept `cargo check -p sextant-hull` green through the batch

---

## 2026-04-19 — Native Hull Validation Workflow Pass

- ✅ Added a native in-window validation checklist panel for operator-guided testing
- ✅ Added remaining-checks guidance and recent validation-trail visibility
- ✅ Made checklist coverage session-aware across provider, consent, tab, Wake, and air-gap/privacy workflows
- ✅ Added `RESET VALIDATION` so a fresh click-through pass can start without resetting real runtime state
- ✅ Kept `cargo check -p sextant-hull` green through the batch

---

## 2026-04-19 — Runtime Sanity And Next-Work Refresh

- ✅ Re-ran `cargo check --workspace` successfully
- ✅ Re-ran Pilot regression coverage successfully
- ✅ Re-exercised `cargo run -p sextant-hull` as a launch sanity check
- ✅ Refreshed `next-work.md` to reflect the current native hull reality instead of earlier pending scaffolding tasks
- ✅ Improved native-hull truthful UX: startup alerts, runtime status, clearer provider save/load/apply flow, empty states, and explicit mesh placeholder messaging

---

## 2026-04-19 — CI Bootstrap

Full details in [archive/2026-04-19-ci-bootstrap.md](archive/2026-04-19-ci-bootstrap.md).

- ✅ Added first Gitea workflow for the Rust workspace
- ✅ CI now runs `cargo check --workspace`
- ✅ CI now runs `cargo test --workspace`
- ✅ Workflow triggers on push, pull request, and manual dispatch

---

## 2026-04-19 — Docs Structure Refresh

Full details in [archive/2026-04-19-docs-structure-refresh.md](archive/2026-04-19-docs-structure-refresh.md).

- ✅ README upgraded into a real docs and onboarding hub
- ✅ `current-state.md` refreshed to match the actual native hull state
- ✅ `source-map.md` added to explain where real product work belongs
- ✅ `operator-workflow.md` added to document the normal development loop
- ✅ docs spine now better matches the stronger template used in `wsky-ai-ops-docs`

---

## 2026-04-18 — Native Hull Workflow Tightened

Full details in [archive/2026-04-18-native-hull-progress.md](archive/2026-04-18-native-hull-progress.md).

- ✅ Hull split into focused modules (`app_core`, `state`, `views`, `poller`, `util`, `deferred`)
- ✅ Toolkit-agnostic async command/event runtime established in the native hull
- ✅ AI/settings panel now supports real provider selection, apply/test/save/load, and readiness feedback
- ✅ Captain's Log and system log are now clearly separated in the UI
- ✅ Command preflight and consent UX now fail loudly instead of silently
- ✅ Pilot fixed so `OpenTab` actually navigates the newly opened tab
- ✅ Wake hybrid search fixed for qualified FTS columns and punctuation-safe query normalization
- ✅ Engine perception now updates tab state with distilled pages
- ✅ Pilot regression coverage added for:
  - new-tab navigation
  - navigate -> distill -> Wake record
  - consent deny flow
  - consent authorize/resume flow
- ✅ Small warning cleanup completed in `sextant-airgap`, `sextant-sync`, `sextant-vault`, and `sextant-wake`

---

## 2026-04-18 — Workspace Compiles Clean

Full details in [archive/2026-04-18-workspace-compiles.md](archive/2026-04-18-workspace-compiles.md).

- ✅ All 15 crates compile — `cargo check` passes with zero errors
- ✅ `.cargo/config.toml` created — MSVC linker + INCLUDE/LIB paths persisted
- ✅ Servo made optional (`servo-backend` feature) — removes libclang/bindgen requirement
- ✅ `sextant-hull` rewritten to Xilem 0.1.0 API — `Xilem`, `MasonryView`, `Axis`, `Color::rgba`
- ✅ `sextant-vault` fixed — bip39 v2, HmacSha512 module-level, Zeroize skip, decrypt return
- ✅ `sextant-engine` fixed — wgpu `required_features`/`required_limits`, PrivacyLevel import
- ✅ `sextant-wake` fixed — rusqlite 0.37, FTS5 bundled, f32 type annotations
- ✅ `sextant-log` fixed — rusqlite 0.37
- ✅ `sextant-inference` fixed — serde_json added, `collect::<Vec<f32>>()`
- ✅ `sextant-pilot` fixed — IntentType removed, chrono added, borrow/match/arity fixes
- ✅ `sextant-mesh` fixed — wireguard-uapi (Linux-only) removed
- ✅ `sextant-bridge` fixed — type inference on `Option<String>`
- ✅ 15 `Cargo.toml` files updated — uuid serde feature, rusqlite versions, missing deps

---

## Prior Sessions (pre-docs)

The project was scaffolded prior to the 2026-04-18 session. The 15-crate workspace, TypeScript simulator, and architecture design existed before systematic error fixing began.
