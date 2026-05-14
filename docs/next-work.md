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
cargo run -p sextant-hull --bin sextant-browser -- --operator-probe https://example.com
cargo run -p sextant-hull --bin sextant-browser -- --operator-run https://example.com --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --operator-timeout 45 --operator-run https://example.com --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --operator-timeout 45 --intent-run "intent: open https://example.com and distill" --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --window-smoke https://example.com --window-smoke-timeout 15
```

Use `--operator-smoke` for a deterministic data-URL workflow that exercises Servo navigation, native DOM fill/click, live DOM distillation, Wake recording/search, Captain's Log writes, and frame capture. Use `--operator-probe <url-or-search>` to aim the same native browser path at representative HTTP/HTTPS pages before doing slower manual click-through. Use `--operator-run <url-or-search>` with `--fill`, `--click`, `--submit`, and `--expect` steps when a blocker requires a repeatable form or interaction script.

Use `--intent-run "<intent>" --expect "<text>"` to exercise the native Intent Bar loop headlessly. The current deterministic path resolves the intent, navigates with Servo when available, distills into Wake, records Captain's Log, and captures a frame. This is the bridge back toward the original Pilot-led workflow while the active browser shell remains the stability target.

Operator runs default to a 120-second internal timeout and accept `--operator-timeout <seconds>` for harder probes. A timeout exits with code `124`.

Use `--window-smoke [url-or-search] --window-smoke-timeout <seconds>` for a bounded user-facing launch/draw check. It creates the real window, initializes Softbuffer and browser app state, optionally navigates to a target, draws once, reports any Servo frame, and exits.

MCP advertising layer:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-mcp -- --self-test
cargo run -p sextant-mcp -- --list-tools
cargo check -p sextant-mcp
```

The MCP server is a local stdio bridge for Codex/Claude. It exposes truthful browser capability resources plus bounded operator tools and `browser_window_smoke`, all delegated to `sextant-browser`.

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
