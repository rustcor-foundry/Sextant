# Completed Work

Audit trail of completed work sessions. Newest first.

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
