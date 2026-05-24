# Next Work

Active priority stack. See [completed.md](completed.md) for session history.

**North Star:** A working sovereign browser where the user can type an intent, the Pilot reasons about it, navigates to a page, distills the content, and records it in the Digital Wake — all without touching a cloud service unless explicitly configured.

---

## Priority Stack

### 0. Keep The Native Browser Lanes Healthy (immediate, ~15 min per pass)

`cargo check --workspace`, `cargo test --workspace`, the focused Pilot/Engine regression suites, the Servo-feature engine suite, and timed native browser launches should stay green whenever hull/runtime changes land.

Heavy browsing lane:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull --bin sextant-browser
```

Direct Servo presentation proof:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull --bin sextant-servo-direct -- --smoke --url https://example.com --timeout-seconds 45
cargo run -p sextant-mcp -- --direct-present-smoke https://example.com --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --render-path-baseline https://example.com --timeout-seconds 45 --json
```

Reader/fallback lane:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull --bin sextant-browser --no-default-features
```

Watch for: event-loop crashes, Servo service timeouts, blank/empty frames, input forwarding failures, softbuffer resize/present failures, thread/async runtime issues.

Use `sextant-servo-direct` or MCP `browser_direct_present_smoke` when the question is whether Servo itself can render directly to a native window without the current frame-capture bridge. The proof binary intentionally uses Servo's `WindowRenderingContext` and `present()` path; the production browser still needs that lane integrated with Sextant chrome, modes, tabs, input, and AI observation boundaries.

Use `sextant-browser --render-path bridge` for the current production user-display path. `--render-path direct` is intentionally parsed but rejected until the direct Servo compositor is integrated into the production shell, so a run cannot silently fall back and make performance look better than it is.

Use `sextant-mcp -- --render-path-baseline <url> --json` when comparing the experimental direct Servo presentation proof against the production bridge-backed visible smoke for the same URL. The structured `renderPathBaseline` output includes direct first-present time, bridge first-frame time, resize/frame costs, the first-frame gap, and an opt-in pre-sized bridge variant.

Native operator bridge:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
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
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode direct --timeout-seconds 30 --json
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode incognito --timeout-seconds 30 --json
cargo run -p sextant-mcp -- --window-smoke https://example.com --mode assisted --user-distill --timeout-seconds 45 --json
cargo run -p sextant-mcp -- --window-smoke --start-showcase --mode direct --json
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

Use `--operator-smoke` for a deterministic data-URL workflow that exercises Servo navigation, native DOM fill/click, live DOM distillation, Wake recording/search, Captain's Log writes, and frame capture. Use `--operator-probe <url-or-search>` to aim the same native browser path at representative HTTP/HTTPS pages before doing slower manual click-through; probe output now includes phase timings for navigation, distillation, Wake, viewport resize, and frame capture. Use `--guard-probe <url-or-search>` when the question is what local guardrails apply: it reports persona, policy source, rule counts, air-gap status, firewall decision, privacy redaction status, and a redacted sample; blocked targets are reported as policy findings instead of attempted navigation. Add `--guard-policy <json-path>` to test a persona policy overlay. Use `--perception-probe <url-or-search>` when the question is what the browser thinks the page is: it reports page type, semantic counts, key nodes, source metadata, and timings. Use `--perf-probe <url-or-search>` for the same timed pass plus a second warm frame capture when performance is the focus, and `--perf-baseline` for a small built-in set of simple and heavier pages. Use `--operator-run <url-or-search>` with `--fill`, `--click`, `--submit`, and `--expect` steps when a blocker requires a repeatable form or interaction script.

Use `--showcase-run` for the launch-demo proof. It exercises a native intent, Servo navigation, live DOM distillation, Wake, Captain's Log, tab creation, native form fill/click, and frame capture in one bounded command.

Use `cargo run -p sextant-mcp -- --launch-preflight --timeout-seconds 60` before live demo work. It runs operator smoke, Intent Bar, showcase, and visible showcase smoke checks in sequence and fails the whole run if any piece breaks.

Use `cargo run -p sextant-mcp -- --hardening-preflight --timeout-seconds 240 --visible-timeout-seconds 60` for the broader confidence pass. It runs launch checks plus real browsing, visible real browsing, and visible shell-interaction checks.

Use `cargo run -p sextant-mcp -- --real-browsing-smoke --timeout-seconds 240` when hardening normal browsing. It covers example.com, a real Google search form submission, MDN distillation, reload, IANA navigation, back/forward, Wake, Captain's Log, and Servo frame capture.

Use `--intent-run "<intent>" --expect "<text>"` to exercise the native Intent Bar loop headlessly. The current deterministic path resolves the intent, navigates with Servo when available, distills into Wake, records Captain's Log, and captures a frame. Use `--consent-run "<sensitive intent>" --expect "<text>"` when the path should pause for Captain's Key consent, authorize it, and resume the bounded browser continuation. This is the bridge back toward the original Pilot-led workflow while the active browser shell remains the stability target.

Operator runs default to a 120-second internal timeout and accept `--operator-timeout <seconds>` for harder probes. A timeout exits with code `124`.

Use `--window-smoke [url-search-or-intent] --window-smoke-timeout <seconds>` for a bounded user-facing launch/draw check. It creates the real window, initializes Softbuffer and browser app state, optionally navigates or runs an intent, waits for a Servo frame when the target/workflow should produce one, reports first draw/frame timing, and exits. Use `--start "<url-search-or-intent>"` for a visible launch that remains open, and `--demo` for the shortest showcase launch that remains open.

Use `sextant-mcp -- --window-smoke <target> --mode assisted --user-distill --json` when the question is whether a normal user-visible DISTILL action can complete through the async event-loop path. The structured `windowSmoke` output should include non-null `distillMs` and, in Assisted/Agent mode, `wakeMs`.

Use `--start-real-browsing --window-smoke --window-smoke-timeout 60` when the question is whether the visible shell can draw after the broader normal-browsing workflow.

Use `--start-shell-interaction --window-smoke --window-smoke-timeout 45` when the question is whether visible chrome clicks still work through the shell's own hit-region path.

MCP advertising layer:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-mcp -- --self-test
cargo run -p sextant-mcp -- --list-tools
cargo check -p sextant-mcp
```

