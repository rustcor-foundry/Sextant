# Performance Log

This log keeps the current native browser timing checkpoints visible while tuning Servo, the render bridge, and the AI observation lanes.

## 2026-05-24

Environment: Windows debug build through `cargo run`, default Servo backend, production `sextant-browser` render path `BRIDGE`.

| Scenario | Mode | First draw | First frame | Nav | Distill | Wake | Frame total | Frame queue | Frame capture | Notes |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `https://example.com` + user DISTILL | Assisted | 30ms | 993ms | 417ms | 436ms | 52ms | 51ms | not in top 6 / `null` | 45ms | Current post-fix checkpoint from `sextant-mcp --window-smoke ... --user-distill --json`; JSON exposes `frameTotalMs=51`, `frameQueueMs=null`, `frameMs=45`. |
| `https://developer.mozilla.org/en-US/docs/Web/HTML` | Assisted | 33ms | 1.5s | 1.4s | n/a | 32ms | n/a | 0ms | 40ms | Bridge queue is no longer the bottleneck; Servo navigation dominates. |
| `https://www.google.com` | Direct | 30ms | 2.5s | 2.4s | n/a | n/a | n/a | 0ms | 66ms | Human-only path confirms Direct skips AI work; Servo navigation dominates. |

Recent movement:

- Before idle-refresh gating, assisted `example.com` user-DISTILL showed `frame-total` around 318ms with `frame-queue` around 272ms and capture around 29-37ms.
- After idle-refresh gating, the same path dropped to `frame-total` around 45-56ms with capture around 42ms and no visible queue pressure in the top slow events.

Current read:

- Render-bridge queue pressure is no longer the active problem on these checkpoints.
- The next large visible bottleneck is Servo navigation/page readiness: about 1.4s for MDN and about 2.4s for Google in the latest debug smokes.
