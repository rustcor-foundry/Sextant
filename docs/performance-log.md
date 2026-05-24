# Performance Log

This log keeps the current native browser timing checkpoints visible while tuning Servo, the render bridge, and the AI observation lanes.

## 2026-05-24

Environment: Windows debug build through `cargo run`, default Servo backend, production `sextant-browser` render path `BRIDGE`.

| Scenario | Mode | First draw | First frame | Nav | Distill | Wake | Frame total | Frame queue | Frame capture | Notes |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `https://example.com` + user DISTILL | Assisted | 46ms | 977ms | 402ms | 422ms | 50ms | 50ms | not in top 6 / `null` | 46ms | Current post-first-frame-return checkpoint from `sextant-mcp --window-smoke ... --user-distill --json`; immediate distill still succeeds after faster visible navigation settle. |
| `https://developer.mozilla.org/en-US/docs/Web/HTML` | Assisted | 30ms | 485ms | 395ms | n/a | 27ms | n/a | n/a | 37ms | Visible load now returns on first real frame; `navUrlWaitMs` 259ms and `navLoadWaitMs` 14ms. |
| `https://www.google.com` | Direct | 50ms | 518ms | 383ms | n/a | n/a | n/a | n/a | 29ms | Human-only path confirms Direct skips AI work; visible load now returns on first real frame with `navLoadWaitMs` 15ms. |
| `https://www.google.com` | Direct | 22ms | 549ms | 470ms | n/a | n/a | n/a | n/a | 22ms | Post input-lane settle checkpoint; warmed run after rebuild. `navUrlWaitMs` was 334ms and `navLoadWaitMs` was 15ms. A prior cold/noisy Google run in the same pass hit 1.0s first frame because URL wait spiked to 748ms, while bridge capture stayed 28ms. |
| built-in input fixture + input latency | Direct | 31ms | 302ms | 213ms | n/a | n/a | 41ms | n/a | 26-32ms | Input-settle wake checkpoint through MCP `browser_window_smoke` / CLI `--input-latency`. Viewport input enqueue reported `0ms`; follow-up frame after typed input improved to `65ms` after shortening the input-settle window to 16ms. |

Recent movement:

- Before idle-refresh gating, assisted `example.com` user-DISTILL showed `frame-total` around 318ms with `frame-queue` around 272ms and capture around 29-37ms.
- After idle-refresh gating, the same path dropped to `frame-total` around 45-56ms with capture around 42ms and no visible queue pressure in the top slow events.
- Before visible navigation settle tuning, MDN first frame was about 1.2-1.5s and Google Direct first frame was about 2.5s.
- After visible navigation settle tuning, MDN first frame is about 763ms and Google Direct first frame is about 756ms; synchronous operator/proof navigation keeps the longer settle path.
- After first-frame return for visible navigation, MDN first frame is about 485ms and Google Direct first frame is about 518ms; the old ~250ms visible load settle wait is now about 14-15ms when Servo has painted a fresh frame.
- After input-lane settle tuning, warmed Google Direct is still in the same band at about 549ms first frame, with bridge capture down around 22ms. The change is aimed at keyboard feel: text input no longer rearms the bridge before it is flushed to Servo, and frame refresh waits a short 24ms settle window after viewport input enqueue.
- The first input-latency smoke gave us a separate keyboard-lane baseline: handoff from the shell to Servo is effectively immediate (`0ms` in the visible smoke), while the user-visible follow-up frame was about 137ms on the centered input fixture.
- After waking frame refresh at the input-settle deadline, the same input fixture improved to about 77ms for the follow-up frame. Shortening the settle window from 24ms to 16ms moved it again to about 65ms. That remaining time is now mostly settle + render bridge capture + redraw polling.

Current read:

- Render-bridge queue pressure is no longer the active problem on these checkpoints.
- The next visible bottleneck is the remaining Servo URL-progress wait, around 250-335ms on warmed MDN/Google checks and occasionally higher on noisy Google runs, plus normal page-specific script/render work after the first frame.
- A tighter 2ms active Servo poll interval was tested and not kept; it did not reliably improve URL wait and made the signal noisier.
