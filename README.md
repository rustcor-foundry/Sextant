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
| Native UI shell | Rust + Xilem 0.1.0 | Working hull, real async command bridge |
| Pilot orchestration | `sextant-pilot` | Real intent flow, consent gating, regression coverage |
| Engine | `sextant-engine` | Real fetch/distill path, simulated renderer backends |
| Memory | `sextant-wake` | Persistent SQLite Wake with FTS + embeddings |
| Security root | `sextant-vault` | Persona, identity, secrets, consent signing scaffold |
| Audit trail | `sextant-log` | Persistent Captain's Log in SQLite |
| Cloud/local inference | Gemini / OpenAI / Anthropic / local | Provider switching and config UI in hull |
| TypeScript simulator | React + Vite + Tailwind | Legacy prototype/reference surface |

## Repo Structure

### Native Product Path

- [rust/sextant-hull](rust/sextant-hull) — native Xilem hull
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

### Rust Native Hull

```bash
cd rust
cargo check
cargo run -p sextant-hull
```

Optional Servo feature:

```bash
cd rust
cargo run -p sextant-hull --features sextant-engine/servo-backend
```

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
