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
| `sextant-vault` | Citadel Vault — BIP-39 seed, Ed25519/ECDSA-P256 identities (custom HMAC-SHA512 derivation, not BIP-32), AES-256-GCM encryption, passphrase-verified unlock, persistent encrypted seed |
| `sextant-wake` | Digital Wake — SQLite FTS5 + vector memory store, temporal decay, consolidation |
| `sextant-inference` | Multi-backend LLM client — llama.cpp, OpenAI, Anthropic, Gemini |
| `sextant-log` | Captain's Log — SQLite audit trail with persona isolation |

### Security Layer

| Crate | Role |
|-------|------|
| `sextant-pq` | **SIMULATED** post-quantum identity — hash-based placeholder, NOT real ML-DSA; provides no cryptographic security (see crate docs) |
| `sextant-privacy` | PII redaction prototype — privacy levels (None / Standard / Strict); demo-grade pattern matching |
| `sextant-firewall` | Network filtering — persona domain rules (wildcard subdomain matching, IP-literal hosts), global blocklist |
| `sextant-airgap` | Air-gap mode — Online / Isolated / Hardened states; enforced in the browser shell via `SEXTANT_AIRGAP` and the guard path |

### Connectivity Layer

| Crate | Role |
|-------|------|
| `sextant-mesh` | **SIMULATED** P2P mesh — in-memory prototype, no real networking (libp2p integration is future work) |
| `sextant-bridge` | Multimodal bridge — image/audio/video input processing |
| `sextant-sync` | Cross-device sync prototype — persona payload serialization/merge; same-seed integrity MAC only, no cross-device key exchange yet |
| `sextant-bio` | **SIMULATED** biometric auth — no OS biometric integration; proofs are forgeable and cannot unlock a locked vault (see crate docs) |

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

The active UI is Rust-native. The earlier TypeScript/React UI mockup has been removed so product work stays focused on the native browser.

The active Rust UI is `sextant-browser`, a first-party shell that owns its event loop and pixel drawing through `winit` and `softbuffer`. In default-feature builds it uses `sextant-engine`'s Servo path for heavy browsing: WebView sessions, live navigation, frame capture, viewport resize, mouse/wheel/key forwarding, and live DOM distillation. In `--no-default-features` builds it stays in a lighter reader/fallback mode without Servo.

The top field is again becoming the planned Intent Bar, not only an address bar. Plain URLs and search strings still go straight through normal navigation. Explicit native intents, for example `intent: open https://example.com and distill`, now plan through `sextant-pilot`'s `PilotAction` vocabulary in default-feature builds before executing against the live browser engine, Wake, and Captain's Log state. Sensitive actions route to `RequestConsent`/`AWAITING CONSENT`; authorization now carries and resumes the bounded browser continuation behind the Captain's Key instead of dropping the plan. Safe navigation/distillation intents route to `Navigate`, `Distill`, and `Analyze` actions and report Pilot state in the Context Vault rail. The `--no-default-features` build keeps the deterministic shell-level fallback path for the lighter browser lane.

The active shell now has a first native perception surface. Distilled semantic maps are visible in the `SENSE` tab as a page type summary, semantic counts, key nodes, and distillation source metadata. Pilot `Perceive` actions route through the same surface, and MCP can request the same bounded perception pass through `browser_perception_probe`.

The active shell also has a first local trust-boundary surface. The `GUARD` tab reports the current persona, policy source, rule counts, air-gap mode, firewall decision for the active or target URL, privacy redaction status, and persisted local appliance certificate trust entries. Navigation now asks this guard before touching the engine, and scripted interactions check the resulting page URL after clicks/submits/fills. The same path is available headlessly through `--guard-probe <url-or-search>` plus optional `--guard-policy <json-path>`, and through MCP as `browser_guard_probe`, so agents can inspect allowed, audited, or blocked browser policy before or during automated work.

The old `sextant-hull-lite` binary name is retained only as a compatibility alias for existing scripts; the implementation lives in `src/browser.rs`.

The older Xilem/Masonry hull remains in the package as reference code for the async command/event architecture and validation ideas, but it is not the current Windows interactive product lane.

---

## Key Design Decisions

**Personas as trust boundary**: Every memory record, identity, and audit entry is scoped to a `persona_id`. Personas are cryptographic identities, not just UI names.

**Servo is optional but active for heavy browsing**: The `sextant-engine` crate has a `servo-backend` feature flag, and the hull package enables it by default. Without it, the browser shell keeps a fetch/distill reader path for fast checks and fallback work. With it, the engine runs a Servo service thread and exposes live navigation, history, frame capture, input forwarding, and live DOM distillation. This path exists and has focused tests, but real-window browsing is still buggy enough to need hardening.

**Captain's Key consent**: Pilot cannot execute destructive or sensitive actions without consent. The active browser shell now has a first native consent surface: sensitive Pilot actions create pending consent state, the Context Vault rail shows AUTHORIZE/DENY controls, authorization records a vault-backed consent signature in Captain's Log, and the browser-owned Pilot bridge resumes the gated continuation after authorization. Destructive or purchasing clicks remain gated; the resumed shell path is a bounded browser review/navigation/distillation plan.

**Local guard surface**: Security crates stay visible in the browser path instead of living only as lower-stack tests. `sextant-browser` now composes `sextant-airgap`, `sextant-firewall`, and `sextant-privacy` into a GUARD report for the active persona and page. The same surface now exposes local appliance certificate trust entries from `appliance-cert-trust.json` with selection, refresh, and revocation controls. The first enforcement layer blocks blacklisted navigation before engine work starts and treats a guard-blocked probe as a successful policy finding. Firewall policy can load as a JSON overlay from `guard-policy.json`, `SEXTANT_GUARD_POLICY`, or `--guard-policy`; overlay persona rules take precedence over built-in persona rules while the global blocklist remains a hard floor.

**Pilot action bridge before full orchestration**: The active browser shell currently uses the `PilotAction` command vocabulary directly for explicit intents while preserving the browser-owned engine/Wake/log objects. The next architecture step is to move this from an action bridge into full `SextantPilot` ownership with vault-backed plan signatures and consent resumption once the browser state can be safely shared with the Pilot runtime.

**Digital Wake decay**: Wake entries age via importance decay (5% per 7 days, pruned at 30 days / <0.2 importance). Semantic deduplication uses cosine similarity > 0.95 threshold.

**MSVC build**: Windows target requires VS Build Tools 2026. The `.cargo/config.toml` in `rust/` persists the linker and INCLUDE/LIB paths so any shell works without manual env setup.
