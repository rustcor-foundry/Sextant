# Current State

Snapshot date: `2026-06-14`

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
| Hosted direct lane | Working / polishing | `sextant-browser --render-path direct --mode direct|incognito` launches a parent Sextant chrome window (egui chrome) hosting the embedded Servo child. Page renders on launch and navigation without requiring mouse input. Chrome idle CPU is now ~0% (was ~343% from a WARP spin + redraw loop). |
| Direct first present | Fast | Google and example.com regularly first-present under 200ms in debug. |
| Direct first interaction | Fast | Google follow-up interaction frame about 6ms after first present, before load complete. |
| Direct load completion | Measured tail | Google can take about 5.1s to report `LoadStatus::Complete`. |
| Direct controls | Baseline | Input, verified click/type, verified form submit, live search, hosted live form, location, tabs, reload, back/forward, resize, Incognito storage all have bounded smokes with URL/title evidence where relevant. |
| Hosted direct chrome | egui chrome, adaptive backend | Interactive launches use `run_hosted_direct_app_egui` (egui 0.27 / egui-winit 0.27, winit 0.29): real address bar, back/forward/reload, new/previous/next/close tab, load progress, and title/cert status wired to stdin child commands. Render backend is adaptive (`init_chrome_backend`): **wgpu** when a hardware GPU adapter is present, **softbuffer software rasterizer** (`rasterize_chrome`) when only WARP/none — so the chrome idles at ~0% CPU on GPU-less hosts instead of spinning the WARP rasterizer pool. An egui `RedrawRequested→repaint` feedback loop that redrew the static chrome ~37x/s was also fixed. The softbuffer `run_hosted_direct_app` is retained for `--hosted-direct-smoke`. |
| Web-platform features | Ungating in progress | Servo ships many features gated off by default. Enabled (verified): IntersectionObserver, CSS Grid, FontFace, VisualViewport, adoptedStyleSheets, CompositionEvent. Prioritized roadmap in [web-platform-gating.md](web-platform-gating.md). `layout_writing_mode_enabled` verified unsafe (panics). |
| Direct animation rate | Capped at ~60fps | Custom `SixtyHzRefreshDriver` paces Servo animation frame starts at 60fps (Servo default 120) so software-rendered animated pages don't double the script/layout/paint work; rustcor child CPU dropped from ~867% to ~130-270%. |
| Local appliance TLS | First user-facing slice | Direct Servo can explicitly bypass self-signed/bootstrap certificate failures for localhost, `.local`, private, and link-local targets with `--allow-insecure-local-tls`; public targets are refused. Strict local-appliance certificate failures report the SHA-256 fingerprint, decorate Servo's certificate page with Sextant `Go Back` / `Trust Once` / `Trust This Appliance` actions, and now surface fingerprint-bound `BACK` / `ONCE` / `TRUST` actions in hosted direct chrome. Persisted origin+fingerprint trust writes to `appliance-cert-trust.json` outside Incognito, with list/forget hooks for MCP and a Guard-tab settings/revocation surface. Hosted direct smoke can now drive the native certificate actions and report requested/completed/relaunched evidence. |
| MCP browser layer | Working | Tools/resources advertise modes, render paths, hosted direct smoke, local appliance cert list/forget, smokes, perf probes, guard/perception, operator flows, and the repeatable `browser_direct_browsing_baseline` suite. |
| AI modes | Working boundary | Agent/Assisted/Observe use bridge shell; Direct/Incognito disable AI observation/persistence. |
| Reader fallback | Working | `sextant-browser --no-default-features` remains the fast fallback lane. |
| Dependency hygiene | Improved / upstream follow-up | Removed the legacy npm surface and old Rust `reqwest 0.11` / `readability` / `xml5ever 0.16` path. `cargo audit` is down to Servo-owned `ml-dsa` and `rsa` advisories plus warnings. |

## Key Direct Metrics

Latest QA pass: Rust build/test green across the default workspace, no-default fallback lane, MCP/direct smoke baselines, hosted direct smoke, assisted bridge smoke, and direct first-interaction/load smokes. The legacy TypeScript simulator has been removed.

