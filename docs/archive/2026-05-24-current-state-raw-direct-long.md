# Current State

Date of this snapshot: `2026-05-24`

## How To Read This

Use this together with:

- [next-work.md](next-work.md) for the active execution order
- [architecture.md](architecture.md) for the stable system model
- [development-guardrails.md](development-guardrails.md) for project rules
- [completed.md](completed.md) for dated history

## Executive Summary

Sextant has a stable core Rust foundation and an owned native browser product path. The previous Xilem/Masonry hull remains useful as reference code, but real Windows click testing exposed native access violations in that toolkit path, so active UI work is now on the first-party `winit` + `softbuffer` shell.

The code is further along than the older May 1 wording suggested. There are now two native browser lanes:

- **Heavy browsing lane:** `sextant-browser` with package default features, which enables Servo live navigation, Servo frame capture, browser viewport input forwarding, back/forward/reload, and live DOM distillation.
- **Reader/fallback lane:** `sextant-browser --no-default-features`, which avoids Servo/Xilem and keeps the direct shell, fetch/distill, Wake, and Captain's Log workflow available for fast smoke checks.

There is also a new experimental direct Servo presentation proof: `sextant-servo-direct`. It opens a Servo `WindowRenderingContext`, loads a URL, paints directly through Servo's rendering context, and presents without the current frame-capture-to-Softbuffer bridge. The first smoke against `https://example.com` reached first direct present in about 165ms after the Windows ANGLE runtime bootstrap was shared with the proof binary, and MCP now exposes that same path as `browser_direct_present_smoke`.

The product is no longer just a compile-clean scaffold. The core crates can:

- route commands through async app commands/events
- persist Wake and Captain's Log state
- run intent/consent regression coverage in the Pilot
- fetch, render, distill, and map browser tab state through the engine

The active `sextant-browser` binary is not just a smoke window. It has a direct-drawn UI with address input, keyboard focus, hit-tested controls, browser/Wake/Log/Guard/Sense/Perf/Validation views, Wake search, Captain's Log display, tab controls, and a Servo frame viewport when built with default features.

`sextant-browser` is now the canonical native product binary. The old `sextant-hull-lite` bin remains as a compatibility alias during the transition, but new docs, scripts, and validation work should use `sextant-browser`.

The original Intent Bar/Context Vault shape is returning in the active shell. The address field now accepts normal URLs/searches plus explicit native intents such as `intent: open https://example.com and distill`; the native intent loop plans, navigates, distills into Wake, records Captain's Log entries, and shows Pilot state in the right rail. This is a deterministic bridge back toward the full `sextant-pilot` orchestration layer while Servo hardening continues.

The biggest remaining gaps are now replacing the temporary frame-capture render bridge with a first-class Servo compositor path, heavy browsing hardening, real-window Servo bug fixing, richer validation inside the browser shell, provider-path hardening, CI depth, and dependency cleanup.

## What Is Working