The MCP server is a local stdio bridge for Codex/Claude. It exposes truthful browser capability resources plus bounded operator tools, `browser_guard_probe`, `browser_guard_policy_read`, `browser_guard_policy_write`, `browser_perception_probe`, and `browser_window_smoke`, with browser execution delegated to `sextant-browser`. Use `sextant-mcp --window-smoke ... --mode <agent|assisted|observe|direct|incognito> --json` when you want the same structured visible smoke data from a normal terminal command.

Xilem/Vello diagnostic ladder:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull --bin sextant-hull -- --xilem-modes
cargo run -p sextant-hull --bin sextant-hull -- --xilem-smoke minimal --xilem-timeout 5
cargo run -p sextant-hull --bin sextant-hull -- --xilem-smoke safe --xilem-timeout 5
cargo run -p sextant-hull --bin sextant-hull -- --xilem-smoke interactive-smoke --xilem-timeout 5
cargo run -p sextant-hull --bin sextant-hull -- --xilem-smoke full --xilem-timeout 5
```

Recent progress: all four bounded Xilem modes survived first paint for five seconds on Windows. The next Xilem/Vello work should focus on actual click/text/focus/accessibility interaction paths instead of treating initial Vello startup as the only suspect.

### 1. Stabilize Heavy Browsing In The Owned Shell (~1-2 days)

The active product lane is the first-party `sextant-browser` shell. It uses `winit` for window/input and direct `softbuffer` drawing while reusing the engine, Wake, and Captain's Log crates. With default features, it is already wired back toward heavy browsing through Servo:

Naming note: `sextant-browser` is canonical. `sextant-hull-lite` remains available only as a compatibility alias for older commands while we finish the transition.

- Servo service thread and WebView sessions
- live navigation, reload, back, and forward
- frame capture into a `RenderedFrame`
- direct pixel painting of the Servo frame into the browser viewport
- mouse, wheel, character-key, and named-key forwarding into the browser viewport
- live DOM distillation through JavaScript evaluation
- an experimental `sextant-servo-direct` proof for Servo-to-window presentation without the frame-capture bridge

Next work is not starting from scratch. It is to make that path reliable:

1. integrate the proven direct Servo presentation path into `sextant-browser` behind a mode/feature boundary so Direct/Incognito can stop depending on frame readback for user display
2. run `--operator-probe` and `--operator-run` against representative HTTP/HTTPS pages, then confirm the same pages in the visible browser shell
3. capture the exact failure modes: blank frame, stale frame, timeout, bad resize, input not reaching forms, navigation state drift, or reader fallback being triggered incorrectly
4. fix the highest-frequency failure first in `sextant-engine` or `sextant-hull/src/browser.rs`
5. keep `--no-default-features` working as the fast reader/fallback lane
6. add focused regression coverage when a bug can be reduced to engine behavior

Recent progress: `https://www.rust-lang.org` exposed a live DOM distillation timeout. The engine now gives Servo requests more room, does not poison the Servo service on a request timeout, and falls back to reader distillation when live DOM distillation fails.

Recent progress: `https://httpbin.org/forms/post` exposed that filled form values were interactable but not visible to distillation. Live DOM distillation now includes current input/select/textarea values in the semantic map, and scripted expectations search semantic attributes as well as page text.

Recent progress: `https://example.com --click "a"` exposed that click-driven navigation could leave the engine tab pointed at the old page. Native interactions now return the current Servo URL and update tab state before later distillation.

Recent progress: harder documentation/repository pages exposed semantic quality gaps. Live DOM and reader distillation now emit text nodes for paragraphs/lists/code-like content, operator probes report the distillation source, and weak live DOM snapshots can fall back to a stronger reader result.

Recent progress: operator smoke/probe/run modes now have an internal timeout guard, so stalled hard-site checks fail with exit code `124` instead of hanging until an outer process kills Cargo or the shell.

