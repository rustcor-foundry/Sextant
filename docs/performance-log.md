# Performance Log

Current timing checkpoints only. Older timing history is archived in [archive/2026-05-24-performance-log-raw-direct-long.md](archive/2026-05-24-performance-log-raw-direct-long.md).

Environment: Windows debug build through `cargo run`, default Servo backend.

## Current Direct Metrics

| Scenario | Mode | First present/frame | Follow-up | Load complete | Notes |
|---|---|---:|---:|---:|---|
| `https://example.com --render-path direct` | Direct | `115-214ms` | n/a | n/a | Raw direct first-present range across recent smokes. |
| `https://example.com --load-smoke` | Direct | `138ms` | n/a | `563ms` | Final URL/title confirmed as `https://example.com/` / `Example Domain`. |
| `https://www.google.com --load-smoke` | Direct | `169ms` | n/a | `5.1s` | Complex-page tail is long even when first present is fast. |
| input fixture `--first-interaction-smoke` | Direct | `140ms` | `5ms` | not waited | Interaction happened before load complete. |
| Google `--first-interaction-smoke` | Direct | `149ms` | `6ms` | not waited | Confirms snappy early interaction before load complete. |
| localhost fixture `--verified-input-smoke` | Direct | `129ms` | `413ms` | not waited | Clicked/focused/typed; page title confirmed `typed:sextant`. |
| localhost fixture `--search-submit-smoke` | Direct | `154-196ms` | `304-520ms` | not waited | Typed into form and submitted with Enter; `/search?q=sextant` URL/title confirmed. |
| DuckDuckGo Lite `--live-search-smoke` | Direct | `142-205ms` | `753ms-1.4s` | not waited | Keyboard focus traversal, typed/submitted `Sextant`; title confirmed `Sextant at DuckDuckGo`. |
| Google `--live-search-smoke` | Direct | `136-179ms` | partial / blocked | n/a | Attempts now include keyboard, search-box click, focus sweep, and center click; MCP reports timeout title/URL plus submitted/blocked diagnostics when a challenge/search URL surfaces. |
| input fixture `--input-latency` | Direct | `134-172ms` | `~1ms` | gated first | Load-complete-gated direct input follow-up. |
| direct tab smoke | Direct | `214ms` | `134ms` | n/a | Opens second WebView directly at target, switches away/back, waits for selected tab URL/title. |
| direct location smoke | Direct | `181ms` | `625ms` | gated first | Exercises title-bar location/search path; final `https://example.org/` / `Example Domain`. |
| direct history smoke | Direct | `174ms` | back/forward `11ms / 22ms` | gated first | Exercises page A -> B -> back -> forward; final `https://example.org/` / `Example Domain`. |
| direct reload smoke | Direct | `150ms` | `411ms` | gated first | Exercises Servo reload path; final URL/title confirmed as `https://example.com/` / `Example Domain`. |
| direct resize smoke | Direct | `168ms` | `27ms` | n/a | Native resize to 640x420 and WebView resize propagation. |
| `https://example.com --mode incognito --render-path direct` | Incognito | `150ms` | n/a | n/a | Ephemeral Servo config dir and HTTP cache disabled. |

## Bridge Shell Reference

| Scenario | Mode | First draw | First frame | Notes |
|---|---|---:|---:|---|
| `https://example.com --user-distill` | Assisted | `46ms` | `977ms` | Distill about `422ms`, Wake about `50ms`, frame total about `50ms`. |
| MDN visible smoke | Assisted | `30ms` | `485ms` | `navUrlWaitMs` about `259ms`, `navLoadWaitMs` about `14ms`. |
| Google bridge visible smoke | Direct | `22-50ms` | `518-549ms` | Bridge capture about `22-29ms`; Direct mode still uses bridge here unless `--render-path direct`. |

## Current Read

- The raw direct path has the right user-speed split: first present and first interaction are fast.
- `loadCompleteMs` is now the complex-page tail metric and should not be confused with perceived responsiveness.
- Google can first-present under 200ms and accept an immediate follow-up interaction frame in about 6ms, while full load completion can stretch to about 5.1s.
- Verified input now proves real click/focus/text receipt on a controlled localhost page; data URL fixtures painted but produced closed-pipeline hit-test failures, so verified input uses HTTP.
- Search-submit smoke now proves Enter-driven form navigation on the direct path and reports the final URL/title; fixture HTTP close handling no longer produces the prior late `UnexpectedMessage` warning on the latest run.
- Live-search smoke now proves a representative public search page can be driven through raw Servo keyboard input. The first attempted mouse-coordinate version failed with empty hit-test results, so the smoke now uses keyboard focus traversal, search-box click, focus sweep, and center-click fallback for harder pages.
- Google is now a tracked complex-page/anti-automation lane rather than an opaque timeout: first present is fast, and MCP separates no-input failures from submitted-but-unverified or challenge/unsupported-browser outcomes.
- Load, reload, location, history, search-submit, and tab smokes now report final URL/title evidence, so direct navigation proofs are comparable across title-bar navigation, form submit, back/forward, reload, and tab selection.
- Bridge queue pressure is no longer the active bottleneck on current assisted/visible checkpoints.
- Next tuning should move verified input/search submit toward live pages and then direct chrome composition.

## Timing Rule

When adding new performance work:

1. Record first present / first frame.
2. Record first meaningful interaction when relevant.
3. Record load complete separately.
4. Record whether the path uses bridge readback or direct Servo presentation.
