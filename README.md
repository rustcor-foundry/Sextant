# Sextant — Sovereign Agentic Browser

Sextant is a local-first, privacy-preserving browser where an AI Pilot reasons about user intent, navigates autonomously, distills page content, and builds a persistent Digital Wake without requiring a cloud service unless the user explicitly enables one.

This repo is the active product workspace for Sextant. It contains both the Rust native product path and the older TypeScript simulator/prototype.

## Read This First

If you only read five docs, read these in order:

1. [Current State](docs/current-state.md) — current product reality and what is still rough
2. [Next Work](docs/next-work.md) — active execution priorities
3. [Architecture](docs/architecture.md) — stable crate and system model
4. [Development Guardrails](docs/development-guardrails.md) — layout, workflow, and runtime rules
5. [Completed](docs/completed.md) — session history and audit trail

## Start Here

1. [Current State](docs/current-state.md) — current product snapshot
2. [Next Work](docs/next-work.md) — highest-value next actions
3. [Operator Workflow](docs/operator-workflow.md) — normal day-to-day development flow
4. [Architecture](docs/architecture.md) — system shape and crate roles
5. [Source Map](docs/source-map.md) — where code and supporting surfaces live
6. [Completed](docs/completed.md) — dated work history and archive links

## Current Product Summary

| Layer | Technology | Current State |
|-------|------------|---------------|
| Native UI shell | Rust + `winit` + `softbuffer` | Active `sextant-browser` shell with direct drawing, Intent/URL input, tabs, Wake, Log, GUARD, SENSE, Perf, Validation, MCP-backed operator tooling, and Servo frame display |
| Legacy native hull | Rust + Xilem 0.1.0 | Parked reference shell; Windows interactive use exposed toolkit access violations |
| Pilot orchestration | `sextant-pilot` | Real intent flow, consent gating, regression coverage |
| Engine | `sextant-engine` | Real fetch/distill path plus Servo-backed live navigation, frame capture, input forwarding, history, and live DOM distillation |
| Memory | `sextant-wake` | Persistent SQLite Wake with FTS + embeddings |
| Security root | `sextant-vault` | Persona, identity, secrets, consent signing scaffold |
| Audit trail | `sextant-log` | Persistent Captain's Log in SQLite |
| Cloud/local inference | Gemini / OpenAI / Anthropic / local | Provider switching and config UI in hull |
| TypeScript simulator | React + Vite + Tailwind | Legacy prototype/reference surface |

## Repo Structure

### Native Product Path

- [rust/sextant-hull/src/browser.rs](rust/sextant-hull/src/browser.rs) — active native browser shell
- [rust/sextant-hull](rust/sextant-hull) — native hull package; Xilem shell remains as parked reference code
- [rust/sextant-pilot](rust/sextant-pilot) — intent reasoning and orchestration
- [rust/sextant-engine](rust/sextant-engine) — fetch, distill, and rendering backend surface
- [rust/sextant-vault](rust/sextant-vault) — vault, personas, identities, secrets
- [rust/sextant-wake](rust/sextant-wake) — Digital Wake memory store
- [rust/sextant-log](rust/sextant-log) — Captain's Log audit trail

### Supporting Surfaces

- [src](src) — React simulator/prototype
- [components](components) — simulator UI components
- [docs](docs) — product, architecture, workflow, and session docs

## Build And Run

### Rust Native Browser Heavy Browsing Lane

This is the active lane for getting back to real browsing. It uses the hull package defaults, which enable the Servo backend.

```bash
cd rust
cargo check
cargo run -p sextant-hull --bin sextant-browser
```

### Rust Native Browser Reader/Fallback Lane

This builds without the Xilem shell and without Servo. It is useful for quick smoke checks and fallback reader-mode work, but it is not the heavy browsing path.

```bash
cd rust
cargo run -p sextant-hull --bin sextant-browser --no-default-features
```

### Rust Native Browser Operator Bridge

These commands exercise the native browser product loop without opening the visible event loop. Use them before manual click-through when hardening heavy browsing.

```bash
cd rust
cargo run -p sextant-mcp -- --launch-preflight --timeout-seconds 60
cargo run -p sextant-mcp -- --hardening-preflight --timeout-seconds 240 --visible-timeout-seconds 60
cargo run -p sextant-mcp -- --real-browsing-smoke --timeout-seconds 240
cargo run -p sextant-hull --bin sextant-browser -- --operator-smoke
cargo run -p sextant-hull --bin sextant-browser -- --showcase-run
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
cargo run -p sextant-hull --bin sextant-browser -- --intent-run "intent: open https://example.com and distill" --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --consent-run "intent: buy https://example.com and checkout" --expect "Example Domain"
cargo run -p sextant-hull --bin sextant-browser -- --window-smoke "intent: open https://example.com and distill" --window-smoke-timeout 20
cargo run -p sextant-hull --bin sextant-browser -- --start-real-browsing --window-smoke --window-smoke-timeout 60
cargo run -p sextant-hull --bin sextant-browser -- --start-shell-interaction --window-smoke --window-smoke-timeout 45
```

For a visible demo start that remains open:

```bash
cd rust
cargo run -p sextant-hull --bin sextant-browser -- --demo
```

### Legacy Xilem Hull

```bash
cd rust
cargo run -p sextant-hull --bin sextant-hull
```

The Xilem/Masonry hull is retained for reference and async-command work, but it is not the current interactive product lane on Windows.

### TypeScript Simulator

```bash
npm install
npm run dev
```

If using the simulator's Gemini path, set `GEMINI_API_KEY` first.

## Docs By Use Case

### Product Direction

- [Current State](docs/current-state.md)
- [Next Work](docs/next-work.md)
- [Product Roadmap](docs/product-roadmap.md)

### Development Workflow

- [Operator Workflow](docs/operator-workflow.md)
- [Development Guardrails](docs/development-guardrails.md)
- [Completed](docs/completed.md)

### System Reference

- [Architecture](docs/architecture.md)
- [Source Map](docs/source-map.md)

## Historical Notes

Session-specific implementation notes live under [docs/archive](docs/archive). They are useful for recovery and design rationale, but the active entry points should be the undated docs above unless a current doc points you into a dated archive file.

## Canonical Remote

The main remote for this project is:

- `rustcor/Sextant` on Gitea

Treat this repo as the source of truth going forward.