Recent progress: a real Google search flow now passes through the native operator bridge. The run loads `https://www.google.com`, fills `textarea[name=q]`, submits `form`, verifies the decoded `q` query value, distills, updates Wake/Log, and captures a Servo frame.

Recent progress: the active shell now has a first native Intent Bar loop. Normal URL/search input still navigates directly, while explicit intents such as `intent: open https://example.com and distill` plan, navigate, distill into Wake, record Captain's Log, and show Pilot state in the Context Vault rail. A matching `--intent-run` operator path keeps it regression-testable without opening the window.

Recent progress: default-feature intent runs now plan through `sextant-pilot`'s `PilotAction` vocabulary before touching the live browser state. Safe browser intents execute as Pilot `Navigate`/`Distill`/`Analyze` actions; sensitive intents pause as `RequestConsent` with `AWAITING CONSENT`; `--no-default-features` keeps the deterministic fallback path. `--intent-run "intent: open https://example.com and distill" --expect "Example Domain"` passes through this lane with Servo frame capture.

Recent progress: pending sensitive Pilot actions now have a first native Captain's Key surface in the active browser shell. The Context Vault rail shows AUTHORIZE/DENY controls, denial records an aborted audit entry, authorization records a consent signature in Captain's Log, and authorization resumes the browser-owned gated continuation. `--consent-run "intent: buy https://example.com and checkout" --expect "Example Domain"` covers the headless consent/resume path. The next deeper pass is to share browser state with `SextantPilot` directly so the full orchestrator owns the vault-signed plan lifecycle.

Recent progress: normal browsing now records phase timings for navigation, live distillation, Wake lookup, viewport resize, and Servo frame capture. The visible status bar shows the latest timing summary, `--operator-probe` emits perf lines, `--perf-probe <url>` is available for single-page baselines, and `--perf-baseline` runs a small representative page set. Initial MDN baseline before viewport-resize caching: navigation 1.2s, distillation 755ms, Wake 6ms, frame capture 863ms.

Recent progress: repeated DISTILL on an unchanged Servo tab now reuses the cached distilled page. `--perf-probe https://example.com --operator-timeout 120` showed first distill at 374ms and repeat distill at 0ms, with first frame resize/frame at 16ms/22ms and warm frame resize/frame at 0ms/19ms.

Recent progress: the MCP layer now advertises `browser_perf_probe` and `browser_perf_baseline`, plus CLI shims `sextant-mcp --perf-probe <target> --timeout-seconds <n>` and `sextant-mcp --perf-baseline --timeout-seconds <n>`, so Codex/Claude can ask the native browser for normal-browsing timing baselines directly. Perf responses also include structured `perfTimings` samples with parsed phase durations and target pages, plus `perfSummary` with max phase timings and the slowest phase.

Recent progress: the visible browser now has a `PERF` tab that shows latest phase timings, the slowest recorded phase, and a capped recent timing history. The shell-interaction smoke walks through the tab so the user-facing performance surface stays covered.

Recent progress: viewport input and frame refresh are less blocking in the visible browser. Wheel input is enqueue-only, passive mouse movement is rate-limited before it reaches Servo, printable character input is press-only, shell-debounced, and coalesced on the Servo service thread, mouse/key/wheel interactions use a debounced one-capture warmup budget, and the visible event loop now runs Servo viewport resize plus frame capture on a worker before polling the result. Synchronous capture remains available for operator/proof commands.

Recent progress: small-window layout no longer assumes a 900x620 shell. Narrow windows hide the right rail, panels are computed from the actual surface size, and the browser viewport has regression coverage proving it stays inside the visible window.

Recent progress: the active shell now has an explicit browser mode boundary ordered as Agent, Assisted, Observe, Direct, and Incognito. The implementation includes capability flags for AI control, DOM/frame observation, Wake reads/writes, content logging, MCP exposure, and direct-render intent. Direct and Incognito now block AI observation/persistence, while the current frame-capture render bridge remains available as a temporary user-render path until true Servo-to-window compositor rendering lands.

Recent progress: frame capture now carries a purpose. Visible refreshes use the temporary render-bridge path so user display still works in every mode, while AI observation/proof snapshots use a separate capability-gated path that Direct and Incognito can block cleanly. Perf labels also distinguish render-bridge captures from AI-observation captures.

Recent progress: the MCP discovery layer now advertises the same browser mode order and capability boundaries, plus the current runtime-loop split between the user interaction loop, Servo service loop, temporary render bridge, AI observation path, and persistence path. This gives Codex/Claude a truthful scope map before they call browser tools.

Recent progress: visible launches and `--window-smoke` now accept `--browser-mode <agent|assisted|observe|direct|incognito>`, and MCP `browser_window_smoke` accepts the same mode selection. Direct-mode `https://example.com` window smoke passed with a real Servo frame through the temporary render bridge while AI observation stayed disabled.

Recent progress: Incognito visible launches and runtime mode switches now use an ephemeral browser data directory for Wake/Log storage instead of the normal browser profile. Leaving Incognito reopens the normal profile and removes the temp directory. Incognito `https://example.com` window smoke passed, reported the ephemeral directory, captured a real Servo frame through the temporary render bridge, and cleaned up the temp directory after exit.