| Capability | State | Notes |
|------------|-------|-------|
| Rust workspace compile | ✅ | `cargo check` passes |
| Workspace warning budget | ✅ | local crate warnings are clear in the latest workspace check; the remaining notice is the known `xml5ever v0.16.2` future-incompatibility warning |
| Rust workspace tests | ✅ | `cargo test --workspace` passes |
| Pilot regression tests | ✅ | `cargo test -p sextant-pilot --lib` passes |
| Native browser reader lane | ✅ | `sextant-browser --no-default-features` builds and stays responsive in a timed real launch check |
| Native browser heavy browsing build | ✅ | package default features wire `sextant-browser` to Servo; focused Servo engine tests pass |
| Native operator bridge | ✅ baseline | `--operator-smoke` drives Servo navigation, native DOM fill/click, distillation, Wake, Log, and frame capture; `--operator-probe <url>` probes real pages and now reports navigation/distill/Wake/viewport-resize/frame timings; `--guard-probe <url>` reports the local trust boundary; `--perception-probe <url>` reports semantic page perception; `--perf-probe <url>` and `--perf-baseline` capture browser performance baselines; `--operator-run` scripts selector actions and expectations; `--operator-timeout <seconds>` bounds stalled runs |
| Native intent runner | ✅ baseline | `--intent-run "intent: open https://example.com and distill" --expect "Example Domain"` exercises the Intent Bar loop headlessly through the Pilot action lane; `--consent-run "intent: buy https://example.com and checkout" --expect "Example Domain"` authorizes pending Captain's Key consent and resumes the bounded browser plan |
| Launch showcase runner | ✅ baseline | `--showcase-run` proves Intent Bar, Servo navigation, live DOM distillation, Wake, Captain's Log, tab creation, native form interaction, validation progress, and frame capture in one bounded command |
| Launch preflight | ✅ baseline | `sextant-mcp --launch-preflight --timeout-seconds 60` runs operator smoke, Intent Bar, showcase, and visible showcase smoke checks in sequence |
| Hardening preflight | ✅ baseline | `sextant-mcp --hardening-preflight --timeout-seconds 240 --visible-timeout-seconds 60` runs launch checks plus real browsing, visible real browsing, and visible shell-interaction checks |
| Real browsing smoke suite | ✅ baseline | `sextant-mcp --real-browsing-smoke --timeout-seconds 240` covers example.com, real Google search form fill/submit, MDN distillation, reload, IANA navigation, back/forward, Wake, Captain's Log, and Servo frame capture |
| MCP browser advertising layer | ✅ baseline | `sextant-mcp` exposes local stdio MCP tools/resources for browser capabilities, browser modes/runtime-loop boundaries, direct-present smoke, render-path baseline, bounded operator smoke/probe/run, guard probe, guard policy read/write, perception probe, perf probe/baseline, and recent Captain's Log reads |
| Xilem/Vello diagnostic ladder | ✅ baseline | parked `sextant-hull` now has bounded `--xilem-smoke` modes for minimal, safe, interactive-smoke, and full shells; all four survived five-second first-paint checks on Windows |
| Servo live navigation | ✅ baseline / ⚠️ buggy | tests cover data URLs, live DOM mutation, history, back/forward, and cache invalidation; real browsing smoke covers `example.com`, Google search, MDN, IANA, reload, and back/forward; broader real-window browsing still needs hardening |
| Servo frame viewport | ✅ baseline / ⚠️ buggy | browser shell captures `RenderedFrame` pixels from Servo and paints them into the `softbuffer` viewport |
| Direct Servo presentation proof | ✅ experimental | `sextant-servo-direct` uses Servo `WindowRenderingContext` and direct `present()`; `https://example.com` smoke reached first direct present in about 151-165ms, and MCP `browser_direct_present_smoke` returns structured `directPresent.firstPresentMs` timing |
| Production raw direct render lane | ✅ experimental | `sextant-browser --render-path direct` now routes through the shared Servo `WindowRenderingContext` runner for Direct/Incognito raw URL/search browsing, direct WebView input, title-bar location mode, native title/history status, reload/back/forward shortcuts, keyboard-only direct tabs, first-interaction timing, direct load-complete timing, and direct resize coverage; MCP smokes reported `directPresentMs` as low as 115ms on `https://example.com`, example.com direct load complete around 591ms, Google direct load complete around 5.1s, a warmed Google first-interaction frame around 6ms before load complete, a direct tab follow-up frame around 32ms, a load-complete-gated direct input follow-up frame around 1ms, a direct location follow-up frame around 464ms, direct history back/forward frames around 31ms, a direct reload frame around 133ms, a direct resize frame around 27ms, and an Incognito direct present around 150ms with ephemeral Servo config storage and HTTP cache disabled, while shell chrome/tabs/AI workflows still use the bridge path |
| Production render-path boundary | ✅ baseline | `sextant-browser` now distinguishes engine backend from user render path; the active chrome/tabs/AI production path is still `BRIDGE`, Direct/Incognito report the direct-compositor gap when using the bridge, and MCP can compare direct proof/raw lane vs bridge smoke timings |
| Browser input forwarding | ✅ baseline / ⚠️ buggy | mouse move/click, wheel, character keys, and named keys are forwarded to the Servo WebView path; scripted selector fill/click works on data URLs, Google search, and `httpbin` form inputs |
| Visible frame refresh responsiveness | ✅ baseline / ⚠️ tuning | wheel input is enqueue-only, passive mouse movement is rate-limited before it reaches Servo, printable character input is press-only, shell-debounced, and coalesced in the Servo service, viewport input uses a debounced one-capture warmup budget, and visible frame refresh runs Servo resize plus frame capture on a worker so expensive layout/capture work does not block the window event loop |
| Visible navigation responsiveness | ✅ baseline / ⚠️ tuning | normal URL/search navigation in the real window starts a Servo navigation worker and keeps the event loop alive while the page loads; operator/proof paths still keep synchronous navigation for deterministic evidence |
| Browser mode boundary | ✅ baseline / ⚠️ tuning | active shell exposes Agent, Assisted, Observe, Direct, and Incognito modes with capability flags for AI control, DOM/frame observation, Wake reads/writes, content logging, MCP exposure, and direct-render intent; frame capture is split between temporary user render bridge and AI observation purposes |
| Small-window viewport layout | ✅ baseline | narrow windows hide the right rail, compute panels from the actual window size, and keep the Servo viewport clipped inside the visible content area |
| Native browser validation surface | ✅ baseline | owned shell now has a `VALIDATION` tab, reset control, and status-bar progress for session-aware manual click-through coverage |
| Native browser guard surface | ✅ baseline | owned shell now has a `GUARD` tab with persona, policy source, rule counts, air-gap, firewall, and privacy redaction status; navigation and scripted interaction paths now stop on local guard blocks; matching `--guard-probe`, `--guard-policy`, MCP `browser_guard_probe`, and MCP guard policy read/write surfaces report and manage allowed/audited/blocked outcomes |
| Native browser perception surface | ✅ baseline | owned shell now has a `SENSE` tab with page perception summary, semantic counts, key nodes, source metadata, and a matching `browser_perception_probe` MCP tool |
| Native browser performance surface | ✅ baseline | owned shell now has a `PERF` tab with latest phase timings, slowest phase callout, and capped recent event history for navigation, distillation, Wake, resize, and frame capture |
| Visible browser smoke check | ✅ baseline | `--window-smoke [target] --window-smoke-timeout <seconds>` launches the real user-facing shell, draws once, optionally navigates or seeds showcase, real-browsing, or shell-interaction proof state, waits for an actual Servo frame when a target/workflow should produce one, reports first draw/frame timing, draw/blit/present costs, and latest PERF timing summary, then exits; shell-interaction smoke now covers visible page-tab switching |
| Xilem/Masonry hull | ⚠️ parked | crashes on Windows during interactive use; retained as reference, not the active product lane |
| Lower-stack guardrails | ✅ | bio, privacy, firewall, bridge, sync, log, inference, Wake, and engine edge cases now have focused coverage |
| Hull modular structure | ✅ | `app_core`, `state`, `views`, `poller`, `util`, `deferred` split in place |
| Async hull command runtime | ✅ | commands/events/snapshots route through toolkit-agnostic app core |
| Persistent Wake storage | ✅ | SQLite file-backed |
| Persistent Captain's Log | ✅ | SQLite file-backed |
| Captain's Log row loading | ✅ | invalid persisted row data returns errors instead of panicking |
| Provider control panel | ✅ | selected/active/tested/readiness state surfaced |
| Provider apply/test/save/load | ✅ | wired through async command path |
| Native validation checklist | ✅ | session-aware checklist, priority ordering, progress rollups, reset control, workflow transcript summary, grouped workflow activity |
| Intent -> Pilot -> Engine -> Wake | ✅ baseline | explicit browser intents now plan through `sextant-pilot` `PilotAction`s in default builds, with the older deterministic shell path retained for `--no-default-features`; full `SextantPilot` vault-signed orchestration is still the next deeper pass |
| Consent gating | ✅ baseline | Pilot action intents pause sensitive work in the browser shell, expose AUTHORIZE/DENY controls in the Context Vault rail, record consent signatures or denial audit entries, and resume the browser-owned gated continuation after authorization; full `SextantPilot` ownership remains next |
| Real page distillation | ✅ baseline | fetch + parse + semantic extraction path exists; Servo live DOM distillation captures current form values, text semantic nodes, can fall back to reader distillation for weak live DOM snapshots, and reuses cached results for repeat distills on unchanged tab URLs |
| Tab create/switch/close | ✅ | wired through hull async path |
| Native page-tab strip | ✅ | tabs keep stable visual order, active/new tabs stay visible, overflow arrows and mouse wheel make hidden tabs reachable |
| Servo feature tests | ✅ | `cargo test -p sextant-engine --lib --features servo-backend` passes 9 tests |
| Default tab startup | ✅ | engine starts with a blank active tab again |
| New-tab targeting | ✅ | new tabs become active so navigation/distillation targets the intended tab |
| Multi-tab perception mapping | ✅ | failed tab distillation no longer reassigns another tab's page to the wrong tab |
| Click-driven navigation sync | ✅ baseline | selector click/submit interactions update the active tab URL before later distillation |

