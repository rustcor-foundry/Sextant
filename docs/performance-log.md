# Performance Log

Current timing checkpoints only. Older timing history is archived in [archive/2026-05-24-performance-log-raw-direct-long.md](archive/2026-05-24-performance-log-raw-direct-long.md).

Environment: Windows debug build, default Servo backend. Baseline rows below were captured through the built MCP/browser binaries with `SEXTANT_MCP_USE_INSTALLED_BROWSER=1`.

## QA Cleanup - 2026-06-06

Validation:

| Check | Result | Notes |
|---|---|---|
| Gitea freshness | Pass | `HEAD`, `origin/main`, and `origin/HEAD` all at `c138960`; ahead/behind `0/0`. |
| Legacy simulator removal | Done | Removed the TypeScript/Vite simulator, npm package files, and simulator docs references. |
| Guard appliance trust settings | Done | Guard tab lists persisted local appliance certificate trust entries and can forget a selected entry; focused browser test covers select/forget/persisted-store update. |
| Hosted certificate action smoke | Pass | Pylon `BACK` action requested/completed and landed on `about:blank`; Pylon `ONCE` action requested/completed/relaunched; Pylon `TRUST` action requested/completed/relaunched in an isolated temp profile and removed the temp profile afterward. All returned to fingerprint `5526c07eed59d052c7c48b7e8bdf1d3685b43e2b127de45e94abbdd2bcef8cb5` where applicable. |
| Google live search | Pass | DOM-located search-box click found `textarea[name="q"]` at `517,306`, submitted `/search?q=Sextant`, live-search frame `6.1s`, first present `156ms`. |
| Bing live search | Pass, no NodeList panic after local Servo patch | Unpatched backtrace showed `NodeList.hasOwn` via `Array.prototype.slice.call(nodeList)` panicking in `components/script/dom/node/nodelist.rs`; after applying `docs/patches/servo-nodelist-bing-panic.patch` to the local Servo checkout and rebuilding `servo-script`, Bing first-presented in `190ms`, submitted normally through the DOM-located `textarea[name="q"]`, reached `https://www.bing.com/search?q=Sextant...`, and reported a `6.3s` live-search frame with no `nodelist.rs` panic. |
| Hosted live form smoke | Pass | `--live-form-smoke` opened `https://httpbin.org/forms/post`, first-presented in `163ms`, DOM-located `input[name="custname"]`, clicked `button:not([type])`, reached `https://httpbin.org/post`, and reported `liveFormFrameMs=484`. |
| Direct browsing baseline | Pass | Required failures `0`; optional Pylon appliance case also passed. |
| Hosted direct smoke `example.com` | Pass | Parent shell ready, child first present `148ms`, active URL `https://example.com/`. |
| Assisted bridge `example.com --user-distill` | Pass | First Servo frame `936ms`; nav `330ms`, distill `520ms`, Wake `77ms`. |
| Direct first-interaction smoke `example.com` | Pass | First present `128ms`, first interaction frame `4ms`, before load complete. |
| Direct load smoke `example.com` | Pass | First present `159ms`, load complete `356ms`, title `Example Domain`. |
| Rust dependency audit | Improved / partial | Removed repo-controlled old advisory paths; `cargo audit` now reports only Servo-owned `ml-dsa 0.0.4` and `rsa 0.9.10` vulnerabilities plus warnings. |

Read: the repo is now native-only and current with Gitea before cleanup work continued. Dependency hardening remains the main non-functional follow-up.

## QA Pass - 2026-05-30

Validation:

| Check | Result | Notes |
|---|---|---|
| `npm run lint` | Pass | TypeScript clean. |
| `npm run build` | Pass | Fixed Tailwind v4 `@apply` issue, lazy-loaded heavyweight AI/tooling imports, and split Vite chunks; no chunk-size warning remains. Main app chunk is now `115.11 kB` minified / `34.44 kB` gzip. |
| `cargo fmt --all --check` | Pass | Rust formatting clean. |
| `cargo test --workspace --no-default-features` | Pass | Fixed feature-gating for Servo/guard tests that require `servo-backend` or `xilem-shell`. |
| `cargo test --workspace` | Pass | Default workspace tests green. |
| `cargo check -p sextant-hull --bin sextant-servo-direct` | Pass | Direct Servo binary checks. |
| `cargo test -p sextant-mcp -- --nocapture` | Pass | `41` MCP tests green. |