Recent progress: native `intent:` execution now stops at the mode boundary. Agent and Assisted can run explicit user-entered browser intents; Observe, Direct, and Incognito now block them before Pilot/browser-control work. A Direct-mode window smoke with `intent: open https://example.com and distill` failed intentionally with the clear boundary message instead of navigating or distilling.

Recent progress: in-window proof workflows now stop at the same mode boundary. Showcase, real-browsing seeding, and shell-interaction seeding require Agent or Assisted mode and cannot bypass Direct/Incognito by calling their visible workflow methods directly. A Direct-mode `--start-showcase --window-smoke` run failed intentionally before proof work began.

Recent progress: MCP `browser_window_smoke` now pre-validates mode/workflow combinations before launching the browser. Direct, Observe, and Incognito still allow normal page smoke checks, but they reject showcase, real-browsing, shell-interaction seeding, and native-intent targets immediately with a structured tool error.

Recent progress: the mode UX now explains itself in-window. Disabled actions use mode-aware messages, the validation tab marks AI observation, Wake, and Captain's Log rows as mode-disabled instead of merely waiting, and the status bar shows both AI posture and profile/ephemeral storage state.

Recent progress: MCP `browser_window_smoke` now returns structured `windowSmoke` data parsed from the visible smoke report: mode, mode status, storage type, ephemeral data path, first draw/frame timing, draw/blit/present costs, perf phases, and latest frame dimensions. An Incognito MCP smoke against `https://example.com` returned `mode=INCOG`, `storage=ephemeral`, and the captured Servo frame dimensions without requiring agents to scrape text.

Recent progress: `sextant-mcp --window-smoke` is now a first-class CLI shim for the same MCP visible-smoke path. It accepts `--mode`/`--browser-mode`, browser-native workflow flags like `--start-showcase`, and `--json`, then uses the MCP mode prevalidation and structured `windowSmoke` parser.

Recent progress: visible user-triggered DISTILL now follows the async worker/polling pattern instead of blocking the event loop on live DOM distillation. The engine exposes async distill result/apply helpers, the shell queues deferred user DISTILL actions through that path, and operator/proof workflows keep the synchronous path for deterministic assertions.

Recent progress: visible smoke can now exercise that user DISTILL lane directly. `sextant-mcp --window-smoke https://example.com --mode assisted --user-distill --timeout-seconds 45 --json` passed with `distillMs=381`, `wakeMs=30`, first shell draw around 33ms, and first Servo frame around 974ms.

Recent progress: render-bridge timing now separates Servo resize from actual frame capture/readback. The bridge also uses one combined Servo service command for resize+capture instead of two service round trips. The corrected assisted user-DISTILL smoke showed `resizeMs=18`, `frameMs=36`, first shell draw around 22ms, and first Servo frame around 874ms, so the earlier 300-400ms frame numbers were mostly a measurement/round-trip artifact rather than pure readback cost.

Recent progress: visible async DISTILL now has the first dedicated Persistence lane. The lane owns Wake/Log handles on a worker thread, records the distilled page, searches Wake, writes the Captain's Log entry, and returns rows to the UI loop. Profile switches shut the lane down cleanly before deleting Incognito temp storage. A real assisted user-DISTILL smoke passed with `distillMs=26`, `wakeMs=46`, `resizeMs=22`, `frameMs=27`, and first Servo frame around 895ms.

Recent progress: visible Wake searches also run through the Persistence lane now. The worker performs the Wake query, writes the Captain's Log search entry, fetches recent logs, and returns both result sets for the UI loop to apply. Regression coverage proves visible Wake search can complete from the lane without touching the UI thread directly.

Recent progress: ordinary visible Captain's Log writes now use the Persistence lane as fire-and-poll work. The synchronous log path remains for operator/proof workflows, but visible navigation, tab, control, and guard log writes no longer need to write SQLite on the UI path. Regression coverage proves visible log writes complete through the lane and refresh recent Log rows.

Recent progress: visible recent-log refreshes now run through the same Persistence lane. `refresh_logs()` keeps the synchronous path for operator/proof workflows, but the deferred visible shell queues Captain's Log reads and polls completion, avoiding leftover UI-thread SQLite reads after navigation, mode changes, or action completion. Regression coverage proves visible log refresh can complete from the lane.

Recent progress: the Servo service now has the first explicit viewport-input scheduling rule. Key, mouse, and wheel input can jump ahead of queued frame-capture/readback work, live DOM distillation, eval probes, and read-only tab inspection, while navigation and control commands keep their ordering. Queued mouse moves also coalesce to the latest position across deferred frame captures without crossing click, wheel, or key boundaries. This is the first concrete split between the user interaction lane and the render bridge inside the single-threaded Servo constraint. A fresh assisted user-DISTILL visible smoke against `https://example.com` passed with first shell draw around 35ms, navigation around 406ms, distill around 310ms, Wake/persistence around 50ms, resize around 20ms, frame readback around 24ms, and first Servo frame around 862ms.

