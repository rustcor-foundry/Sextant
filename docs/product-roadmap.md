# Product Roadmap — Sextant to Ship

**Thesis:** The AI browser category is real (Arc/Dia, Perplexity Comet, ChatGPT Atlas, Brave Leo, Opera Aria). All of them are cloud-dependent. None are sovereign. That gap is the wedge.

**Competitive anchor:** Brave captured ~80M monthly actives with a Chromium rewrap and a privacy narrative. Sextant is a deeper thesis — agentic + local-first + cryptographic identity — in a moment when users are actively looking for alternatives to cloud-AI defaults.

---

## Strategic Positioning

### What we are

> The first browser where the AI serves you, not a company. Your intent, your memory, your keys — on your machine. Cloud is an option you enable, not a dependency you inherit.

### What we are NOT

- A Chromium rewrap with a chat sidebar (Brave Leo, Opera, Edge Copilot)
- A cloud-native AI browser (Arc, Dia, Comet, Atlas)
- A privacy browser that ignores AI (Firefox, LibreWolf)
- An enterprise compliance tool (Island, Talon) — though that's a future wedge

### The three-line pitch

1. **Type an intent, the AI navigates for you** — agentic, not chat-sidebar
2. **Your browsing memory is yours** — encrypted, local, persona-scoped, searchable
3. **Works offline, works sovereign** — air-gap mode, post-quantum identity, local inference

---

## Phase 1 — MVP Launch (3-6 months)

**Goal:** Ship a working binary that a real user can download and experience the core loop.

### Ship criteria

- [ ] Windows + macOS binaries, < 100MB installer
- [ ] Single killer workflow end-to-end: **intent → Pilot → navigation → distill → Wake record → recall**
- [ ] Local inference via bundled llama.cpp (Qwen2.5-7B or similar, 4-5GB model)
- [ ] Gemini/OpenAI/Anthropic brains as opt-in cloud routes
- [ ] Persona create/switch with visible UX impact
- [ ] Wake search that feels fast and useful
- [ ] Captain's Log audit view (the "receipts" moment)
- [ ] One demo-worthy air-gap toggle

### Pragmatic engine decision — critical path

**Servo is a trap for a solo/small team.** It's a research project with an unstable API, painful Windows builds, and incomplete form/media support.