| Metric | Latest useful checkpoint |
|---|---:|
| Direct browsing baseline | Required failures `0`; simple, complex, appliance, live-search, and live-form cases passing |
| Hosted direct smoke | Parent shell ready, embedded child first-present `156ms`, active URL `https://example.com/` |
| Direct viewport baseline | DuckDuckGo max paint stayed in the `833-891ms` range across `640x420`, `960x620`, and `1180x760`; still points toward scene/content work more than raw viewport fill |
| Direct complexity baseline | Separates loading paint from audit overhead: DuckDuckGo full `818ms` loading paint, MDN `903ms` loading paint, DuckDuckGo Lite max paint `3ms`, example.com max paint is audit-only |
| Native-only repo cleanup | Done | Legacy TypeScript/Vite simulator source and npm package files removed. |
| Assisted bridge smoke | `example.com` assisted user-distill passed: first shell draw `4ms`, first Servo frame `758ms`, nav `277ms`, distill `409ms`, Wake `60ms` |
| Google first direct present | `149ms` |
| Google first interaction frame | `6ms` before load complete |
| Verified direct input | `413ms`, clicked/focused/typed through live localhost fixture |
| Verified form submit | `304-520ms`, typed, submitted, and verified `/search?q=sextant` URL/title |
| Live direct search | `841ms`, `26` frames, DuckDuckGo Lite title confirmed `Sextant at DuckDuckGo` in the baseline |
| Google live search | Passing current smoke; first present `156ms`, DOM-located search box click found `textarea[name="q"]`, submitted `/search?q=Sextant`, live-search frame `6.1s`; MCP still classifies no-query and submitted-unverified failures when they occur |
| Bing live search | Passing after local Servo `NodeList` defensive patch: first present `190ms`, DOM-located search box found `textarea[name="q"]` at `560,172`, normal page submit reached `https://www.bing.com/search?q=Sextant...`, live-search frame `6.3s`; patched rerun had no `nodelist.rs` panic |
| Live hosted form smoke | Passing in the direct baseline against `https://httpbin.org/forms/post`: first present `144ms`, DOM-located `input[name="custname"]`, clicked `button:not([type])`, reached `https://httpbin.org/post`, `liveFormFrameMs=492` |
| Google load complete | `5.1s` |
| example.com first direct present | `162ms` in the baseline |
| example.com load complete | `477ms`, final title `Example Domain` |
| DuckDuckGo complex first direct present | `165ms` in the latest baseline |
| DuckDuckGo complex load complete | `2.2s`, `23` frames, max direct frame `1.3s`, slow phase `loading`; page complexity about `816` elements, `2,413` CSS rules, `46` scripts, and `76` resources |
| DuckDuckGo viewport scale check | Max paint `891ms` at `640x420`, `833ms` at `960x620`, `837ms` at `1180x760`; page complexity is about `780` elements, `2,904` CSS rules, `48` scripts, and `74` resources |
| Complexity comparison at `960x620` | example.com `346ms` audit-only paint, DuckDuckGo Lite `3ms`, DuckDuckGo full `818ms` loading paint, MDN `903ms` loading paint |
| DuckDuckGo resource audit | `13` images, `0` broken images, `42` inline SVGs, `34` zero-size SVGs, `10` stylesheets |
| Direct resize follow-up | `27ms` |
| Direct reload follow-up | `411ms`, final `https://example.com/` / `Example Domain` |
| Direct history back/forward | `31ms / 31ms` |
| Direct location final evidence | `625ms`, final `https://example.org/` / `Example Domain` |
| Direct history final evidence | `660ms`, final `https://example.org/` / `Example Domain` |
| Direct tab follow-up | `134ms`, selected tab URL/title confirmed |
| Pylon local appliance login page | `173ms` first present, `300ms` load complete with local TLS override, final title `Login - Pylon` |
| Pylon strict certificate failure | `188ms` first present, `559ms` load complete, fingerprint `5526c07eed59d052c7c48b7e8bdf1d3685b43e2b127de45e94abbdd2bcef8cb5` |
| Pylon hosted certificate `BACK` smoke | Parent shell ready, action requested/completed, active URL `about:blank`, fingerprint `5526c07eed59d052c7c48b7e8bdf1d3685b43e2b127de45e94abbdd2bcef8cb5` |
| Pylon hosted certificate `ONCE` smoke | Parent shell ready, action requested/completed/relaunched, first present `112ms`, active URL `https://192.0.2.130:8080/app/login` |
| Pylon hosted certificate `TRUST` smoke | Isolated profile, action requested/completed/relaunched, first present `136ms`, active URL `https://192.0.2.130:8080/app/login`, temp profile removed |