Recent progress: the visible shell now has its own viewport input lane before the engine enqueue. Mouse moves collapse to the latest point, adjacent wheel deltas merge, and printable text batches before the Servo service sees the commands. The queue is pinned to the active tab and is dropped if the user switches tabs before flush, preventing stale input from landing on the wrong page. This keeps high-frequency winit handlers cheap and makes the user interaction lane visible in MCP `browser_capabilities`. The post-change assisted user-DISTILL visible smoke passed with first shell draw around 25ms, navigation around 407ms, distill around 346ms, Wake/persistence around 48ms, resize around 30ms, frame readback around 51ms, and first Servo frame around 918ms.

Recent progress: visible tab-switch and close-tab controls now refresh the render bridge through the async frame-capture path instead of synchronously resizing/capturing Servo from the click handler. Operator/proof workflows keep the synchronous refresh path for deterministic evidence. The post-change assisted user-DISTILL visible smoke passed with first shell draw around 38ms, navigation around 488ms, distill around 448ms, Wake/persistence around 90ms, resize around 27ms, frame readback around 36ms, and first Servo frame around 1.1s.

Recent progress: visible reload, back, and forward controls now use async Servo navigation-control workers and the existing pending-navigation polling path. The synchronous reload/back/forward methods remain for operator/proof workflows, but normal user controls no longer perform Servo navigation on the UI loop after the loading notice paints. A Servo-backed regression proves visible reload completes through the worker, and the post-change assisted user-DISTILL visible smoke passed with first shell draw around 19ms, navigation around 477ms, distill around 370ms, Wake/persistence around 58ms, resize around 31ms, frame readback around 33ms, and first Servo frame around 1.0s.

Recent progress: fresh debug probes showed the remaining performance shape more clearly. `https://www.google.com` loaded with Servo in about 2.1s with resize/frame around 10ms/27ms; `https://developer.mozilla.org/en-US/docs/Web/HTML` loaded in about 1.1s with resize/frame around 31ms/17ms on the latest run, while visible MDN smoke drew chrome in about 22ms and reached the first Servo frame in about 1.2s. Normal visible URL/search navigation now starts a Servo navigation worker and keeps the event loop alive while the page load runs; Google visible smoke showed the loading status and first shell draw around 29-35ms, then the first Servo frame around 2.3s. A pre-navigation viewport resize experiment removed the first-frame resize but pushed Google navigation to about 8.1s, so it is not used in the live path. The next bottlenecks to attack are first-frame readiness/settle time inside Servo navigation, residual Servo/WebRender CPU churn while idle, and any remaining user-facing Wake/search/log work that shows up as input latency.

Recent progress: a first direct Servo presentation proof now compiles and runs as `sextant-servo-direct`. It shares the Windows ANGLE runtime bootstrap used by the engine, creates Servo's `WindowRenderingContext`, loads `https://example.com`, paints, and calls `present()` directly. The first smoke reached direct present in about 165ms, confirming that the next render-bridge milestone should be integration into the production shell rather than more readback optimization.

Recent progress: MCP now advertises and runs the direct-present proof through `browser_direct_present_smoke` plus `sextant-mcp --direct-present-smoke <url> --json`. The structured result includes target, first-present timing, `frameReadback=false`, and `productionIntegrated=false`, so agents can distinguish the experimental direct compositor lane from the production render bridge.

Recent progress: `sextant-browser` now has a production user render-path boundary. Window smoke reports `renderPath=BRIDGE`, a human-readable render status, and `renderGap=direct compositor pending` when Direct/Incognito mode still require the future compositor path. `--render-path direct` exits with a clear integration-incomplete error instead of silently falling back to the frame bridge.

Recent progress: MCP now has `browser_render_path_baseline`. The latest example.com run measured direct Servo first present at 179ms, production bridge first frame at 1.2s, bridge resize at 368ms, bridge capture at 50ms, and a 1.0s first-frame gap. The opt-in pre-sized bridge variant removed post-navigation resize but pushed navigation/first frame to 8.1s/8.3s, confirming that pre-sizing should stay off by default while direct compositor integration remains the obvious rendering win.

Recent progress: the visible browser now has a `SENSE` tab for distilled page perception. It summarizes page type, semantic counts, key nodes, and distillation source, Pilot `Perceive` routes there, `--perception-probe <target>` emits the same semantic report, and MCP exposes it as `browser_perception_probe` with structured `perception` output.

Recent progress: the visible browser now has a `GUARD` tab for the local trust boundary. It reports persona, policy source, rule counts, air-gap status, firewall decision, and privacy redaction status, `--guard-probe <target>` emits the same report before and after navigation/distillation, and MCP exposes it as `browser_guard_probe` with structured `guard` output. Navigation and scripted interaction paths now ask the local guard first; blacklisted targets are blocked and audited before engine navigation starts. `guard-policy.json`, `SEXTANT_GUARD_POLICY`, and `--guard-policy <json-path>` can add persona-specific firewall overlay rules for operator and agent probes. MCP can now read/write the canonical browser policy through `browser_guard_policy_read` and `browser_guard_policy_write`.