**Recommended:** Use `wry` (Tauri's webview wrapper) or `webview2-com` directly for Phase 1. Real pages render immediately, form input works, video plays. Keep the `sextant-engine` distillation layer as a post-processor on the rendered DOM.

Ship Servo integration in Phase 3 when the product has users and the investment is justified by a specific need (e.g., parallel layout for 100+ tabs). Until then, Servo is a differentiator we tell, not a feature users feel.

### What gets cut from current scope

- Servo integration (deferred to Phase 3)
- P2P mesh (deferred to Phase 2)
- Cross-device sync (deferred to Phase 2)
- Multimodal bridge (deferred to Phase 2)
- Hardware biometrics beyond OS prompt (deferred indefinitely)

### What MUST be in MVP

- Native performance feel (Xilem is fine, but the real hull must not stutter)
- Installer + auto-update infrastructure (Sparkle on macOS, custom on Windows)
- Crash reporting (Sentry, local-only first then opt-in remote)
- A website. Sextant needs a `.com` and a 3-minute demo video.

---

## Phase 2 — Public Beta + Community (6-12 months)

**Goal:** Grow from 100 early testers to 10k-50k actives. Build the moat.

### Product milestones

- Persona-as-marketing — ship "Work / Personal / Research" templates with pre-tuned privacy settings, memory scope, and agent permissions
- Air-gap mode as a **viral moment** — journalists, researchers, pentesters, compliance roles
- Mesh tab sharing on local network (Bonjour discovery, encrypted channel)
- Plugin/agent SDK — let community ship custom Pilots (research agent, shopping agent, news triage agent)
- Wake export + import as a feature, not just an API
- Benchmark story — "X seconds from intent to answer, locally" vs cloud competitors

### Distribution strategy

| Channel | Audience | Tactic |
|---------|----------|--------|
| Hacker News | Early adopters, OSS community | Launch post: "Sovereign AI browser in Rust, here's how the crypto vault works" |
| r/privacy, r/selfhosted | Privacy-pilled users | Air-gap mode demo |
| r/LocalLLaMA | Local AI enthusiasts | "Use your local Ollama as the Pilot brain" |
| X/Twitter dev community | Rust + AI builders | Technical threads on Pilot architecture, Wake memory |
| Podcast circuit | Broader tech | Positioning against Atlas/Dia as "the one that doesn't phone home" |
| YouTube demos | Mass tech | Short visual demos of intent → result loop |

### Community moat

- Open source core (MIT or Apache 2) with a clear CLA
- Public roadmap with user voting
- Contributor docs — this template is step one
- Discord/Matrix for persona template sharing

---

## Phase 3 — Scale + Revenue (12-24 months)

**Goal:** 1M+ actives, sustainable revenue, a path to 10M+.

### Revenue model options (pick 2)

1. **Sextant Sync** — optional end-to-end encrypted persona/wake sync across devices. $5-8/month. Recurring.
2. **Sextant Enterprise** — air-gap + compliance tier for regulated industries (legal, healthcare, finance, gov). $20-40/seat/month.
3. **Sextant Bridge** — pass-through API for cloud models with privacy-preserving routing (PII redaction layer as a service). Usage-based.
4. **Agent marketplace** — revenue share with agent developers (defer, complex)
5. **Token/rewards layer** — Brave's playbook. High reward, high narrative risk. Not Phase 3 material.

### Engineering investments

- Servo integration for tab-heavy power users (genuine parallelism story)
- Mobile — iOS (hard, WebKit-only) and Android (easier via Chromium)
- Real P2P mesh (libp2p) for true cross-network persona handoff
- Post-quantum as a real feature — sign every session, verify on peer reconnect

### Strategic moats

- **Personas as identity primitive** — once a user has 6 months of Wake history in "Research," switching costs are real
- **Local AI performance** — if we're 3x faster than cloud round-trip for common queries, that's sticky
- **Air-gap credibility** — one high-profile user (journalist, security researcher, compliance officer) is worth 100k ad dollars
- **Open source trust** — the one verifiable way to say "we don't phone home"

---

## Hard Questions Still Unanswered

1. **Funding path** — Bootstrapped? Open source + grants (Mozilla, NLnet, Protocol Labs)? VC (risky given the "sovereign" narrative)?
2. **Team size for Phase 1** — Solo is possible to MVP. Beta needs 2-4. Scale needs 10+.
3. **Rendering engine commitment** — Are we willing to drop the Servo thesis for shippability? (Recommendation: yes, for now.)
4. **Mobile story** — Critical for capturing Brave-scale share. iOS/Android is where attention lives. Not Phase 1, but the answer matters for positioning.
5. **Licensing** — MIT/Apache (maximum adoption) vs AGPL (prevent cloud rewrap)? Brave used MPL.
6. **Agent/Pilot trust model** — Who can ship a Pilot? Signed by us only? Community-signed? Web-of-trust? This is the OWASP top 10 of the AI era.

---

## Immediate Next Three Sessions

1. **Session A (this week)** — Get `cargo run` launching the hull. First end-to-end intent loop with cloud Gemini. Working demo video, even if rough.
2. **Session B** — Bundle llama.cpp binary and auto-launch it. First fully local intent loop. This is the "it actually works offline" moment.
3. **Session C** — Replace simulated engine with `wry` webview. Real page rendering, distillation on real DOM. This is the "it's a real browser" moment.

After those three sessions, Sextant is demoable and the Phase 1 scope becomes obvious.