## Regression Coverage In Place

Pilot tests currently cover:

- opening a target in a new tab
- navigate -> distill -> write to Wake
- request consent -> deny
- request consent -> authorize -> resume plan

Engine tests currently cover:

- multi-tab perception updates tab state
- Servo navigation updates active tab state
- Servo navigation preserves history within a tab
- Servo back/forward updates the active tab URL
- Servo distillation supports data URLs
- navigation invalidates cached distilled page content
- live DOM distillation reflects inline and async script mutations

## Current UX State

The old Xilem hull now tells the truth more clearly than earlier passes, but it is parked until the native access violation is understood or made irrelevant:

- stability-mode startup defers heavier engine work until browser use while still bootstrapping a live shell
- browser address controls, intent commands, provider settings, Wake, audit, mesh placeholder, and validation panels are visible again
- command preflight failures are surfaced instead of silently ignored
- consent buttons explain when no consent request is pending
- dashboard shows last command summary and async job count
- dashboard now shows startup alerts and a live runtime status line
- Captain's Log shows persisted audit entries
- System Log is separate from the audit trail
- AI settings show selected vs active provider and readiness/test state
- provider settings panel is implemented in the native hull
- mesh toggle now opens an explicit native placeholder instead of hiding a deferred panel
- empty Wake/tab states now explain what to do next instead of rendering as blank areas
- full-shell Wake/Audit and Validation panels are wired back into the parked Xilem reference path for richer diagnostics under `SEXTANT_FULL_SHELL`
- validation checklist now shows remaining steps, recent successful checks, and per-workflow coverage
- consent, provider, tab, Wake, and control workflows now track session-aware coverage instead of relying only on visible state
- `RESET VALIDATION` clears checklist coverage for a fresh click-through pass without resetting real runtime state
- validation panel now shows overall coverage totals, workflow rollups, a current-focus line, and priority-ordered next steps
- checklist rows now include compact completion badges so detailed rows match top-level progress
- recent validation trail now tags each success by workflow, and the panel shows latest grouped activity for provider, consent, tab, Wake, and controls
- audit validation now requires a fresh Captain's Log entry after reset instead of counting older persisted history as current-session coverage