Recent progress: the browser now has a launch showcase path. `--showcase-run` proves the product loop across Intent Bar, Servo, Wake, Captain's Log, tab creation, native form interaction, and frame capture. The visible shell also accepts `--start "<url-search-or-intent>"`, and `--window-smoke` can now run an intent before drawing.

Recent progress: the visible browser toolbar now includes a `SHOWCASE` control that runs the same launch-demo workflow in-window.

Recent progress: `sextant-browser --demo` now opens the visible browser directly into the launch showcase proof state. `--start-showcase` can be combined with `--window-smoke` for a bounded visible check of that same path.

Recent progress: `sextant-mcp --launch-preflight` now runs the launch-critical pre-demo sequence and passed operator smoke, Intent Bar, showcase, and visible showcase checks.

Recent progress: `sextant-mcp --real-browsing-smoke --timeout-seconds 240` now runs and passes a broader normal-browsing suite across example.com, Google search, MDN, reload, IANA, back/forward, Wake, Captain's Log, and Servo frame capture. It also exposed and fixed a stale Servo navigation success where a requested navigation could report success while the WebView stayed on the previous URL.

Recent progress: the same real-browsing workflow can now seed the visible browser with `--start-real-browsing`, and `--start-real-browsing --window-smoke --window-smoke-timeout 60` passed with a captured Servo frame. MCP `browser_window_smoke` also accepts `{"real_browsing": true}` for the same visible proof.

Recent progress: QA found that Google can issue a delayed interstitial redirect after the search form test and race the following documentation navigation. The real-browsing workflow now opens a fresh tab for the MDN/IANA history segment after proving Google search, keeping the broader visible smoke stable without dropping the real Google coverage.

Recent progress: `--start-shell-interaction --window-smoke --window-smoke-timeout 45` now passes. It clicks the native chrome controls for address/run, distill, Wake, view tabs, viewport focus, new tab, page-tab switching, close tab, and page-tab overflow paging before drawing, and MCP `browser_window_smoke` accepts `{"shell_interaction": true}` for the same check.

Recent progress: the visible page-tab strip now behaves more like a real browser surface. Tabs keep stable visual order when selected, closing the active tab falls back to the adjacent tab, newly opened/selected tabs stay visible, and overflow arrows plus mouse-wheel paging make hidden tabs reachable.

Recent progress: `sextant-mcp --hardening-preflight --timeout-seconds 240 --visible-timeout-seconds 60` now passes. It chains operator smoke, Intent Bar, showcase, real browsing, visible showcase, visible real browsing, and visible shell-interaction checks into one broader confidence command.

Recent progress: targeted visible smokes now require a captured Servo frame before passing, cache the Softbuffer surface size so normal redraws skip repeated resize calls, and print startup work, first shell draw, final draw cost, Servo frame blit, Softbuffer present, and first Servo frame timing. The visible frame blit now uses integer stepping and row-slice writes instead of per-pixel float division. URL/search/intent startup, live user Enter/RUN navigation, reload/back/forward controls, and DISTILL are queued until after the shell paints a loading status before blocking on Servo or distillation work, so targeted launches and normal browsing show truthful native feedback before the page frame or Wake result arrives. Operator/showcase proof workflows keep immediate navigation before the event loop so deterministic smoke scripts still run to completion. Latest debug checks showed chrome-only first draw around 20-35ms, `https://example.com` first shell draw around 18-34ms with first Servo frame around 891ms-1.1s, `https://developer.mozilla.org/en-US/docs/Web/HTML` around 1.1-1.2s to first frame with about 7ms in frame blit, and `https://www.google.com` first shell draw around 32-48ms with first Servo frame around 2.3s.

Recent progress: async visible navigation now returns sub-phase timings for firewall, Servo navigation/control, Servo inspect, and reader fallback. Window smoke prints the slowest perf event plus the top perf events, and MCP parses those into `slowestPerfEvent` / `slowPerfEvents` so agents can see which lane is actually slow. The browser configures a fresh Servo runtime with the visible browser viewport so the first capture can skip same-size resize without the rejected pre-size-navigation path, and async navigation now returns the first real bridge frame directly so the shell can paint it without launching a second capture worker. Latest `https://example.com` smoke showed first shell draw around 22ms, startup/navigation work around 432ms, Servo navigation sub-phase around 405ms, bridge resize at 0ms, frame capture around 23ms, and first Servo frame around 520ms.

Recent progress: redirected Servo navigations now count as successful URL progress instead of waiting for the exact requested URL until timeout. This fixed the `https://www.rust-lang.org/` path, where Servo lands on `https://rust-lang.org/`: visible smoke dropped from about 8.3s first frame to about 1.3s, and `browser_perf_probe` reports navigation around 1.2s with live DOM distillation intact. MCP perf timing parsing now also attaches target URLs for `[perf-baseline] <url>: perf ...` lines, so `perfSummary.slowestPhase.target` identifies the page behind the slow phase. Latest four-target baseline shows max navigation around 925ms, frame capture under about 46ms, and MDN live DOM distillation around 1.1s as the current slowest phase.

