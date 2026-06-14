# Web-Platform Feature Gating

Servo's library `Preferences` defaults leave many web-platform features gated
`false` (the upstream defaults are conservative, and the checked-out Servo ships
no `prefs.json` override, so servoshell runs with the same gates). Real sites
silently break on the missing ones: an absent global throws a `ReferenceError`
(this is how the IntersectionObserver bug presented), and an unimplemented CSS
feature falls back wrong (`display: grid` collapsed to block layout).

We selectively enable verified-safe features in the direct lane
(`rust/sextant-hull/src/direct_servo.rs`, in the `Preferences` block). The rule
is **never enable a gate blind** — some gates protect incomplete code that
panics. `layout_writing_mode_enabled` was the first proof: it asserts
"Mixed horizontal and vertical writing modes are not supported yet" and panics
the style thread on essentially any page with a vertical element.

## Verification process

1. Add the pref in the `Preferences` block in `direct_servo.rs`.
2. Probe it with a self-reporting test page (reports pass/fail via `document.title`,
   read back from the hosted-direct child log). Never enable without a probe.
3. Layout features additionally get: a stress page (edge cases) **and** a
   real-site no-panic smoke (load a real site, grep the child log for `panic`).
4. One small batch per commit; update the status tables below.

Status legend: ✅ enabled · 🧪 test next · ⛔ unsafe (panics) · 🔒 deferred
(needs permission UX / privacy policy) · 🏗️ heavy (large/incomplete surface) ·
🚫 never (test-only / internal).

## ✅ Enabled

| Pref | Feature | Verified by |
|---|---|---|
| `dom_intersection_observer_enabled` | IntersectionObserver | probe (`E-IOCB int=true`, `observers=1`) |
| `layout_grid_enabled` | CSS Grid | probe + stress (`fr`/`minmax`/`repeat`/named-areas/`span`/`auto-fill`/nesting), rustcor no-panic |
| `dom_fontface_enabled` | `FontFace` / `document.fonts` | probe |
| `dom_visual_viewport_enabled` | `window.visualViewport` | probe |
| `dom_adoptedstylesheet_enabled` | `document.adoptedStyleSheets` (web components) | probe |
| `dom_composition_event_enabled` | `CompositionEvent` (IME text input) | probe |
| `dom_indexeddb_enabled` | IndexedDB | probe `roundtrip=OK` over http://localhost. Storage is profile-scoped via the direct lane's `config_dir` (persistent `browser/direct-servo` for Direct; ephemeral temp dir swept on close for Incognito). `file://` correctly rejects with "operation is insecure". |
| `dom_webvtt_enabled` | WebVTT | probe: `new VTTCue(...)` / `new VTTRegion()` construct and are functional (`VTTCueNew=OK VTTRegionNew=OK`). |

## 🧪 Priority queue (work/reward)

Reward = how often real sites need it. Work = build/test cost + completeness
(panic) risk. Builds here are sextant-hull-only (~40s, no Servo recompile).

> **2026-06-14 finding:** the gated *layout* features beyond Grid are
> **parse-only** in this Servo — the prefs appear only in the codegen CSS-parse
> map (`script_bindings/codegen/run.py`), with no evaluation in
> `components/style/` or `components/layout/`. Enabling them is a no-op (probe:
> `container=no multicol=no`) and could make feature-detecting sites render
> *worse* (they detect "support" then use a layout that never applies). So the
> real remaining wins are **DOM APIs that have real implementations**, not CSS
> layout. Survey method: check for a populated `components/script/dom/<feature>/`
> dir or a non-stub `dom/<feature>.rs`.

### P1 — implemented DOM APIs (queue worked through)

IndexedDB and WebVTT are enabled (above). `dom_exec_command_enabled` is **left
off — partial**: Servo's `command_if_command_is_supported`
(`dom/execcommand/execcommands.rs`) returns `Some` for only 5 commands
(`delete`, `defaultParagraphSeparator`, `fontSize`, `styleWithCss`, `underline`
— `underline` verified working), while the common editor commands (`bold`,
`italic`, `insertText`, `createLink`, lists, justify) return `None` and no-op.
`queryCommandSupported` reports this accurately, but most editors call the common
commands directly, so exposing it would mislead them. Re-evaluate if Servo adds
the common commands.

