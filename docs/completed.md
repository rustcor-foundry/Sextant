# Completed Work

Audit trail of completed work sessions. Newest first.

---

## 2026-05-14 — Launch Showcase Path

- Added `sextant-browser --showcase-run` as a bounded launch-demo proof across Intent Bar, Servo navigation, live DOM distillation, Wake, Captain's Log, tab creation, native form fill/click, validation progress, and frame capture
- Added visible browser launch seeding with `--start "<url-search-or-intent>"`
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