Recent progress: live DOM distillation telemetry now splits Servo service queue wait, the outer Servo JavaScript evaluation wait, Rust parse time, the in-page script's own runtime, and before/after Servo load status. `--perf-probe` also runs tiny eval probes before and after distillation. A JSON-transport experiment, a main/article-scoped DOM walk, and a bounded pre-eval readiness wait did not produce a real speedup, so they were not kept as performance changes. The useful result is diagnostic: MDN probes kept full semantic coverage at 180 nodes and showed queue wait at 0ms, a trivial pre-distill eval at `HeadParsed` taking about 1368ms with `script=0ms`, then the full DOM distill immediately after taking about 199ms with `script=34ms`. The bottleneck is hitting Servo eval while page work is still settling, not queue pressure or selector workload.

Recent progress: MCP perf summaries now surface diagnostic eval probes without hiding normal phase timing. `perfSummary.slowestPhase` remains the normal navigation/distill/Wake/resize/frame view, while `perfSummary.slowestObservedPhase` and `maxEvalProbeMs` include the before/after tiny eval samples so agents can see when Servo eval readiness is the real long pole.

Recent progress: visible navigation now has an opt-in AI-observation warmup lane. After a real initial navigation frame is applied, Agent/Assisted/Observe modes can schedule a tiny async Servo eval probe to absorb the first slow eval window before the user or AI asks for DOM distillation. The shell waits for a short idle window before starting it, and the Servo scheduler treats the probe as background work: explicit distillation and render-bridge capture can pass it before it starts, and Direct/Incognito skip the lane entirely so the human-only browsing path does not pay for AI readiness work.

Recent progress: the Servo service scheduler now lets pending live-DOM distillation yield to already-queued viewport input, matching the existing frame-capture yield behavior. This does not preempt a distillation once Servo has started evaluating it, but it keeps AI observation work from jumping ahead of user keystrokes, clicks, wheel, pointer movement, explicit distillation, or render capture work that is already waiting in the service queue.

Recent progress: viewport wheel input now uses an enqueue-only Servo command and queues frame warmup instead of waiting for Servo acknowledgement or synchronously capturing a Servo frame inside the wheel handler. That keeps scroll input responsive while the event loop owns the next frame capture.

Recent progress: viewport mouse/key/wheel input now uses a smaller interaction frame-warmup budget than page navigation and distillation. This keeps enough follow-up captures for visible feedback without rearming the full navigation warmup window on every input burst.

Recent progress: viewport text input no longer rearms the render bridge while it is still only shell-buffered. The frame warmup starts when the text is flushed into the tab-pinned viewport input lane, and bridge refresh waits a short 24ms settle window after input enqueue before capturing. This keeps Servo input ahead of readback work instead of letting every keystroke immediately schedule another bridge capture. The PERF history now records `input viewport enqueue` events when viewport input is handed to Servo. Warmed Google Direct smoke stayed in the improved load band: first draw around 22ms, first frame around 549ms, navigation around 470ms, `navUrlWaitMs` around 334ms, and bridge capture around 22ms.

Recent progress: visible window smoke now has an input-latency mode. MCP `browser_window_smoke` accepts `input_latency: true`, and CLI runs can pass `--input-latency` / `--window-input-smoke`. With no target, the smoke opens a centered input fixture, waits briefly after the first frame, sends text through the same viewport input lane as user typing, waits for the follow-up bridge frame, and returns structured `inputEnqueueMs` / `inputFrameMs` fields. The first Direct-mode fixture checkpoint showed input enqueue at 0ms and follow-up frame around 137ms, making keyboard responsiveness a tracked metric instead of just a manual feel check.

Recent progress: input follow-up capture now wakes at the 24ms input-settle deadline instead of sleeping until the full 48ms interaction cadence. The Direct-mode input fixture improved from about 137ms to about 77ms for the follow-up frame, with input enqueue still reporting 0ms. The remaining visible delay is mostly settle time, bridge capture, and redraw polling rather than shell-to-Servo handoff.

Recent progress: the input-settle window was shortened from 24ms to 16ms after the smoke proved shell-to-Servo enqueue remained at 0ms. The Direct-mode input fixture follow-up frame moved from about 77ms to about 65ms. The remaining target is now the bridge/readback and redraw tail rather than the input queue.

Recent progress: visible event-loop frame refresh now starts Servo frame capture on a background worker and polls the result instead of blocking the window loop. Synchronous `refresh_frame` remains available for operator/proof paths that need immediate evidence.

Recent progress: async render-bridge telemetry now splits Servo service queue wait into a `frame-queue` perf event, and window smoke prints the top six slow perf events so queue pressure is visible to operators and MCP clients. A fresh assisted user-DISTILL smoke showed the problem clearly (`frame-total` about 318ms with `frame-queue` about 272ms), then idle frame refresh was changed to wait while foreground Servo navigation, distillation, or AI warmup work is pending. The follow-up smoke dropped `frame-total` to about 56ms with capture around 29-37ms and no `frame-queue` in the top six.

Recent progress: current timing checkpoints now live in `docs/performance-log.md`, and MCP window-smoke JSON exposes `frameTotalMs` and `frameQueueMs` as first-class fields. The latest checkpoints show bridge queue pressure cleared on example.com/MDN/Google smokes, leaving Servo navigation/page readiness as the next large bottleneck.