Remaining candidates are now the niche/perf (P3), privacy-gated, and heavy tiers
below — lower reward or larger work per feature.

> Probe DOM features over **http://localhost** (`python -m http.server`), not
> `file://` — storage/secure-context APIs reject the `file://` opaque origin
> (IndexedDB threw "operation is insecure" there but `roundtrip=OK` over http).

### ✓ Resolved prerequisite (IndexedDB storage scoping)

Servo's IndexedDB base dir is `config_dir/IndexedDB`, falling back to a
cwd-relative `IndexedDB/` when `config_dir` is `None`
(`components/storage/indexeddb/mod.rs`). The direct lane previously passed
`config_dir` only for Incognito (an ephemeral temp dir); Direct got `None`, so
storage leaked cwd-relative. Fixed in `browser.rs`: Direct now uses a persistent
`browser/direct-servo` profile dir, Incognito keeps the ephemeral temp dir, and
the Incognito parent sweeps leftover `sextant-browser-direct-incognito-*` temp
dirs on close (the embedded child is hard-killed, so it cannot self-clean). This
also profile-scopes cookies/HSTS/auth-cache, not just IndexedDB.

### P3 — niche / perf / likely stubbed

`dom_offscreen_canvas_enabled` (no `dom/offscreencanvas*` file — likely stubbed),
`dom_cookiestore_enabled`, `dom_sanitizer_enabled`, `dom_canvas_capture_enabled`,
`dom_navigator_protocol_handlers_enabled`, `dom_allow_preloading_module_descendants`,
`dom_servoparser_async_html_tokenizer_enabled` (parser change — extra care).

### ⚠️ Parse-only / not laid out (no benefit — leave off)

`layout_container_queries_enabled`, `layout_columns_enabled` (verified: parse, no
layout). `layout_css_attr_enabled` and `layout_variable_fonts_enabled` are also
layout-gated and unverified — low priority since Grid is the only implemented
layout gate found.

## 🔒 Deferred — needs permission UX / privacy policy

These expose user data or hardware and must align with the Direct/Incognito
sovereignty stance (explicit consent, no silent access) before enabling:
`dom_geolocation_enabled`, `dom_notification_enabled`, `dom_permissions_enabled`,
`dom_credential_management_enabled`, `dom_async_clipboard_enabled`,
`dom_bluetooth_enabled`, `dom_wakelock_enabled`, `dom_webxr_*`.

## 🏗️ Heavy — large/incomplete surface (dedicated effort each)

`dom_serviceworker_enabled`, `dom_webrtc_enabled` (+`_transceiver`),
`dom_webgpu_enabled`, `dom_webgl2_enabled`, `dom_worklet_enabled`
(+`_blockingsleep`), `media_glvideo_enabled`. Several need a real GPU or are
partial; revisit per-feature, likely better on the GPU workstation.

## ⛔ Unsafe — verified to panic

| Pref | Failure |
|---|---|
| `layout_writing_mode_enabled` | `assert_eq!` panic in `layout/flow/mod.rs`: "Mixed horizontal and vertical writing modes are not supported yet" — fires on any vertical element in a horizontal page. |

## 🚫 Never — test-only / internal Servo prefs

Not web features; for Servo's own test suite or internals — leave off:
`css_animations_testing_enabled`, `dom_*_testing_*`, `dom_testbinding*`,
`dom_testperf_enabled`, `dom_testutils_enabled`, `dom_servo_helpers_enabled`,
`dom_fullscreen_test`, `dom_microdata_testing_enabled`, `dom_webxr_test`,
`dom_permissions_testing_*`, `layout_animations_test_enabled`,
`layout_unimplemented`, `media_testing_enabled`, `gfx_precache_shaders`
(GPU internal). `dom_allow_scripts_to_close_windows` is a behavior policy, not a
feature.
