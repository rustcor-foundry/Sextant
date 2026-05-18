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

Reader/fallback lane:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull --bin sextant-browser --no-default-features
```

Watch for: event-loop crashes, Servo service timeouts, blank/empty frames, input forwarding failures, softbuffer resize/present failures, thread/async runtime issues.

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

Use `--start-real-browsing --window-smoke --window-smoke-timeout 60` when the question is whether the visible shell can draw after the broader normal-browsing workflow.

Use `--start-shell-interaction --window-smoke --window-smoke-timeout 45` when the question is whether visible chrome clicks still work through the shell's own hit-region path.

MCP advertising layer:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-mcp -- --self-test
cargo run -p sextant-mcp -- --list-tools
cargo check -p sextant-mcp
```

The MCP server is a local stdio bridge for Codex/Claude. It exposes truthful browser capability resources plus bounded operator tools, `browser_guard_probe`, `browser_guard_policy_read`, `browser_guard_policy_write`, `browser_perception_probe`, and `browser_window_smoke`, with browser execution delegated to `sextant-browser`.

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

Next work is not starting from scratch. It is to make that path reliable:

1. run `--operator-probe` and `--operator-run` against representative HTTP/HTTPS pages, then confirm the same pages in the visible browser shell
2. capture the exact failure modes: blank frame, stale frame, timeout, bad resize, input not reaching forms, navigation state drift, or reader fallback being triggered incorrectly
3. fix the highest-frequency failure first in `sextant-engine` or `sextant-hull/src/browser.rs`
4. keep `--no-default-features` working as the fast reader/fallback lane
5. add focused regression coverage when a bug can be reduced to engine behavior

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

Recent progress: viewport wheel input now queues frame warmup instead of synchronously capturing a Servo frame inside the wheel handler. That keeps scroll input responsive while the event loop owns the next frame capture.

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
