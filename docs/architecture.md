# Architecture

## Core Principle

Sextant is a sovereign agentic browser — the user's machine is the trust boundary. No cloud dependency for core operation. All persona data, browsing history, cryptographic keys, and AI inference are local by default.

The system is built around a clean layered split:

- `sextant-hull` is the visible UI shell and event entry point
- `sextant-pilot` is the reasoning and orchestration brain
- `sextant-engine` handles page fetch, layout, and distillation
- `sextant-vault` is the cryptographic security root — all identity flows through it
- `sextant-wake` is persistent episodic memory (Digital Wake)
- supporting crates handle specialized concerns: inference, privacy, mesh, sync, etc.

---

## Crate Map

### Core Layer

| Crate | Role |
|-------|------|
| `sextant-hull` | Xilem UI shell — tabs, intent bar, viewport, captain's log |
| `sextant-pilot` | Agentic orchestration — intent reasoning, navigation, consent gating |
| `sextant-engine` | Page distillation engine — Servo rendering (optional), semantic extraction |
| `sextant-vault` | Citadel Vault — BIP-39 HD keys, Ed25519/ML-DSA identity, AES-256-GCM encryption |
| `sextant-wake` | Digital Wake — SQLite FTS5 + vector memory store, temporal decay, consolidation |
| `sextant-inference` | Multi-backend LLM client — llama.cpp, OpenAI, Anthropic, Gemini |
| `sextant-log` | Captain's Log — SQLite audit trail with persona isolation |

### Security Layer

| Crate | Role |
|-------|------|
| `sextant-pq` | Post-quantum identity — ML-DSA-65 signing via `pqcrypto-dilithium` |
| `sextant-privacy` | PII redaction — privacy levels (None / Standard / Strict) |
| `sextant-firewall` | Network filtering — domain rules, tracker blocking |
| `sextant-airgap` | Air-gap mode — Online / Isolated / Hardened states |

### Connectivity Layer

| Crate | Role |
|-------|------|
| `sextant-mesh` | P2P mesh — libp2p-based local network, tab/file sharing |
| `sextant-bridge` | Multimodal bridge — image/audio/video input processing |
| `sextant-sync` | Cross-device sync — persona payload serialization and merge |
| `sextant-bio` | Biometric auth — TouchID / FaceID proof abstraction |

---

## Dependency Graph (simplified)

```
sextant-hull
  ├── sextant-pilot
  │     ├── sextant-vault
  │     ├── sextant-engine
  │     ├── sextant-wake
  │     ├── sextant-inference
  │     └── sextant-log
  ├── sextant-mesh
  ├── sextant-airgap
  ├── sextant-privacy
  ├── sextant-pq
  ├── sextant-sync
  └── sextant-bio

sextant-engine
  └── sextant-privacy
```

---

## UI Layer

The TypeScript layer (`src/`) is a React 19 + Vite + Tailwind + shadcn simulator. It was the original AI Studio prototype and provides an interactive preview. It integrates Gemini via API key and has a `webLLMService.ts` stub for WebGPU local inference. It is **not** the production target — the Rust hull is.

The hull should be developed under the rules in [development-guardrails.md](development-guardrails.md), especially the Xilem layout guardrails: keep tuple-based view groups small, extract sub-view functions early, and avoid growing one large inline native view tree.

---

## Key Design Decisions

**Personas as trust boundary**: Every memory record, identity, and audit entry is scoped to a `persona_id`. Personas are cryptographic identities, not just UI names.

**Servo is optional**: The `sextant-engine` crate has a `servo-backend` feature flag. Without it, the engine uses a simulated distiller. This avoids the `libclang`/`bindgen` requirement at development time.

**Captain's Key consent**: Pilot cannot execute destructive or sensitive actions without a cryptographic signature from the vault. This is the `AwaitingConsent` state in `PilotStatus`.

**Digital Wake decay**: Wake entries age via importance decay (5% per 7 days, pruned at 30 days / <0.2 importance). Semantic deduplication uses cosine similarity > 0.95 threshold.

**MSVC build**: Windows target requires VS Build Tools 2026. The `.cargo/config.toml` in `rust/` persists the linker and INCLUDE/LIB paths so any shell works without manual env setup.
