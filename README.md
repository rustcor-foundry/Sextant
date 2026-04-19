# Sextant — Sovereign Agentic Browser

A local-first, privacy-preserving browser where an AI Pilot reasons about your intent, navigates autonomously, and builds a persistent memory of your web activity — all without leaving your machine unless you decide otherwise.

## Read This First

1. [Development Guardrails](docs/development-guardrails.md) — project rules for layout, workflow, and scope
2. [Current State](docs/current-state.md) — live build status and known gaps
3. [Next Work](docs/next-work.md) — active priority stack
4. [Architecture](docs/architecture.md) — crate map and design decisions
5. [Completed](docs/completed.md) — session audit trail

---

## Project Summary

| Layer | Technology | Status |
|-------|-----------|--------|
| Native UI shell | Rust + Xilem 0.1.0 | ✅ Compiles |
| Rendering engine | Servo (optional feature) | Simulated |
| Agentic orchestration | `sextant-pilot` | Scaffolded |
| Episodic memory | SQLite FTS5 + vectors | Scaffolded |
| Cryptographic vault | Ed25519 + AES-256-GCM + ML-DSA | Scaffolded |
| Local AI inference | llama.cpp / OpenAI / Gemini | Scaffolded |
| P2P mesh | libp2p | Scaffolded |
| TypeScript simulator | React 19 + Vite + Tailwind | Working |

---

## Crate Structure

```
rust/
├── sextant-hull        # Xilem UI shell
├── sextant-pilot       # Agentic orchestration + intent reasoning
├── sextant-engine      # Page distillation (Servo optional)
├── sextant-vault       # Cryptographic vault (HD keys, AES, ML-DSA)
├── sextant-wake        # Digital Wake — episodic memory store
├── sextant-inference   # Multi-backend LLM client
├── sextant-log         # Captain's Log — audit trail
├── sextant-pq          # Post-quantum identity (ML-DSA-65)
├── sextant-privacy     # PII redaction + privacy levels
├── sextant-mesh        # P2P mesh networking
├── sextant-bridge      # Multimodal input bridge
├── sextant-sync        # Cross-device persona sync
├── sextant-bio         # Biometric auth abstraction
├── sextant-airgap      # Air-gap mode (Online/Isolated/Hardened)
└── sextant-firewall    # Network filtering + tracker blocking
```

---

## Build

**Requirements**: Rust stable, VS Build Tools 2026 (Windows)

The `.cargo/config.toml` in `rust/` persists the MSVC linker and SDK paths — no manual environment setup needed after cloning.

```bash
cd rust

# Check all crates compile
cargo check

# Run the native hull
cargo run -p sextant-hull

# Run with Servo rendering (requires libclang)
cargo run -p sextant-hull --features sextant-engine/servo-backend
```

## TypeScript Simulator

The `src/` directory contains a React + Vite UI that was the original prototype. It provides an interactive preview and connects to Gemini via API key.

```bash
npm install
# Set GEMINI_API_KEY in .env.local
npm run dev
```

---

## Design Principles

**Sovereign by default**: No telemetry, no cloud sync unless the user enables it. The vault is the trust root — nothing sensitive leaves without a signed consent.

**Persona isolation**: Every browsing session, memory entry, and identity is scoped to a named persona. Switching persona changes the AI's context, the available keys, and the memory surface.

**Air-gap ready**: The `Hardened` air-gap mode cuts all network access and forces inference to a local llama.cpp endpoint.

**Captain's Key consent**: The Pilot cannot execute sensitive actions without a vault signature. This is enforced at the protocol level, not just the UI.
