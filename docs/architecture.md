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
| `sextant-hull` | Native UI shell package. Active lane is `sextant-browser` (`winit` + `softbuffer`); Xilem shell is parked reference code |
| `sextant-pilot` | Agentic orchestration — intent reasoning, navigation, consent gating |
| `sextant-engine` | Browser engine surface — optional Servo live navigation/frame capture/input, reader fallback, and semantic extraction |
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

The active Rust UI is `sextant-browser`, a first-party shell that owns its event loop and pixel drawing through `winit` and `softbuffer`. In default-feature builds it uses `sextant-engine`'s Servo path for heavy browsing: WebView sessions, live navigation, frame capture, viewport resize, mouse/wheel/key forwarding, and live DOM distillation. In `--no-default-features` builds it stays in a lighter reader/fallback mode without Servo.

The top field is again becoming the planned Intent Bar, not only an address bar. Plain URLs and search strings still go straight through normal navigation. Explicit native intents, for example `intent: open https://example.com and distill`, run through a deterministic shell-level intent loop that resolves a target, navigates through the engine, distills into Wake when requested, records Captain's Log entries, and reports Pilot state in the Context Vault rail. This is intentionally thin so it can be replaced by deeper `sextant-pilot` ownership once the Servo browser lane is stable.

The old `sextant-hull-lite` binary name is retained only as a compatibility alias for existing scripts; the implementation lives in `src/browser.rs`.

The older Xilem/Masonry hull remains in the package as reference code for the async command/event architecture and validation ideas, but it is not the current Windows interactive product lane.

---

## Key Design Decisions

**Personas as trust boundary**: Every memory record, identity, and audit entry is scoped to a `persona_id`. Personas are cryptographic identities, not just UI names.

**Servo is optional but active for heavy browsing**: The `sextant-engine` crate has a `servo-backend` feature flag, and the hull package enables it by default. Without it, the browser shell keeps a fetch/distill reader path for fast checks and fallback work. With it, the engine runs a Servo service thread and exposes live navigation, history, frame capture, input forwarding, and live DOM distillation. This path exists and has focused tests, but real-window browsing is still buggy enough to need hardening.

**Captain's Key consent**: Pilot cannot execute destructive or sensitive actions without a cryptographic signature from the vault. This is the `AwaitingConsent` state in `PilotStatus`.

**Digital Wake decay**: Wake entries age via importance decay (5% per 7 days, pruned at 30 days / <0.2 importance). Semantic deduplication uses cosine similarity > 0.95 threshold.

**MSVC build**: Windows target requires VS Build Tools 2026. The `.cargo/config.toml` in `rust/` persists the linker and INCLUDE/LIB paths so any shell works without manual env setup.