Smoke baselines:

| Scenario | First present/frame | Follow-up | Load complete | Max paint | Notes |
|---|---:|---:|---:|---:|---|
| Direct baseline `simple_load` / `https://example.com` | `162ms` | n/a | `477ms` | `6ms` | Required; title `Example Domain`; slow frames `0`. |
| Direct baseline `complex_load` / DuckDuckGo | `165ms` | n/a | `2.2s` | `1.3s` | Required; slow phase `loading`; `816` elements, `2,413` CSS rules, `46` scripts, `76` resources. |
| Direct baseline `appliance_load` / Pylon | `143ms` | n/a | `262ms` | `3ms` | Optional local appliance case; title `Login - Pylon`; local TLS override path still works. |
| Direct baseline `live_search` / DuckDuckGo Lite | `155ms` | `841ms` | n/a | `582ms` | Required; title `Sextant at DuckDuckGo`; slow phase `idle`. |
| Direct baseline `live_form` / httpbin | `144ms` | `492ms` | n/a | `762ms` | Required; DOM-located `input[name="custname"]`, submitted to `https://httpbin.org/post`; slow phase `idle`. |
| Assisted bridge `example.com --user-distill` | shell draw `4ms` / Servo frame `758ms` | n/a | n/a | n/a | Nav `277ms`, distill `409ms`, Wake `60ms`, frame `23ms`. |

Viewport and complexity read:

| Suite | Result |
|---|---|
| Direct viewport baseline | DuckDuckGo max loading paint: `838ms` at `640x420`, `859ms` at `960x620`, `1.1s` at `1180x760`; required failures `0`. |
| Direct complexity baseline | `example.com` max paint `403ms` audit-only, DuckDuckGo Lite `4ms`, DuckDuckGo full `969ms` loading paint, MDN `789ms` loading paint; required failures `0`. |

Current QA read: correctness is green; after the follow-up below, the frontend bundle warning is closed. The main product risk remains complex-page Servo loading paint and the user-facing direct chrome/composition lane.

## QA Gap Follow-Up - 2026-05-30

Changes:

- Frontend first-load bundle reduced by lazy-loading Gemini and removing unused static MCP/WebLLM SDK imports. Vite now emits `index` at `115.11 kB` minified / `34.44 kB` gzip, plus split `vendor-react`, `vendor-google-genai`, `vendor-motion`, and `vendor-icons` chunks.
- Direct Servo now paces non-input frame-ready redraws at a slower cadence while a page is still loading, while keeping input-pump frames immediate.

Validation:

| Check | Result |
|---|---|
| `npm run lint` | Pass |
| `npm run build` | Pass, no chunk-size warning |
| `cargo fmt --all --check` | Pass |
| `cargo check -p sextant-hull --bin sextant-servo-direct` | Pass |
| `cargo test -p sextant-hull --bin sextant-servo-direct -- --nocapture` | Pass, `3/3` |
| `cargo test -p sextant-hull --bin sextant-browser -- --nocapture` | Pass, `74/74` |
| `cargo test -p sextant-mcp -- --nocapture` | Pass, `41/41` |

Smoke results after pacing:

| Scenario | First present/frame | Follow-up | Load complete | Max paint | Notes |
|---|---:|---:|---:|---:|---|
| Direct baseline `simple_load` / `https://example.com` | `270ms` | n/a | `581ms` | `3ms` | Required; no slow frames. |
| Direct baseline `complex_load` / DuckDuckGo | `114ms` | n/a | `1.6s` | `799ms` | Required; loading paint improved from the prior `936ms` baseline run. |
| Direct baseline `appliance_load` / Pylon | `110ms` | n/a | `249ms` | `3ms` | Optional local appliance case still passes. |
| Direct baseline `live_search` / DuckDuckGo Lite | `141ms` | `703ms` | n/a | `438ms` | Required; improved from `767ms` follow-up and `542ms` max paint. |
| Direct complexity `DuckDuckGo full` at `960x620` | `138ms` | n/a | `1.7s` | `818ms` | Required; `19` frames, one slow loading frame. |
| Direct complexity `MDN article` at `960x620` | `137ms` | n/a | `1.6s` | `903ms` | Still bouncy; complex article scene remains a Servo/WebRender tuning target. |
| Direct viewport DuckDuckGo `640x420` / `960x620` / `1180x760` | `108ms` / `150ms` / `152ms` | n/a | `1.6-1.7s` | `891ms` / `833ms` / `837ms` | Required failures `0`; viewport size is still not the primary driver. |

