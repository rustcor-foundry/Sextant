# Performance Log

This log keeps the current native browser timing checkpoints visible while tuning Servo, the render bridge, and the AI observation lanes.

## 2026-05-24

Environment: Windows debug build through `cargo run`, default Servo backend, production `sextant-browser` render path `BRIDGE`.

| Scenario | Mode | First draw | First frame | Nav | Distill | Wake | Frame total | Frame queue | Frame capture | Notes |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `https://example.com` + user DISTILL | Assisted | 30ms | 954ms | 388ms | 426ms | 56ms | 56ms | not in top 6 / `null` | 41ms | Current post-fix checkpoint from `sextant-mcp --window-smoke ... --user-distill --json`; immediate distill still succeeds after faster visible navigation settle. |
| `https://developer.mozilla.org/en-US/docs/Web/HTML` | Assisted | 32ms | 763ms | 651ms | n/a | 33ms | n/a | n/a | 22ms | Visible load settle capped at ~251ms; `navUrlWaitMs` 277ms and `navLoadWaitMs` 251ms. |
| `https://www.google.com` | Direct | 28ms | 756ms | 647ms | n/a | n/a | n/a | n/a | 22ms | Human-only path confirms Direct skips AI work; visible load settle capped at ~251ms. |

Recent movement:

- Before idle-refresh gating, assisted `example.com` user-DISTILL showed `frame-total` around 318ms with `frame-queue` around 272ms and capture around 29-37ms.
- After idle-refresh gating, the same path dropped to `frame-total` around 45-56ms with capture around 42ms and no visible queue pressure in the top slow events.
- Before visible navigation settle tuning, MDN first frame was about 1.2-1.5s and Google Direct first frame was about 2.5s.
- After visible navigation settle tuning, MDN first frame is about 763ms and Google Direct first frame is about 756ms; synchronous operator/proof navigation keeps the longer settle path.

Current read:

- Render-bridge queue pressure is no longer the active problem on these checkpoints.
- The next visible bottleneck is the remaining Servo URL-progress wait, around 266-277ms on the current MDN/Google checks, plus normal page-specific script/render work after the first frame.
