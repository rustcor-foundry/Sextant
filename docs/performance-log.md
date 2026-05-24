# Performance Log

This log keeps the current native browser timing checkpoints visible while tuning Servo, the render bridge, and the AI observation lanes.

## 2026-05-24

Environment: Windows debug build through `cargo run`, default Servo backend, production `sextant-browser` render path `BRIDGE`.

| Scenario | Mode | First draw | First frame | Nav | Distill | Wake | Frame total | Frame queue | Frame capture | Notes |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `https://example.com` + user DISTILL | Assisted | 46ms | 977ms | 402ms | 422ms | 50ms | 50ms | not in top 6 / `null` | 46ms | Current post-first-frame-return checkpoint from `sextant-mcp --window-smoke ... --user-distill --json`; immediate distill still succeeds after faster visible navigation settle. |
| `https://developer.mozilla.org/en-US/docs/Web/HTML` | Assisted | 30ms | 485ms | 395ms | n/a | 27ms | n/a | n/a | 37ms | Visible load now returns on first real frame; `navUrlWaitMs` 259ms and `navLoadWaitMs` 14ms. |
| `https://www.google.com` | Direct | 50ms | 518ms | 383ms | n/a | n/a | n/a | n/a | 29ms | Human-only path confirms Direct skips AI work; visible load now returns on first real frame with `navLoadWaitMs` 15ms. |

Recent movement:

- Before idle-refresh gating, assisted `example.com` user-DISTILL showed `frame-total` around 318ms with `frame-queue` around 272ms and capture around 29-37ms.
- After idle-refresh gating, the same path dropped to `frame-total` around 45-56ms with capture around 42ms and no visible queue pressure in the top slow events.
- Before visible navigation settle tuning, MDN first frame was about 1.2-1.5s and Google Direct first frame was about 2.5s.
- After visible navigation settle tuning, MDN first frame is about 763ms and Google Direct first frame is about 756ms; synchronous operator/proof navigation keeps the longer settle path.
- After first-frame return for visible navigation, MDN first frame is about 485ms and Google Direct first frame is about 518ms; the old ~250ms visible load settle wait is now about 14-15ms when Servo has painted a fresh frame.

Current read:

- Render-bridge queue pressure is no longer the active problem on these checkpoints.
- The next visible bottleneck is the remaining Servo URL-progress wait, around 248-259ms on the current MDN/Google checks, plus normal page-specific script/render work after the first frame.
- A tighter 2ms active Servo poll interval was tested and not kept; it did not reliably improve URL wait and made the signal noisier.
