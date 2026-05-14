# Source Map

This repo is the active product workspace for Sextant.

Use this file to answer one question quickly: where should a given kind of change actually happen?

## Canonical Remote

The main remote going forward is:

- `rustcor/Sextant` on Gitea

Use that repository as the canonical shared source of truth.

## Most Important Entry Points

### Product Docs

- [current-state.md](current-state.md) — current live product status
- [next-work.md](next-work.md) — active execution order
- [architecture.md](architecture.md) — stable system model
- [development-guardrails.md](development-guardrails.md) — project rules
- [completed.md](completed.md) — dated work history

### Native Product Path

- [rust/Cargo.toml](../rust/Cargo.toml) — Rust workspace root
- [rust/sextant-hull/src/browser.rs](../rust/sextant-hull/src/browser.rs) — active native browser shell, direct drawing, address input, browser/Wake/Log views, Servo frame display, and viewport input forwarding
- [rust/sextant-hull/src/lite.rs](../rust/sextant-hull/src/lite.rs) — compatibility wrapper for the old `sextant-hull-lite` bin alias
- [rust/sextant-hull/src/main.rs](../rust/sextant-hull/src/main.rs) — parked Xilem app startup and composition root
- [rust/sextant-hull/src/app_core.rs](../rust/sextant-hull/src/app_core.rs) — toolkit-agnostic command/event runtime
- [rust/sextant-hull/src/views.rs](../rust/sextant-hull/src/views.rs) — parked Xilem hull UI surface and validation reference
- [rust/sextant-hull/src/state.rs](../rust/sextant-hull/src/state.rs) — parked Xilem live hull state and validation reference
- [rust/sextant-pilot/src/lib.rs](../rust/sextant-pilot/src/lib.rs) — intent planning, consent, execution
- [rust/sextant-engine/src/lib.rs](../rust/sextant-engine/src/lib.rs) — tab state, fetch/distill, Servo service, live navigation, frame capture, input forwarding, and reader fallback
- [rust/sextant-mcp/src/main.rs](../rust/sextant-mcp/src/main.rs) — local stdio MCP server advertising native browser tools/resources for Codex and Claude
- [rust/sextant-wake/src/lib.rs](../rust/sextant-wake/src/lib.rs) — Digital Wake search/record/consolidation
- [rust/sextant-vault/src/lib.rs](../rust/sextant-vault/src/lib.rs) — personas, identities, secrets, consent signing
- [rust/sextant-log/src/lib.rs](../rust/sextant-log/src/lib.rs) — Captain's Log persistence

### Prototype / Legacy Reference Surface

- [src/App.tsx](../src/App.tsx) — TypeScript simulator shell
- [src/services/geminiService.ts](../src/services/geminiService.ts) — simulator Gemini integration
- [src/services/webLLMService.ts](../src/services/webLLMService.ts) — simulator local inference stub

## Where To Make Changes

### If The Task Is About The Real Product

Make changes in the Rust workspace.

This includes:

- hull UX
- native browser heavy browsing behavior
- command flow
- Pilot logic
- engine behavior
- Wake and Log persistence
- provider configuration
- consent and audit behavior

### If The Task Is About Legacy Prototype Behavior

Make changes in the TypeScript app.

This includes:

- simulator UI-only experiments
- old Gemini demo surface
- prototype interaction references

The TypeScript app is not the production target.

## Relationship Between The Two Surfaces

Use the TypeScript simulator for:

- visual reference
- old interaction ideas
- quick prototype behavior comparison

Use the Rust hull for:

- real product work
- truthful system state
- persisted memory and audit
- actual command orchestration

## Archive Relationship

Use [docs/archive](archive) for:

- session-specific implementation history
- design rationale from earlier passes
- recovery notes

Do not use archive files as the default entry point when an undated current doc already covers the same area.