Read: frame pacing cut some redraw churn and moved DuckDuckGo's complex loading paint into the high-700/low-800ms band in the main baseline, but it did not eliminate the core heavy-scene cost. The remaining performance work is under Servo/WebRender scene construction/paint, plus direct chrome composition.

## Hosted Direct Chrome - 2026-05-30

Direct/Incognito feature work after QA:

- The direct parent shell now exposes real browser chrome around the embedded Servo child window: address input, load progress, back/forward/reload, new tab, previous tab, next tab, and close tab.
- The Servo child reports chrome state back through its hosted log stream: active tab index/count, active title, active URL, and back/forward availability.
- The parent shell parses that state and renders active tab/title status plus disabled button states without enabling AI observation or persistence in Direct/Incognito.
- The Servo child now reports local appliance certificate fingerprints to the hosted log stream; the parent shell surfaces native `ONCE` / `TRUST` actions and relaunches trusted appliance sessions back into the embedded child window instead of falling through to the old standalone relaunch path.
- Hosted direct certificate chrome now includes a native `BACK` action that leaves the warning without creating trust, and the profile trust store has list/forget hooks via `--list-local-appliance-certs`, `--forget-local-appliance-cert <url|all>`, MCP `browser_local_appliance_cert_list`, and MCP `browser_local_appliance_cert_forget`.
- MCP now exposes `browser_hosted_direct_smoke`, and `sextant-browser` supports `--hosted-direct-smoke` for a bounded parent-shell proof that exits after first-present evidence from the embedded child.

Validation:

| Check | Result |
|---|---|
| `cargo fmt --all --check` | Pass |
| `cargo check -p sextant-hull --bin sextant-browser` | Pass |
| `cargo test -p sextant-hull --bin sextant-browser hosted_direct -- --nocapture` | Pass, `4/4` |
| `cargo test -p sextant-hull --bin sextant-servo-direct -- --nocapture` | Pass, `3/3` |
| `cargo test -p sextant-hull --bin sextant-browser -- --nocapture` | Pass, `74/74` |
| `cargo test -p sextant-mcp -- --nocapture` | Pass, `43/43` |
| Direct raw load smoke `example.com` | Pass, first present `141ms`, load complete `397ms`, max paint `3ms` |
| Hosted direct smoke `example.com` | Pass, parent shell ready, embedded child PID reported, first present `156ms`, active URL `https://example.com/` |

Read: hosted direct is becoming the product path for normal user browsing. The remaining direct-chrome work is manual QA, visual polish, certificate settings/revocation, and richer hosted-shell smoke coverage.

## Direct Browsing Baseline - 2026-05-29

Command:

```powershell
.\target\debug\sextant-mcp.exe --direct-browsing-baseline --timeout-seconds 75 --json
```