## Current Interpretation

The raw direct lane proves the main architectural bet: Servo can render to the real native window fast enough to feel browser-like when the frame-capture bridge is bypassed.

The important split is now explicit:

- `directPresentMs` and `firstInteractionFrameMs` represent user-perceived speed.
- `loadCompleteMs` represents complex-page tail work.

That split is why Google can feel snappy even while Servo continues background page work for several seconds.

## Known Gaps

| Gap | Priority | Notes |
|---|---|---|
| Direct shell composition | Medium | egui hosted chrome renders the parent shell (address bar, nav, tabs, status) for Direct/Incognito; pages render on launch + navigation. Render backend is now adaptive GPU(wgpu)/CPU(softbuffer) and idles at ~0% CPU; the egui redraw feedback loop is fixed. Remaining: HiDPI chrome/content boundary alignment, icon/tab-strip polish, and porting the appliance-cert trust→relaunch flow from the softbuffer shell. |
| Appliance certificate UX | High | Hosted direct chrome now parses certificate fingerprints from the child, shows a native warning/control strip, can leave the warning without trusting, relaunches the trusted child embedded after `Trust Once` / `Trust This Appliance`, exposes persisted trust entries in the Guard tab, and has hosted-shell smoke coverage for native `BACK` / `ONCE` / `TRUST` actions; next is manual appliance QA and visual polish. |
| Live-site direct interaction depth | High | Controlled click/type and form-submit proof works; Google/Bing live search pass through normal input, and a hosted `httpbin.org` form now passes through DOM-targeted raw Servo click/type/submit. Real hyperlink navigation is now proven and first-class: `--live-link-smoke` DOM-locates a cross-document `a[href]`, clicks it, and verifies the cross-host result (`example.com` -> `www.iana.org`, `916ms`-`1.3s` nav frame), wired through the standalone bin, `--window-smoke`, MCP, and the direct browsing baseline (optional `live_link` case, `liveLink`/`liveLinkUrl`/`liveLinkFrameMs`). Next is multi-field-form and scroll-into-view controls, then promoting `live_link` to a required baseline case. |
| Complex-page Servo tail | High | Co-recorded worst-frame attribution (2026-06-11) shows the heavy loading frame is ~100% WebRender `webview.paint()`: `spin`≈`0ms`, `present`≈`1ms`, paint `1.1s` (DDG full) / `794ms` (MDN). Paint stays flat across a `3.3x` pixel-area range, so it is content/scene-bound, not GPU fill-rate. Optimization target is WebRender frame building inside `paint()`; next step is a release-profile sizing pass. Google's multi-second `loadCompleteMs` is a separate network/subresource axis. See [performance-log.md](performance-log.md). |
| Bridge shell polish | High | Full Agent/Assisted shell still depends on bridge and should stay stable. |
| Manual QA pass | High | Run the actual debug browser and validate normal browsing feel. |
| Servo dependency advisories | Medium | `ml-dsa 0.0.4` and `rsa 0.9.10` are pulled through Servo's `servo-script`; `ml-dsa` has a fixed prerelease but Servo currently constrains `^0.0.4`, and `rsa` has no fixed upgrade. |
| Xilem/Vello | Parked | Diagnostic ladder remains archived/reference until direct path is further along. |

## Practical Recommendation

Next work should keep moving down [next-work.md](next-work.md):

1. Keep the direct browsing baseline green and use it as the scoreboard for user-side browsing.
2. Broaden live-page search/input proof beyond DuckDuckGo Lite and capture failures by site.
3. Start designing the direct chrome/tab/address overlay path.
4. Keep bridge shell smokes green while direct grows into the main user path.
5. Use `performance-log.md` for timing deltas and avoid relying on feel alone.
