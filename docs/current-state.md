# Current State

Date of this snapshot: `2026-04-18`

## How To Read This

Use this together with:

- [Next Work](next-work.md) for priorities
- [Architecture](architecture.md) for the stable crate model
- [Completed](completed.md) for session history

---

## Executive Summary

The Sextant Rust workspace (15 crates) compiles cleanly as of 2026-04-18. `cargo check` passes with zero errors. All foundational crate scaffolding is in place. The hull launches a Xilem window with a working 3-panel layout (Dashboard / Viewport / Digital Wake). No actual network, inference, or persistence calls are wired to the UI yet — async actions log placeholder messages pending live wiring.

---

## Compilation Status

| Crate | Status | Notes |
|-------|--------|-------|
| `sextant-vault` | ✅ Compiles | bip39 v2 API, HmacSha512, Zeroize all fixed |
| `sextant-engine` | ✅ Compiles | wgpu API updated, Servo is optional feature |
| `sextant-wake` | ✅ Compiles | rusqlite 0.37, FTS5 bundled, f32 types fixed |
| `sextant-log` | ✅ Compiles | rusqlite 0.37 |
| `sextant-inference` | ✅ Compiles | serde_json added, Vec<f32> collect fixed |
| `sextant-pilot` | ✅ Compiles | chrono added, borrow/match fixes |
| `sextant-mesh` | ✅ Compiles | wireguard-uapi (Linux-only) removed |
| `sextant-bridge` | ✅ Compiles | type inference fixes |
| `sextant-privacy` | ✅ Compiles | |
| `sextant-pq` | ✅ Compiles | |
| `sextant-sync` | ✅ Compiles | |
| `sextant-bio` | ✅ Compiles | |
| `sextant-airgap` | ✅ Compiles | |
| `sextant-firewall` | ✅ Compiles | |
| `sextant-hull` | ✅ Compiles | Full Xilem 0.1.0 API rewrite, borrow fixes |

---

## Build Environment

| Item | State |
|------|-------|
| Rust toolchain | stable, `x86_64-pc-windows-msvc` |
| VS Build Tools | 2026, `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools` |
| `.cargo/config.toml` | ✅ Present — linker + INCLUDE/LIB paths hardcoded |
| Servo feature | Disabled by default (`servo-backend` feature flag) |
| `libclang` / bindgen | Not required (Servo is optional) |

---

## What Is Working

| Capability | Status |
|------------|--------|
| `cargo check` workspace | ✅ Clean |
| Xilem window launches | ✅ Expected (not verified via `cargo run` yet) |
| Vault init + persona create | ✅ Wired in `main()` init block |
| Pilot created with Gemini brain | ✅ Wired in `main()` |
| Digital Wake init (in-memory) | ✅ Wired in `main()` |
| Captain's Log init (in-memory) | ✅ Wired in `main()` |
| Mesh node creation | ✅ Wired in `main()` |
| Tab sync from engine | ✅ `sync_tabs()` called on startup |

---

## Known Gaps

| Gap | Priority | Notes |
|-----|----------|-------|
| `cargo run` not yet verified | High | Compile passes; window launch unconfirmed |
| All async UI actions are stubs | High | Air-gap, privacy, wake search, mesh all log "pending" |
| Pilot AI calls not wired to real inference | High | Brain created but `reason_about_intent()` not called from UI |
| Digital Wake not persisted to disk | Medium | Using `new_in_memory()` — resets on exit |
| Captain's Log not persisted to disk | Medium | Same as above |
| Servo rendering not integrated | Medium | Simulated engine only |
| No CI pipeline | Medium | No build automation yet |
| TypeScript UI not connected to Rust hull | Low | Two separate surfaces; Rust hull is the target |
| PQ identity not shown in UI | Low | Field exists in state, not rendered |