The active native browser hull is intentionally plain, but it now has real browser-facing pieces:

- owned `winit` event loop
- direct `softbuffer` pixel rendering
- address input and keyboard focus
- browser chrome shortcuts for common navigation/control actions: Ctrl+L, Ctrl+T, Ctrl+W, Ctrl+R, Alt+Left, and Alt+Right
- browser, Wake, Captain's Log, GUARD, SENSE, Perf, and Validation tabs
- visible page-tab strip with stable ordering, clickable tab switching, adjacent close fallback, and overflow paging controls
- native browser `VALIDATION` tab for session-aware manual click-through coverage
- direct controls for RUN, NEW TAB, BACK, FORWARD, RELOAD, CLOSE TAB, DISTILL, and WAKE search
- SHOWCASE control that seeds the visible app into the launch-demo proof state and opens the Validation tab with passed-step evidence
- Intent/URL entry that preserves normal browsing while allowing explicit native intent runs to plan through Pilot actions, navigate, distill, and write Wake/Log state
- Context Vault rail with current Pilot status, intent, plan summary, and result text
- Captain's Key consent controls in the Context Vault rail for pending sensitive Pilot actions
- Servo viewport frame painting when built with default features
- browser viewport mouse, wheel, character-key, and named-key forwarding into the engine
- passive browser viewport mouse movement is rate-limited so Servo is not buried under stale hover commands while clicks still force the final pointer position
- printable browser viewport text input is forwarded only on key press, debounced in the shell, and coalesced in the Servo service, reducing no-op release traffic and fast-typing service-loop churn
- viewport wheel input uses an enqueue-only Servo command and queues frame warmup instead of blocking the wheel handler on Servo acknowledgement or immediate frame capture
- browser viewport mouse/key/wheel input uses a debounced one-capture interaction frame-warmup budget, reducing repeated capture pressure after input bursts
- visible viewport input now has a shell-side lane: mouse moves, wheel deltas, and printable text queue/coalesce before engine enqueue, the queue is pinned to the active tab so stale input is dropped on tab changes, and the Servo service then prioritizes queued viewport input over pending frame captures
- visible event-loop frame refresh starts Servo viewport resize plus frame capture on a background worker and polls for completion, keeping the window loop free while layout/capture work completes; visible tab switch and close-tab refreshes use that async render-bridge path too, and idle refresh now waits for foreground Servo navigation/distillation/warmup work instead of adding avoidable queue pressure
- the Servo service now prioritizes queued viewport input over pending frame captures and pending live-DOM distillation, lets explicit distillation/render work pass queued AI eval warmups, and coalesces queued mouse moves, so render readback and AI observation work are less likely to sit in front of keystrokes or active user work before they start
- visible URL/search navigation plus reload/back/forward controls start Servo navigation workers and poll for completion, so the shell can keep drawing and handling close/input events during page load or history navigation
- small windows hide the right rail and compute the browser viewport from the actual surface size instead of a fixed minimum shell size
- visible browser modes are ordered Agent, Assisted, Observe, Direct, Incognito; Direct and Incognito block AI observation/persistence, while Agent keeps full AI control
- Direct/Incognito still use the current frame-capture render bridge for the normal chrome/tabs shell path until Servo can render directly under shell chrome
- `sextant-servo-direct` now proves Servo can create a real window rendering context and present directly after priming the Windows ANGLE runtime; the same runner is also available from `sextant-browser --render-path direct` as a raw URL/search browsing lane
- the raw direct Servo runner forwards mouse move/click, wheel, printable text, IME commit text, and common named keys straight into Servo's `WebView::notify_input_event`; the direct input smoke proves a typed-text follow-up present without frame readback
- the raw direct Servo runner also listens to Servo URL/title/history delegate updates, mirrors them into the native window title, maps `Ctrl+L` to a title-bar location/search mode, and maps `Ctrl+R`/`F5`, `Alt+Left`, and `Alt+Right` to Servo reload/back/forward
- MCP/window smoke can now exercise the raw direct title-bar location mode with `--location-smoke <url-or-search>` and reports `locationFrameMs`; scripted location smoke waits for the active WebView load state before sending the next navigation, avoiding the early Servo browsing-context race
- MCP/window smoke can now exercise raw direct history with `--history-smoke <second-url-or-search>`; it navigates to the second target through the direct location path, waits for back/forward availability, then reports `historyNavigationFrameMs`, `historyBackFrameMs`, and `historyForwardFrameMs`
- MCP/window smoke can now exercise raw direct reload with `--reload-smoke`; it waits for the active page load state, calls Servo reload through the direct control path, and reports `reloadFrameMs`
- the raw direct Servo runner has first keyboard-only tab controls: `Ctrl+T` creates another direct WebView on the same window rendering context, `Ctrl+W` closes the active tab, `Ctrl+Tab`/`Ctrl+PageDown` cycles forward, and `Ctrl+PageUp` cycles backward; inactive tabs no longer wake the direct redraw loop, and tab resize is applied across all WebViews plus refreshed when a tab becomes active
- the raw direct Servo runner is now scoped to Direct and Incognito modes only; Agent, Assisted, and Observe still require the bridge shell because that is where AI observation/control surfaces live
- raw direct Incognito now passes an ephemeral Servo `config_dir`, disables Servo HTTP cache, reports `servoConfigDir` and `servoHttpCacheDisabled` through MCP window-smoke JSON, and uses a guarded retry cleanup helper for Windows teardown handles
- MCP exposes the direct-present proof through `browser_direct_present_smoke` and `sextant-mcp --direct-present-smoke <url> --json`, returning structured target, first-present timing, frame-readback=false, and productionIntegrated=false fields
- the production shell now has a user render-path boundary: `BRIDGE` is integrated for chrome/tabs/AI workflows, `DIRECT` launches the raw Servo window-present lane, and bridge window smoke reports `renderPath`, `renderPathStatus`, and `renderGap`
- MCP exposes `browser_render_path_baseline` and `sextant-mcp --render-path-baseline <url> --json`; the current example.com baseline measured direct present at 179ms, production bridge first frame at 1.2s, and a 1.0s gap. The opt-in pre-sized bridge variant removed resize cost but pushed first frame to 8.3s, so pre-sizing should remain experimental and off by default.
- frame capture now carries a purpose: the visible render bridge remains available in every mode for user display, while AI observation/proof snapshots are blocked when the active mode disables frame observation
- visible launches and window smokes accept `--browser-mode <agent|assisted|observe|direct|incognito>`, and Direct-mode window smoke proves the temporary render bridge can still draw a page while AI observation is disabled
- Incognito visible launches and runtime switches use an ephemeral browser data directory for Wake/Log storage, report that path in window-smoke output, restore normal profile storage when leaving Incognito, and clean up the temp directory after bounded smoke runs
- native `intent:` execution is limited to Agent and Assisted modes; Observe, Direct, and Incognito stop explicit intents at the mode boundary before navigation, distillation, Wake writes, or Pilot control work
- in-window proof workflows such as Showcase, real-browsing seed, and shell-interaction seed are also limited to Agent and Assisted modes so proof automation cannot bypass Direct/Incognito boundaries
- MCP `browser_window_smoke` pre-validates mode/workflow combinations so agents get immediate boundary errors for Direct/Observe/Incognito proof seeders or native-intent targets instead of launching the browser first
- disabled action messages, validation rows, and the status bar now explain the active mode boundary, including AI posture and profile vs ephemeral storage state
- MCP `browser_window_smoke` now returns structured `windowSmoke` fields for mode, mode status, storage, ephemeral data path, draw/navigation/frame timings, and latest frame dimensions
- `sextant-mcp --window-smoke` exposes the same visible-smoke path from the terminal, including mode selection, proof-workflow flags, mode prevalidation, and JSON output
- Servo frame capture skips redundant explicit paint calls when the WebView delegate has already painted a newly-ready frame
- native operator bridge commands for automated smoke/probe runs before manual click-through
- local trust-boundary reporting and navigation enforcement for persona, JSON policy overlays, air-gap, firewall, and privacy redaction in-window and through MCP
- normal-browsing timing telemetry for navigation, distillation, Wake search, viewport resize, and Servo frame capture in the status bar and operator probe output
- visible-window smoke timing telemetry for startup work, first shell draw, final draw cost, Servo frame blit, Softbuffer present, latest PERF summary, slowest/top perf events, and first Servo frame, with targeted smokes proving a real captured page frame instead of only a chrome redraw
- visible user-triggered DISTILL now queues an async engine worker and polls completion from the event loop; operator/proof distillation remains synchronous for deterministic checks
- MCP/window smoke can exercise that async user DISTILL path with `user_distill` / `--window-smoke-distill` and returns structured `distillMs` and `wakeMs`
- visible async DISTILL now hands Wake record/search and Captain's Log write to a dedicated Persistence lane worker; visible Wake searches, ordinary visible Log writes, and visible recent-log refreshes use the same lane, and the UI loop only polls completion and applies returned Wake/Log rows
- render-bridge telemetry now splits Servo resize from actual frame capture/readback; the first corrected assisted smoke showed resize around 18ms and frame readback around 36ms, which means the earlier 300-400ms frame numbers were mostly a measurement/round-trip artifact
- async render-bridge telemetry now also records `frame-queue`, separating Servo service wait from actual resize/capture work; after idle refresh stopped queueing during foreground distillation, an assisted user-DISTILL smoke dropped `frame-total` from about 318ms to about 56ms while capture stayed around 29-37ms
- [performance-log.md](performance-log.md) now keeps current before/after timing checkpoints for visible smokes and render-bridge/Servo bottlenecks, including first draw, first frame, navigation, distillation, Wake, frame queue, and frame capture timings
- async visible navigation now carries sub-phase timings for firewall, Servo navigation/control, Servo inspect, and reader fallback; window smoke and MCP expose the slowest/top perf events so bridge resize, navigation, and capture costs can be compared without scraping the whole log
- visible URL/search navigation uses a shorter visual load-settle wait than synchronous operator/proof navigation, preserving deterministic operator readiness while cutting MDN/Google first-frame smokes from roughly 1.2-2.5s to about 760ms on the latest debug runs
- visible URL/search navigation can now return once Servo has produced the first fresh bridge frame, instead of always waiting out the visual load-settle cap; the latest MDN/Google smokes reached first frame around 485-518ms, with `navLoadWaitMs` down to about 14-15ms
- the visible browser now hints Servo's initial runtime viewport from the real browser viewport and returns the first real bridge frame with async navigation; the latest `https://example.com` visible smoke first-framed in about 520ms with bridge resize at 0ms and frame capture around 23ms
- redirected Servo navigations no longer wait for the full exact-URL timeout; `https://www.rust-lang.org/` now accepts its `https://rust-lang.org/` final URL and visible smoke first-frames in about 1.3s instead of about 8.3s
- MCP perf baselines now attach target URLs to structured timing samples; the latest four-target baseline shows max navigation around 925ms and identifies MDN live DOM distillation around 1.1s as the current slowest phase
- live DOM perception timing now splits Servo service queue wait, Servo evaluate wait, Rust parse, in-page script work, and before/after Servo load status; `--perf-probe` also runs tiny before/after eval probes. Recent MDN probes showed queue wait at 0ms, a trivial pre-distill eval at `HeadParsed` taking about 1368ms with `script=0ms`, then the full DOM distill immediately after taking about 199ms with `script=34ms`; the slow phase is hitting Servo eval while page work is still settling, not DOM extraction, queue pressure, or selector trimming.
- MCP perf summaries now keep normal phase timing and diagnostic eval timing separate: `slowestPhase` still covers navigation/distill/Wake/resize/frame, while `slowestObservedPhase` can point at a slower eval probe and `maxEvalProbeMs` captures the worst tiny-eval sample.
- visible Agent/Assisted/Observe navigation can schedule a lightweight AI-observation eval warmup after the first real navigation frame is applied; the warmup waits for a short idle window, yields to explicit user distillation and render-bridge work, and Direct/Incognito do not start it at all, preserving the user-only performance path.
- page perception reporting for distilled semantic maps in-window and through MCP
- bounded `--window-smoke` mode for visible shell launch/draw checks before manual browsing
- visible launch seeding with `--start "<url-search-or-intent>"` for demo/manual sessions
- shortest visible demo launch with `--demo`, plus bounded showcase draw checks with `--start-showcase --window-smoke`
- launch showcase mode with `--showcase-run`
- native operator runs have an internal 120-second timeout by default and exit `124` on timeout
- status bar shows native browser validation progress during manual work, and `RESET CHECKS` starts a fresh coverage pass without restarting
- reader-mode distilled page display when a live frame is not available
- window-title status updates for quick smoke validation
- visible native browser startup failures now report clean `[sextant-browser] failed: ...` messages instead of panicking during window/event-loop setup
- queued URL/search/intent startup, live user navigation, reload/back/forward controls, and DISTILL in the visible shell so the native window and loading status can paint before Servo/navigation/distillation blocks on real page work
- no Xilem, Masonry, Vello widget tree, or toolkit lifecycle dependency on the critical path