Recent progress: visible navigation now uses a shorter Servo load-settle timeout than synchronous operator/proof navigation, and MCP window-smoke JSON exposes `navUrlWaitMs` and `navLoadWaitMs` when split navigation events are present. MDN assisted first-frame timing moved from about 1.2s to about 763ms, and Google Direct moved from about 2.5s to about 756ms, while example.com assisted user-DISTILL still passes immediately after navigation.

Recent progress: visible navigation now exits its load wait as soon as a fresh Servo frame is available, while synchronous operator/proof paths keep the longer deterministic readiness wait. The latest MDN assisted smoke reached first frame around 485ms with `navLoadWaitMs` around 14ms, and Google Direct reached first frame around 518ms with `navLoadWaitMs` around 15ms. The next measured target is the remaining `navUrlWaitMs` around 248-259ms without accepting stale navigation state.

### 2. Full In-Window Workflow Validation (~1-2 hours)

The old Xilem validation checklist is reference material. Rebuild the same workflow coverage in the native browser shell around its actual controls and browser viewport, then run a real click-through pass that exercises the app end to end in the window and fixes anything that still only works in tests or partial runtime paths.

Recent progress: `sextant-browser` now has a first-party `VALIDATION` tab and status-bar progress for the current window session. It tracks navigation, frame/reader visibility, real forwarded browser input, distillation, Wake, Captain's Log, tab controls, and error surfacing. `RESET CHECKS` starts a fresh validation pass without restarting or clearing runtime state.

Focus checks:
1. navigate from the address field and verify the live viewport or reader fallback updates truthfully
2. click into the browser viewport, type into a simple form page, and confirm key/mouse events reach Servo
3. distill the active page and verify Wake and Captain's Log update
4. switch between Browser, Wake, and Captain's Log views
5. create, close, reload, go back, and go forward across tabs
6. confirm failures surface in-window instead of only through stderr or logs
7. refine the native browser validation checklist based on real manual pass findings

### 3. Truthful UX and Error Surfacing (~2-4 hours)

The native hull is already much more honest than earlier passes, so this lane is now follow-up based on what real validation uncovers:
1. tighten any stale or misleading status text
2. surface startup/runtime failures in-window instead of only via stderr/logs
3. make unfinished panels explicit without implying they are wired
4. remove any remaining "looks wired but is not" edge cases

### 4. Real Inference Configuration Path (~2-4 hours)

Provider switching and settings controls are implemented, and the hull loads environment keys on startup. The remaining work is to make the configured provider path fully real and clearly persisted:
- verify applied provider changes always switch the active Pilot brain
- harden vault save/load behavior for provider settings
- confirm local inference and air-gap interactions behave as intended
- remove any leftover placeholder-model assumptions where they still exist

### 5. Servo Rendering Backend Hardening (~1-2 days)

The hull package now exposes the `servo-backend` feature explicitly. `cargo test -p sextant-engine --lib --features servo-backend` covers Servo navigation, tab history, back/forward, live DOM distillation, data URLs, and cache invalidation. Remaining work is hardening rather than first enablement:
1. run real-window navigation against representative HTTP/HTTPS pages
2. improve error text for Servo service/session failures
3. tighten frame refresh and resize behavior in `sextant-browser`
4. decide whether the hull should expose a visible reader/fallback mode switch for machines that cannot run Servo cleanly

### 6. Warning and Dependency Cleanup (~1-2 hours)

The workspace is currently green, but there is still follow-up cleanup worth doing:
- reduce remaining low-signal warnings in less-active crates
- inspect the `xml5ever v0.16.2` future-incompatibility notice and decide whether to upgrade or track it explicitly
- keep the workspace warning budget low enough that new regressions stand out

### 7. CI Depth

Basic Rust workspace CI is already in place on Gitea. The next CI pass should add depth rather than bootstrap:
- consider a dedicated hull smoke check strategy
- add focused crate test jobs if failures become noisy
- keep CI aligned with the native product path rather than the legacy simulator

---

## Known Blockers

| Blocker | Affects | Notes |
|---------|---------|-------|
| Interactive native launch validation still needs a human pass | Items 0-3 | `--operator-smoke` and `--operator-probe` now cover native browser automation before full click-through |
| Xilem/Masonry hull crashes interactively on Windows | Legacy hull | Parked as reference while the first-party shell becomes the active lane |
| Real cloud/local provider validation depends on credentials/services | Item 3 | Behavior is partly environment-dependent |
| Servo runtime behavior still needs broader real-window exercise | Items 1 and 5 | Engine tests and operator probes cover controlled and basic HTTP/HTTPS paths; representative interactive sites still need validation |
| Local `wsky-distiller` dependency | Build onboarding | `sextant-engine` currently depends on `../../../wsky-distiller/distill`, so this checkout is expected beside Sextant |

---

## Nice-to-Have

- **Mesh panel**: `is_mesh_open` exists, but mesh remains a later native pass
- **PQ identity display**: `active_pq_identity` in state, not shown in hull
- **Sync UI**: `SextantSync` is wired in state but no import/export UI