| Scenario | Mode | First present/frame | Follow-up | Load complete | Notes |
|---|---|---:|---:|---:|---|
| baseline `simple_load` / `https://example.com` | Direct | `162ms` | n/a | `477ms` | Title `Example Domain`; `12` frames, max direct frame `7ms`, slow frames `0`. |
| baseline `complex_load` / DuckDuckGo | Direct | `165ms` | n/a | `2.2s` | `23` frames; max direct frame `1.3s`, slow frames `1`, phase `loading`; resource audit found `18` images, `7` broken images, `40` inline SVGs, `33` zero-size SVGs, `10` stylesheets, `0` canvases. |
| baseline `appliance_load` / Pylon | Direct | `143ms` | n/a | `262ms` | Local TLS bypass case; title `Login - Pylon`; `11` frames; optional by default so offline appliances do not fail the whole baseline. |
| baseline `live_search` / DuckDuckGo Lite | Direct | `155ms` | `841ms` | n/a | Title `Sextant at DuckDuckGo`; `26` frames after ready-frame pacing; max direct frame `584ms`, slow frames `1`, phase `idle`. |
| baseline `live_form` / httpbin | Direct | `144ms` | `492ms` | n/a | DOM-located `input[name="custname"]`, clicked `button:not([type])`, reached `https://httpbin.org/post`; `26` frames, max direct frame `763ms`, slow frames `1`, phase `idle`. |

Required failures: none.

## Direct Viewport Baseline - 2026-05-29

Command:

```powershell
.\target\debug\sextant-mcp.exe --direct-viewport-baseline --timeout-seconds 75 --json
```

| Scenario | Viewport | First present | Load complete | Max paint | Notes |
|---|---:|---:|---:|---:|---|
| DuckDuckGo complex load | `640x420` | `113ms` | `1.7s` | `912ms` | `22` frames, slow phase `loading`; `780` elements, `82` interactive, `2,904` CSS rules, `48` scripts, `74` resources. |
| DuckDuckGo complex load | `960x620` | `117ms` | `1.8s` | `963ms` | `23` frames, slow phase `loading`; same complexity counters. |
| DuckDuckGo complex load | `1180x760` | `139ms` | `1.6s` | `843ms` | `22` frames, slow phase `loading`; same complexity counters. |

Read: the loading-phase heavy paint does not scale linearly with viewport pixel area in this debug run. DuckDuckGo consistently presents a heavy scene: about `780` elements, `2,904` accessible CSS rules, `48` scripts, `42` inline SVGs, and `74` resource timing entries. That points more toward Servo/WebRender scene/content work than raw fill-rate.

## Direct Complexity Baseline - 2026-05-30

Command:

```powershell
.\target\debug\sextant-mcp.exe --direct-complexity-baseline --timeout-seconds 75 --json
```

Viewport: `960x620`.

| Scenario | First present | Load complete | Max paint | Loading paint | Audit paint | Complexity |
|---|---:|---:|---:|---:|---:|---|
| example.com | `175ms` | `919ms` | `482ms` | n/a | `482ms` | `11` elements, `4` CSS rules, `0` scripts, `0` resources. |
| DuckDuckGo Lite | `166ms` | `643ms` | `3ms` | n/a | n/a | `27` elements, `0` CSS rules, `0` scripts, `2` resources. |
| DuckDuckGo full | `130ms` | `1.8s` | `890ms` | `890ms` | n/a | `780` elements, `2,904` CSS rules, `48` scripts, `74` resources. |
| MDN HTML article | `130ms` | `1.6s` | `811ms` | `811ms` | n/a | `1,553` elements, `359` CSS rules, `6` scripts, `37` resources. |

Read: phase-specific metrics separate audit/tooling cost from real loading paint. The remaining user-visible risk is concentrated in complex loading scenes, not simple pages or DDG Lite.

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
- The new direct browsing baseline gives us one repeatable MCP command for simple load, complex page, local appliance TLS, and live search.
- Direct ready-frame pacing now caps non-input redraw churn; live-search smoke dropped from thousands of frames to `26` while still completing.
- `loadCompleteMs` is now the complex-page tail metric and should not be confused with perceived responsiveness.
- The direct viewport baseline suggests the remaining DuckDuckGo heavy paint is not primarily viewport pixel-area cost.
- The direct complexity baseline now distinguishes `maxLoadingPaintMs` from `maxPostLoadAuditPaintMs`; simple pages can show audit-only paint, while DuckDuckGo full and MDN show true loading-phase paint spikes.
- Google can first-present under 200ms and accept an immediate follow-up interaction frame in about 6ms, while full load completion can stretch to about 5.1s.
- DuckDuckGo now resource-audits cleanly for images on the direct path, with zero broken images in the latest run; the remaining visible/rendering clue is a large number of zero-size inline SVG nodes.
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