## Known Gaps

| Gap | Priority | Notes |
|-----|----------|-------|
| Direct Servo compositor integration | High | raw production Direct/Incognito lane works, but full shell chrome/tabs/AI-surface composition over Servo direct presentation is still pending |
| Heavy browsing hardening | High | default-feature `sextant-browser` has Servo live browsing pieces, but real-window use is still buggy |
| Full in-window interactive workflow exercise | High | now belongs on the browser shell; the Xilem checklist is reference material |
| Full native browser manual validation pass | High | the first validation tab is in place; next pass should drive it through real window usage and fix any misses |
| Real sync identity semantics | Medium | sync simulation now round-trips, but imported identities intentionally do not restore secret-key material |
| Remaining dependency warning | Medium | `xml5ever v0.16.2` future-incompatibility notice remains |
| CI workflow depth | Medium | basic Rust workspace CI exists, but it is still minimal |
| Servo backend hardening | Medium | covered by tests for controlled cases, but still needs representative HTTP/HTTPS and interactive form/navigation exercise |
| Local/mesh/sync/PQ feature surfaces | Low | intentionally deferred behind the core browser loop |

## Canonical Repo State

This project now has a valid git repo and canonical remote on Gitea:

- `rustcor/Sextant`

That remote should be treated as the main source of truth going forward.

## Practical Recommendation

The next best work should keep following [next-work.md](next-work.md) and bias toward:

1. validating and hardening the default-feature Servo heavy browsing lane
2. fixing issues discovered during real click-through use
3. running the native browser validation tab through a real manual pass and refining any misses
4. provider-path and inference hardening where environment-backed behavior still drifts
5. dependency warning cleanup and CI depth
