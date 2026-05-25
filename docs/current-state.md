# Current State

Snapshot date: `2026-05-24`

This file is the short working snapshot. Detailed older state is archived in [archive/2026-05-24-current-state-raw-direct-long.md](archive/2026-05-24-current-state-raw-direct-long.md).

## Executive Summary

Sextant's active product lane is the first-party Rust native browser, `sextant-browser`.

There are now two user-render paths:

- `--render-path bridge`: current full shell path with chrome, tabs, AI observation surfaces, Wake, Log, Guard, Sense, Perf, Validation, and frame-capture display through Softbuffer.
- `--render-path direct`: raw Servo `WindowRenderingContext` path for Direct/Incognito URL/search browsing, bypassing frame readback and AI observation.

The direct lane is starting to feel like a real browser: first pixels arrive quickly, early interaction is crisp, and slow complex-page load completion is now measured separately instead of being confused with perceived responsiveness.

## Working Now

| Area | State | Notes |
|---|---|---|
| Canonical binary | Working | `sextant-browser` is the native product binary. |
| Bridge shell | Working / tuning | Full chrome/tabs/AI path still uses the frame-capture bridge. |
| Raw direct lane | Experimental / promising | `sextant-browser --render-path direct --mode direct|incognito`. |
| Direct first present | Fast | Google and example.com regularly first-present under 200ms in debug. |
| Direct first interaction | Fast | Google follow-up interaction frame about 6ms after first present, before load complete. |
| Direct load completion | Measured tail | Google can take about 5.1s to report `LoadStatus::Complete`. |
| Direct controls | Baseline | Input, verified click/type, verified form submit, live search, location, tabs, reload, back/forward, resize, Incognito storage all have bounded smokes with URL/title evidence where relevant. |
| MCP browser layer | Working | Tools/resources advertise modes, render paths, smokes, perf probes, guard/perception, and operator flows. |
| AI modes | Working boundary | Agent/Assisted/Observe use bridge shell; Direct/Incognito disable AI observation/persistence. |
| Reader fallback | Working | `sextant-browser --no-default-features` remains the fast fallback lane. |

## Key Direct Metrics

| Metric | Latest useful checkpoint |
|---|---:|
| Google first direct present | `149ms` |
| Google first interaction frame | `6ms` before load complete |
| Verified direct input | `413ms`, clicked/focused/typed through live localhost fixture |
| Verified form submit | `304-520ms`, typed, submitted, and verified `/search?q=sextant` URL/title |
| Live direct search | `753ms-1.4s`, DuckDuckGo Lite title confirmed `Sextant at DuckDuckGo` |
| Google live search | Partial / blocked | First present `136-179ms`; MCP now separates no-input, submitted-unverified, and challenge/unsupported-browser evidence via attempts, timeout title, and timeout URL. |
| Google load complete | `5.1s` |
| example.com first direct present | `138ms` on latest load smoke |
| example.com load complete | `563ms`, final `https://example.com/` / `Example Domain` |
| Direct resize follow-up | `27ms` |
| Direct reload follow-up | `411ms`, final `https://example.com/` / `Example Domain` |
| Direct history back/forward | `31ms / 31ms` |
| Direct location final evidence | `625ms`, final `https://example.org/` / `Example Domain` |
| Direct history final evidence | `660ms`, final `https://example.org/` / `Example Domain` |
| Direct tab follow-up | `134ms`, selected tab URL/title confirmed |

## Current Interpretation

The raw direct lane proves the main architectural bet: Servo can render to the real native window fast enough to feel browser-like when the frame-capture bridge is bypassed.

The important split is now explicit:

- `directPresentMs` and `firstInteractionFrameMs` represent user-perceived speed.
- `loadCompleteMs` represents complex-page tail work.

That split is why Google can feel snappy even while Servo continues background page work for several seconds.

## Known Gaps

| Gap | Priority | Notes |
|---|---|---|
| Direct shell composition | High | Need chrome/tabs/address overlay around the direct Servo window path. |
| Live-site direct interaction depth | High | Controlled click/type and form-submit proof works; next step is representative live search/input behavior. |
| Complex-page Servo tail | High | Google load-complete tail remains long; now measurable separately. |
| Bridge shell polish | High | Full Agent/Assisted shell still depends on bridge and should stay stable. |
| Manual QA pass | High | Run the actual debug browser and validate normal browsing feel. |
| Xilem/Vello | Parked | Diagnostic ladder remains archived/reference until direct path is further along. |

## Practical Recommendation

Next work should keep moving down [next-work.md](next-work.md):

1. Broaden live-page search/input proof beyond DuckDuckGo Lite and capture failures by site.
2. Start designing the direct chrome/tab/address overlay path.
3. Keep bridge shell smokes green while direct grows into the main user path.
4. Use `performance-log.md` for timing deltas and avoid relying on feel alone.
