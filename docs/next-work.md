# Next Work

Active priority stack. See [completed.md](completed.md) for session history.

**North Star:** A working sovereign browser where the user can type an intent, the Pilot reasons about it, navigates to a page, distills the content, and records it in the Digital Wake — all without touching a cloud service unless explicitly configured.

---

## Priority Stack

### 0. Keep The Native-Lite Lanes Healthy (immediate, ~15 min per pass)

`cargo check --workspace`, `cargo test --workspace`, the focused Pilot/Engine regression suites, the Servo-feature engine suite, and timed native-lite launches should stay green whenever hull/runtime changes land.

Heavy browsing lane:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull --bin sextant-hull-lite
```

Reader/fallback lane:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull --bin sextant-hull-lite --no-default-features
```

Watch for: event-loop crashes, Servo service timeouts, blank/empty frames, input forwarding failures, softbuffer resize/present failures, thread/async runtime issues.

Native operator bridge:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull --bin sextant-hull-lite -- --operator-smoke
cargo run -p sextant-hull --bin sextant-hull-lite -- --operator-probe https://example.com
cargo run -p sextant-hull --bin sextant-hull-lite -- --operator-run https://example.com --expect "Example Domain"
```

Use `--operator-smoke` for a deterministic data-URL workflow that exercises Servo navigation, native DOM fill/click, live DOM distillation, Wake recording/search, Captain's Log writes, and frame capture. Use `--operator-probe <url-or-search>` to aim the same native-lite path at representative HTTP/HTTPS pages before doing slower manual click-through. Use `--operator-run <url-or-search>` with `--fill`, `--click`, `--submit`, and `--expect` steps when a blocker requires a repeatable form or interaction script.

### 1. Stabilize Heavy Browsing In The Owned Shell (~1-2 days)

The active product lane is the first-party `sextant-hull-lite` shell. It uses `winit` for window/input and direct `softbuffer` drawing while reusing the engine, Wake, and Captain's Log crates. With default features, it is already wired back toward heavy browsing through Servo:

- Servo service thread and WebView sessions
- live navigation, reload, back, and forward
- frame capture into a `RenderedFrame`
- direct pixel painting of the Servo frame into the lite viewport
- mouse, wheel, character-key, and named-key forwarding into the browser viewport
- live DOM distillation through JavaScript evaluation

Next work is not starting from scratch. It is to make that path reliable:

1. run `--operator-probe` and `--operator-run` against representative HTTP/HTTPS pages, then confirm the same pages in the visible lite shell
2. capture the exact failure modes: blank frame, stale frame, timeout, bad resize, input not reaching forms, navigation state drift, or reader fallback being triggered incorrectly
3. fix the highest-frequency failure first in `sextant-engine` or `sextant-hull/src/lite.rs`
4. keep `--no-default-features` working as the fast reader/fallback lane
5. add focused regression coverage when a bug can be reduced to engine behavior

Recent progress: `https://www.rust-lang.org` exposed a live DOM distillation timeout. The engine now gives Servo requests more room, does not poison the Servo service on a request timeout, and falls back to reader distillation when live DOM distillation fails.

Recent progress: `https://httpbin.org/forms/post` exposed that filled form values were interactable but not visible to distillation. Live DOM distillation now includes current input/select/textarea values in the semantic map, and scripted expectations search semantic attributes as well as page text.

Recent progress: `https://example.com --click "a"` exposed that click-driven navigation could leave the engine tab pointed at the old page. Native interactions now return the current Servo URL and update tab state before later distillation.

### 2. Full In-Window Workflow Validation (~1-2 hours)

The old Xilem validation checklist is reference material. Rebuild the same workflow coverage in the native-lite shell around its actual controls and browser viewport, then run a real click-through pass that exercises the app end to end in the window and fixes anything that still only works in tests or partial runtime paths.

Focus checks:
1. navigate from the address field and verify the live viewport or reader fallback updates truthfully
2. click into the browser viewport, type into a simple form page, and confirm key/mouse events reach Servo
3. distill the active page and verify Wake and Captain's Log update
4. switch between Browser, Wake, and Captain's Log views
5. create, close, reload, go back, and go forward across tabs
6. confirm failures surface in-window instead of only through stderr or logs
7. rebuild session-aware validation UI once the lite control set is stable enough

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
3. tighten frame refresh and resize behavior in `sextant-hull-lite`
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
| Interactive native launch validation still needs a human pass | Items 0-3 | `--operator-smoke` and `--operator-probe` now cover native-lite automation before full click-through |
| Xilem/Masonry hull crashes interactively on Windows | Legacy hull | Parked as reference while the first-party shell becomes the active lane |
| Real cloud/local provider validation depends on credentials/services | Item 3 | Behavior is partly environment-dependent |
| Servo runtime behavior still needs broader real-window exercise | Items 1 and 5 | Engine tests and operator probes cover controlled and basic HTTP/HTTPS paths; representative interactive sites still need validation |
| Local `wsky-distiller` dependency | Build onboarding | `sextant-engine` currently depends on `../../../wsky-distiller/distill`, so this checkout is expected beside Sextant |

---

## Nice-to-Have

- **Mesh panel**: `is_mesh_open` exists, but mesh remains a later native pass
- **PQ identity display**: `active_pq_identity` in state, not shown in hull
- **Sync UI**: `SextantSync` is wired in state but no import/export UI
