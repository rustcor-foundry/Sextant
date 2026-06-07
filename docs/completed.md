# Completed Work

Current high-signal history only. Older detailed session logs are archived under [archive/](archive/), including the pre-trim snapshots from May 24:

- [completed long snapshot](archive/2026-05-24-completed-raw-direct-long.md)
- [current-state long snapshot](archive/2026-05-24-current-state-raw-direct-long.md)
- [next-work long snapshot](archive/2026-05-24-next-work-raw-direct-long.md)
- [performance-log long snapshot](archive/2026-05-24-performance-log-raw-direct-long.md)

---

## 2026-05-24 - Raw Direct Servo Browser Lane

- Refactored the Servo `WindowRenderingContext` proof into shared `direct_servo` code used by both `sextant-servo-direct` and `sextant-browser --render-path direct`.
- Made Direct and Incognito the only valid modes for the raw direct path; Agent, Assisted, and Observe still use the bridge shell because AI observation/control surfaces live there.
- Added direct WebView user input: mouse move/click, wheel, printable text, IME commit text, common named keys, reload/back/forward shortcuts, `Ctrl+L` title-bar location mode, and keyboard tab controls.
- Added a verified direct input smoke that serves a controlled localhost page, clicks/focuses an input, types through Servo, and verifies page-observed text via title change.
- Added a direct form-submit smoke that types into a controlled search form, submits with Enter through Servo input, and verifies the submit handler through page title.
- Added a live direct search smoke that drives DuckDuckGo Lite through raw Servo keyboard input and verifies the result through page title.
- Added a live direct form smoke that drives a hosted `httpbin.org` HTML form through raw Servo click/type/submit and verifies the final `/post` URL.
- Added raw direct tab isolation: inactive tabs no longer wake the direct redraw loop, all direct WebViews resize on window resize, and focused tabs get a fresh resize.
- Added direct Incognito isolation: ephemeral Servo `config_dir`, HTTP cache disabled, MCP reporting for storage/cache state, and guarded cleanup for Windows-held files.
- Added MCP/window-smoke coverage for direct first present, input, first interaction, load complete, location navigation, history, reload, resize, tab switching, and Incognito storage.
- Added structured URL/title evidence for direct load, reload, location, history, search submit, and selected-tab smokes.

Latest direct smoke checkpoints:

| Scenario | Result |
|---|---:|
| `https://example.com` first direct present | `115-214ms` |
| `https://example.com` direct load complete | `563ms`, final `https://example.com/` / `Example Domain` |
| `https://www.google.com` first direct present | `149-182ms` |
| `https://www.google.com` direct load complete | `5.1s` |
| Google first interaction after first present | `6ms`, before load complete |
| Verified direct click/type fixture | `413ms`, page confirmed typed text |
| Verified direct form submit fixture | `304-520ms`, page confirmed `/search?q=sextant` URL/title |
| Live direct search smoke | `753ms-1.4s`, page confirmed `Sextant at DuckDuckGo` |
| Google live-search diagnostics | first present `136-179ms`, attempts tracked, timeout title/URL captured, submitted/blocked diagnostics parsed when available |
| Live direct hosted form smoke | `484ms`, DOM-located `input[name="custname"]`, submit click reached `https://httpbin.org/post` |
| Built-in input fixture first interaction | `5ms`, before load complete |
| Load-complete-gated direct input follow-up | about `1ms` |
| Direct location follow-up | about `464ms` |
| Direct history back / forward | about `31ms / 31ms` |
| Direct location final URL/title | `https://example.org/` / `Example Domain` |
| Direct history final URL/title | `https://example.org/` / `Example Domain` |
| Direct reload follow-up | `411ms`, final `https://example.com/` / `Example Domain` |
| Direct resize follow-up | about `27ms` |
| Direct tab follow-up | `134ms`, selected tab URL/title confirmed |
| Direct Incognito first present | about `150ms` |

---

## 2026-05-22 - Bridge Navigation And Perf Telemetry

- Added visible navigation phase telemetry, slowest/top perf-event reporting, and MCP structured parsing for navigation, frame queue, frame capture, resize, distillation, and Wake timing.
- Folded first bridge frame return into async visible navigation, cutting bridge first-frame checks for representative pages.
- Fixed redirected Servo URL waits, including the `www.rust-lang.org` to `rust-lang.org` case.
- Confirmed bridge queue pressure is no longer the main bottleneck on current checkpoints; Servo/page readiness and complex-page tail work are the remaining performance targets.

---

## Earlier Work

Older milestones covering the Xilem/Vello diagnostic ladder, native browser shell, guard/perception/perf tabs, operator workflows, Intent Bar, consent flow, persistence lane, visible input lane, and mode boundaries are preserved in:

- [archive/2026-05-24-completed-raw-direct-long.md](archive/2026-05-24-completed-raw-direct-long.md)
