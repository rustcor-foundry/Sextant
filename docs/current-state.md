# Current State

Date of this snapshot: `2026-05-13`

## How To Read This

Use this together with:

- [next-work.md](next-work.md) for the active execution order
- [architecture.md](architecture.md) for the stable system model
- [development-guardrails.md](development-guardrails.md) for project rules
- [completed.md](completed.md) for dated history

## Executive Summary

Sextant has a stable core Rust foundation and an owned native-lite product path. The previous Xilem/Masonry hull remains useful as reference code, but real Windows click testing exposed native access violations in that toolkit path, so active UI work is now on the first-party `winit` + `softbuffer` shell.

The code is further along than the older May 1 wording suggested. There are now two native-lite lanes:

- **Heavy browsing lane:** `sextant-hull-lite` with package default features, which enables Servo live navigation, Servo frame capture, browser viewport input forwarding, back/forward/reload, and live DOM distillation.
- **Reader/fallback lane:** `sextant-hull-lite --no-default-features`, which avoids Servo/Xilem and keeps the direct shell, fetch/distill, Wake, and Captain's Log workflow available for fast smoke checks.

The product is no longer just a compile-clean scaffold. The core crates can:

- route commands through async app commands/events
- persist Wake and Captain's Log state
- run intent/consent regression coverage in the Pilot
- fetch, render, distill, and map browser tab state through the engine

The active `sextant-hull-lite` binary is not just a smoke window. It has a direct-drawn UI with address input, keyboard focus, hit-tested controls, browser/Wake/Log views, Wake search, Captain's Log display, tab controls, and a Servo frame viewport when built with default features.

The biggest remaining gaps are now heavy browsing hardening, real-window Servo bug fixing, richer validation inside the lite shell, provider-path hardening, CI depth, and dependency cleanup.

## What Is Working

| Capability | State | Notes |
|------------|-------|-------|
| Rust workspace compile | ✅ | `cargo check` passes |
| Workspace warning budget | ✅ | local crate warnings are clear in the latest workspace check; the remaining notice is the known `xml5ever v0.16.2` future-incompatibility warning |
| Rust workspace tests | ✅ | `cargo test --workspace` passes |
| Pilot regression tests | ✅ | `cargo test -p sextant-pilot --lib` passes |
| Native-lite reader lane | ✅ | `sextant-hull-lite --no-default-features` builds and stays responsive in a timed real launch check |
| Native-lite heavy browsing build | ✅ | package default features wire `sextant-hull-lite` to Servo; focused Servo engine tests pass |
| Native operator bridge | ✅ baseline | `--operator-smoke` drives Servo navigation, native DOM fill/click, distillation, Wake, Log, and frame capture; `--operator-probe <url>` probes real pages; `--operator-run` scripts selector actions and expectations |
| Servo live navigation | ✅ baseline / ⚠️ buggy | tests cover data URLs, live DOM mutation, history, back/forward, and cache invalidation; operator probes now cover `example.com`, `rust-lang.org`, MDN, Wikipedia, docs.rs, GitHub, `neverssl`, and `httpbin`; broader real-window browsing still needs hardening |
| Servo frame viewport | ✅ baseline / ⚠️ buggy | lite shell captures `RenderedFrame` pixels from Servo and paints them into the `softbuffer` viewport |
| Browser input forwarding | ✅ baseline / ⚠️ buggy | mouse move/click, wheel, character keys, and named keys are forwarded to the Servo WebView path; scripted selector fill/click works on data URLs and `httpbin` form inputs |
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
| Intent -> Pilot -> Engine -> Wake | ✅ baseline | covered by Pilot regression tests |
| Consent gating | ✅ baseline | await/deny/authorize flow covered by tests |
| Real page distillation | ✅ baseline | fetch + parse + semantic extraction path exists; Servo live DOM distillation captures current form values, text semantic nodes, and can fall back to reader distillation for weak live DOM snapshots |
| Tab create/switch/close | ✅ | wired through hull async path |
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

The active native-lite hull is intentionally plain, but it now has real browser-facing pieces:

- owned `winit` event loop
- direct `softbuffer` pixel rendering
- address input and keyboard focus
- browser, Wake, and Captain's Log tabs
- direct controls for GO, NEW TAB, BACK, FORWARD, RELOAD, CLOSE TAB, DISTILL, and WAKE search
- Servo viewport frame painting when built with default features
- browser viewport mouse, wheel, character-key, and named-key forwarding into the engine
- native operator bridge commands for automated smoke/probe runs before manual click-through
- reader-mode distilled page display when a live frame is not available
- window-title status updates for quick smoke validation
- visible native-lite startup failures now report clean `[sextant-lite] failed: ...` messages instead of panicking during window/event-loop setup
- no Xilem, Masonry, Vello widget tree, or toolkit lifecycle dependency on the critical path

## Known Gaps

| Gap | Priority | Notes |
|-----|----------|-------|
| Heavy browsing hardening | High | default-feature `sextant-hull-lite` has Servo live browsing pieces, but real-window use is still buggy |
| Full in-window interactive workflow exercise | High | now belongs on the lite shell; the Xilem checklist is reference material |
| Lite-shell validation rebuild | High | old Xilem checklist concepts need to be rebuilt against the lite shell's actual controls and viewport |
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
3. rebuilding validation coverage around the lite shell's actual controls
4. provider-path and inference hardening where environment-backed behavior still drifts
5. dependency warning cleanup and CI depth
