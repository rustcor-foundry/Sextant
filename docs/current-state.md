# Current State

Date of this snapshot: `2026-05-14`

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

The product is no longer just a compile-clean scaffold. The core crates can:

- route commands through async app commands/events
- persist Wake and Captain's Log state
- run intent/consent regression coverage in the Pilot
- fetch, render, distill, and map browser tab state through the engine

The active `sextant-browser` binary is not just a smoke window. It has a direct-drawn UI with address input, keyboard focus, hit-tested controls, browser/Wake/Log views, Wake search, Captain's Log display, tab controls, and a Servo frame viewport when built with default features.

`sextant-browser` is now the canonical native product binary. The old `sextant-hull-lite` bin remains as a compatibility alias during the transition, but new docs, scripts, and validation work should use `sextant-browser`.

The original Intent Bar/Context Vault shape is returning in the active shell. The address field now accepts normal URLs/searches plus explicit native intents such as `intent: open https://example.com and distill`; the native intent loop plans, navigates, distills into Wake, records Captain's Log entries, and shows Pilot state in the right rail. This is a deterministic bridge back toward the full `sextant-pilot` orchestration layer while Servo hardening continues.

The biggest remaining gaps are now heavy browsing hardening, real-window Servo bug fixing, richer validation inside the browser shell, provider-path hardening, CI depth, and dependency cleanup.

## What Is Working

| Capability | State | Notes |
|------------|-------|-------|
| Rust workspace compile | ✅ | `cargo check` passes |
| Workspace warning budget | ✅ | local crate warnings are clear in the latest workspace check; the remaining notice is the known `xml5ever v0.16.2` future-incompatibility warning |
| Rust workspace tests | ✅ | `cargo test --workspace` passes |
| Pilot regression tests | ✅ | `cargo test -p sextant-pilot --lib` passes |
| Native browser reader lane | ✅ | `sextant-browser --no-default-features` builds and stays responsive in a timed real launch check |
| Native browser heavy browsing build | ✅ | package default features wire `sextant-browser` to Servo; focused Servo engine tests pass |
| Native operator bridge | ✅ baseline | `--operator-smoke` drives Servo navigation, native DOM fill/click, distillation, Wake, Log, and frame capture; `--operator-probe <url>` probes real pages; `--operator-run` scripts selector actions and expectations; `--operator-timeout <seconds>` bounds stalled runs |
| Native intent runner | ✅ baseline | `--intent-run "intent: open https://example.com and distill" --expect "Example Domain"` exercises the Intent Bar loop headlessly through Servo navigation, live DOM distillation, Wake, Captain's Log, and frame capture |
| Launch showcase runner | ✅ baseline | `--showcase-run` proves Intent Bar, Servo navigation, live DOM distillation, Wake, Captain's Log, tab creation, native form interaction, validation progress, and frame capture in one bounded command |
| MCP browser advertising layer | ✅ baseline | `sextant-mcp` exposes local stdio MCP tools/resources for browser capabilities, bounded operator smoke/probe/run, and recent Captain's Log reads |
| Xilem/Vello diagnostic ladder | ✅ baseline | parked `sextant-hull` now has bounded `--xilem-smoke` modes for minimal, safe, interactive-smoke, and full shells; all four survived five-second first-paint checks on Windows |
| Servo live navigation | ✅ baseline / ⚠️ buggy | tests cover data URLs, live DOM mutation, history, back/forward, and cache invalidation; operator probes now cover `example.com`, Google search, `rust-lang.org`, MDN, Wikipedia, docs.rs, GitHub, `neverssl`, and `httpbin`; broader real-window browsing still needs hardening |
| Servo frame viewport | ✅ baseline / ⚠️ buggy | browser shell captures `RenderedFrame` pixels from Servo and paints them into the `softbuffer` viewport |
| Browser input forwarding | ✅ baseline / ⚠️ buggy | mouse move/click, wheel, character keys, and named keys are forwarded to the Servo WebView path; scripted selector fill/click works on data URLs, Google search, and `httpbin` form inputs |
| Native browser validation surface | ✅ baseline | owned shell now has a `VALIDATION` tab, reset control, and status-bar progress for session-aware manual click-through coverage |
| Visible browser smoke check | ✅ baseline | `--window-smoke [target] --window-smoke-timeout <seconds>` launches the real user-facing shell, draws once, optionally navigates, reports frame capture, and exits |
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

The active native browser hull is intentionally plain, but it now has real browser-facing pieces:

- owned `winit` event loop
- direct `softbuffer` pixel rendering
- address input and keyboard focus
- browser, Wake, and Captain's Log tabs
- native browser `VALIDATION` tab for session-aware manual click-through coverage
- direct controls for RUN, NEW TAB, BACK, FORWARD, RELOAD, CLOSE TAB, DISTILL, and WAKE search
- SHOWCASE control that seeds the visible app into the launch-demo proof state
- Intent/URL entry that preserves normal browsing while allowing explicit native intent runs to plan, navigate, distill, and write Wake/Log state
- Context Vault rail with current Pilot status, intent, plan summary, and result text
- Servo viewport frame painting when built with default features
- browser viewport mouse, wheel, character-key, and named-key forwarding into the engine
- native operator bridge commands for automated smoke/probe runs before manual click-through
- bounded `--window-smoke` mode for visible shell launch/draw checks before manual browsing
- visible launch seeding with `--start "<url-search-or-intent>"` for demo/manual sessions
- launch showcase mode with `--showcase-run`
- native operator runs have an internal 120-second timeout by default and exit `124` on timeout
- status bar shows native browser validation progress during manual work, and `RESET CHECKS` starts a fresh coverage pass without restarting
- reader-mode distilled page display when a live frame is not available
- window-title status updates for quick smoke validation
- visible native browser startup failures now report clean `[sextant-browser] failed: ...` messages instead of panicking during window/event-loop setup
- no Xilem, Masonry, Vello widget tree, or toolkit lifecycle dependency on the critical path

## Known Gaps

| Gap | Priority | Notes |
|-----|----------|-------|
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
