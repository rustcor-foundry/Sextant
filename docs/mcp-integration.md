# MCP Integration

Sextant now has a local stdio MCP server in the Rust workspace:

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-mcp
```

This server advertises the native browser capabilities to MCP clients such as Codex and Claude without opening a network listener. It speaks newline-delimited JSON-RPC over stdin/stdout and keeps logs on stderr only.

## Current Tool Scope

The first layer is intentionally truthful and narrow:

- `browser_capabilities` reports the current native browser, operator bridge, tools, and resources.
- `browser_operator_smoke` runs the deterministic `sextant-browser --operator-smoke` check.
- `browser_launch_preflight` runs operator smoke, Intent Bar, showcase, and visible showcase smoke checks in sequence.
- `browser_hardening_preflight` runs launch preflight coverage plus real browsing, visible real browsing, and visible shell-interaction checks.
- `browser_real_browsing_smoke` runs the broader user-browsing suite across example.com, Google search, MDN, reload, IANA, back/forward, Wake, Captain's Log, and frame capture.
- `browser_guard_probe` evaluates a target page against the local trust-boundary guard and returns persona, policy source, rule counts, air-gap, firewall, and privacy redaction status. Guard-blocked targets are returned as policy findings instead of attempted navigation. Calls may include `policy_path` to test a JSON firewall policy overlay.
- `browser_guard_policy_read` reads the canonical browser `guard-policy.json` overlay from the local browser data directory.
- `browser_guard_policy_write` validates and writes the canonical browser `guard-policy.json` overlay.
- `browser_perception_probe` opens and distills a target page, then returns page perception summary, semantic counts, key nodes, source metadata, and timings.
- `browser_perf_probe` runs a timed single-page browser pass with navigation, distillation, Wake, viewport resize, first-frame, and warm-frame timings.
- `browser_perf_baseline` runs the built-in normal-browsing performance baseline across simple and heavier representative pages.
- `browser_direct_browsing_baseline` runs the direct Servo user-browsing baseline across simple load, complex load, optional local appliance load, and live search.
- `browser_hosted_direct_smoke` launches the Direct/Incognito parent chrome with an embedded Servo child and exits after first-present evidence.
- `browser_local_appliance_cert_list` lists persisted profile-scoped local appliance certificate trust exceptions.
- `browser_local_appliance_cert_forget` removes one profile-scoped local appliance certificate trust exception, or clears all persisted local appliance trust entries.
- `browser_direct_viewport_baseline` runs the direct Servo complex-page load at several viewport sizes to test whether heavy paint cost scales with pixel area.
- `browser_direct_complexity_baseline` runs direct Servo load/resource-audit checks across simple, lite, complex, and article-style pages to compare paint cost with page complexity.
- `browser_showcase_run` runs the launch-demo proof across Intent Bar, Servo navigation, Wake, Captain's Log, form interaction, and frame capture.
- `browser_window_smoke` launches the visible `sextant-browser` shell, optionally navigates to a target or runs the showcase, real-browsing, or shell-interaction seed first, draws once, and exits.
  In Direct render mode it can also pass `allow_insecure_local_tls` for localhost, `.local`, private-IP, or link-local appliance testing, and can run `resource_audit` after direct load-smoke to capture missing image/SVG/stylesheet/canvas evidence. The TLS bypass is a dev/test bridge for bootstrap self-signed certificates, not a public-web TLS bypass.
  Live-search smoke summaries include `liveSearchAttempts`, `liveSearchDomTarget`, `liveSearchDomFormTargetUrl`, `timeoutTitle`, `timeoutUrl`, `liveSearchSubmitted`, `liveSearchBlocked`, and `liveSearchBlockReason` so live-page input failures can distinguish challenge pages, submitted-but-unverified searches, no-submit home-page timeouts, and explicit form-derived navigation fallbacks.
- `browser_intent_run` drives the native Intent Bar loop, for example `intent: open https://example.com and distill`, with optional expected text validation.
- `browser_authorized_intent_run` drives a sensitive Intent Bar loop, authorizes the pending Captain's Key request, and resumes the bounded browser continuation.
- `browser_operator_probe` opens a URL or search phrase, distills it, and reports the native operator output.
- `browser_operator_run` executes ordered `fill`, `click`, `submit`, and `expect` selector steps through the native browser operator bridge.
- `captains_log_recent` reads recent Captain's Log entries for the browser persona.

During development, operator tools prefer the local Cargo workspace so MCP calls exercise the current source instead of a stale sibling executable. Packaged installs can set `SEXTANT_MCP_USE_INSTALLED_BROWSER=1` to prefer the canonical `sextant-browser` binary beside `sextant-mcp`.

The development fallback command is:

```bash
cargo run -p sextant-hull --bin sextant-browser -- ...
```

## Resources

The server also exposes static capability resources:

- `sextant://browser/capabilities`
- `sextant://browser/operator-workflow`
- `sextant://browser/mcp-tools`

Use these resources as the "what can this browser do right now?" context surface for agents.

## Development Checks

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-mcp -- --self-test
cargo run -p sextant-mcp -- --list-tools
cargo check -p sextant-mcp
cargo test -p sextant-mcp
```

When testing against a real MCP client, initialize first, then call `tools/list`. For quick manual protocol checks, send one JSON-RPC message per line.

Use `browser_window_smoke` when the question is "does the actual user-facing window launch and draw?" Use `browser_operator_*` tools when the question is "can the native browser automation path navigate, interact, distill, and record state?"

Use `browser_intent_run` when the question is "can the browser perform the user-facing Intent Bar workflow?" A typical call is:

```json
{"intent":"intent: open https://example.com and distill","expect":"Example Domain","timeout_seconds":45}
```

Use `browser_authorized_intent_run` when the question is "does Captain's Key consent resume the browser-owned plan?" A typical call is:

```json
{"intent":"intent: buy https://example.com and checkout","expect":"Example Domain","timeout_seconds":120}
```

Use `browser_showcase_run` for a bounded pre-demo proof that the launch path still works end to end.

Use `browser_real_browsing_smoke` when the question is "does normal browsing still hold past the curated showcase?"

Use `browser_guard_probe` when the question is "what local guardrails apply to this page?" A typical call is:

```json
{"target":"https://example.com","timeout_seconds":120}
```

Use `browser_guard_policy_read` when an agent needs to inspect the canonical browser policy:

```json
{}
```

Use `browser_guard_policy_write` to validate and write the canonical browser policy:

```json
{
  "policy": {
    "persona_rules": {
      "browser-persona": [
        {
          "domain_pattern": "example.com",
          "action": "Block",
          "reason": "operator policy overlay"
        }
      ]
    }
  }
}
```

With a policy overlay:

```json
{"target":"https://example.com","policy_path":"C:/path/to/guard-policy.json","timeout_seconds":120}
```

Guard probe responses include `structuredContent.guard.persona`, `structuredContent.guard.policy`, `structuredContent.guard.rules`, `structuredContent.guard.airgap`, `structuredContent.guard.firewall`, `structuredContent.guard.action`, `structuredContent.guard.blocked`, `structuredContent.guard.block`, `structuredContent.guard.privacy`, and the raw `structuredContent.guard.lines`. A blocked target still returns a successful tool result when the browser proved the policy decision, with `blocked: true` and the block line preserved.

Policy overlay format:

```json
{
  "persona_rules": {
    "browser-persona": [
      {
        "domain_pattern": "example.com",
        "action": "Block",
        "reason": "operator policy overlay"
      }
    ]
  },
  "global_blacklist": ["ads.example.net"]
}
```

Use `browser_perception_probe` when the question is "what does the native browser perceive on this page?" A typical call is:

```json
{"target":"https://example.com","timeout_seconds":120}
```

Perception probe responses include `structuredContent.perception.summary`, `structuredContent.perception.counts`, `structuredContent.perception.nodes`, and `structuredContent.perception.source`, plus the normal `operatorReport` and `perfTimings` fields.

Use `browser_perf_probe` when the question is "where is this page spending time?" A typical call is:

```json
{"target":"https://developer.mozilla.org/en-US/docs/Web/HTML","timeout_seconds":180}
```

Use `browser_perf_baseline` when the question is "what is the current normal-browsing timing baseline across representative pages?"

Use `browser_direct_browsing_baseline` when the question is "is the direct Servo browsing path still fast and usable across simple, complex, appliance, and search cases?" A typical call is:

```json
{"timeout_seconds":75}
```

Optional arguments are `simple_target`, `complex_target`, `appliance_target`, `search_target`, and `appliance_required`. By default the local appliance case is optional so an offline lab box does not fail the whole baseline.

Use `browser_direct_viewport_baseline` when the question is "does the remaining heavy direct paint scale with viewport size?" A typical call is:

```json
{"timeout_seconds":75}
```

It runs the complex target at `640x420`, `960x620`, and `1180x760`, returning viewport area, max paint, normalized paint-per-megapixel, resource/page-complexity audit summary, and slow-frame phase samples.

Use `browser_direct_complexity_baseline` when the question is "does direct paint cost track page complexity?" A typical call is:

```json
{"timeout_seconds":75}
```

It compares example.com, DuckDuckGo Lite, full DuckDuckGo, and an MDN article at `960x620`. Responses include phase-specific paint metrics such as `maxLoadingPaintMs` and `maxPostLoadAuditPaintMs`, so agents can separate page-load rendering cost from audit/tooling overhead.

Use `browser_hosted_direct_smoke` when the question is "does the product Direct shell host Servo and report first-present evidence?" A typical call is:

```json
{"target":"https://example.com","timeout_seconds":30}
```

Responses include `structuredContent.hostedDirect.parentShellReady`, `childPid`, `firstPresentMs`, `activeUrl`, and `certificateFingerprintSha256` when a strict local appliance certificate failure is encountered. For local appliance certificate QA, pass `certificate_action` as `back`, `once`, or `trust`; responses then include `certificateAction`, `certificateActionRequested`, `certificateActionCompleted`, and `certificateActionRelaunched` when the action relaunches the child. Set `isolated_profile: true` for trust-button smokes that should persist into a temporary browser profile and clean it up afterward instead of mutating the normal profile trust store.

Use `browser_local_appliance_cert_list` when an agent or settings workflow needs to inspect persisted local appliance trust. A typical call is:

```json
{}
```

Responses include `structuredContent.trustStore.path`, `count`, and `entries`. Each entry includes the trusted local appliance origin, SHA-256 fingerprint, label, and creation timestamp.

Use `browser_local_appliance_cert_forget` when an agent or settings workflow needs to revoke persisted local appliance trust. A typical call is:

```json
{"target":"https://192.0.2.130:8080/app/login"}
```

The `target` value can also be `all` to clear every persisted local appliance certificate exception.

Performance tool responses include `structuredContent.perfTimings`, an array of parsed timing samples with `target`, `navigationMs`, `distillMs`, `wakeMs`, `resizeMs`, and `frameMs` fields. Pending phases are returned as `null`. They also include `structuredContent.perfSummary`, with max timings by phase and the single slowest phase, including the target page when available.

Visible window-smoke responses include `structuredContent.windowSmoke.frameTotalMs`, `frameQueueMs`, and `frameMs` when those slow perf events are present, so agents can distinguish render-bridge wall time, Servo service wait, and actual capture/readback work. They also expose `navUrlWaitMs` and `navLoadWaitMs` when split Servo navigation events are present.

Direct window-smoke responses can also include `directFrameCount`, `directSlowFrameCount`, `directSlowFrames`, `maxDirectFrameMs`, `maxDirectSpinMs`, `maxDirectPaintMs`, `maxDirectPresentMs`, and `resourceAuditSummary` so agents can separate heavy Servo paint frames from missing-asset/resource health. Slow direct frame samples include the observed phase, such as `loading`, `input`, `post-load-audit`, or `idle`.
Direct window-smoke calls can set `viewport_width` and `viewport_height` for Direct-render tests; MCP passes these through to the raw Servo window as initial physical-pixel bounds.

The native browser writes operator/performance report lines to stdout and browser diagnostics to stderr, so MCP clients can use `operatorReport` and `perfTimings` without filtering Servo log noise.

Use `browser_hardening_preflight` when the question is "does the launch path, broader browsing path, and visible shell interaction path all still pass?"

Use `browser_window_smoke` with `{"showcase": true}` when the question is "does the actual visible showcase window draw?"

Use `browser_window_smoke` with `{"real_browsing": true, "timeout_seconds": 60}` when the question is "does the actual visible shell draw after normal browsing?"

Use `browser_window_smoke` with `{"shell_interaction": true, "timeout_seconds": 45}` when the question is "do the visible chrome controls still work through their click path?"

Use `browser_window_smoke` with local appliance TLS only when the target is an appliance you control on localhost, `.local`, a private IP, or a link-local address:

```json
{
  "target": "https://192.0.2.130:8080/app/login",
  "mode": "direct",
  "render_path": "direct",
  "load_smoke": true,
  "allow_insecure_local_tls": true,
  "timeout_seconds": 45
}
```

The product UX target is stricter than this smoke flag: default strict TLS, a certificate warning for local appliances, `Trust Once`, profile-scoped `Trust This Appliance` pinned to the certificate fingerprint, revocation, and no persisted trust in Incognito. The profile trust foundation is `appliance-cert-trust.json`, keyed by local appliance origin and SHA-256 certificate fingerprint; strict certificate failures now report `certificateFingerprintSha256`, and the hosted direct shell surfaces native certificate telemetry/actions. `remember_local_appliance_cert` and `local_appliance_cert_fingerprint` remain the direct smoke development hooks for scripted trust-store work, while `sextant-browser --list-local-appliance-certs` and `sextant-browser --forget-local-appliance-cert <url|all>` are the settings/operator hooks for inspecting and revoking trust.

Use the CLI preflight before live demo work:

```bash
cargo run -p sextant-mcp -- --launch-preflight --timeout-seconds 60
cargo run -p sextant-mcp -- --hardening-preflight --timeout-seconds 240 --visible-timeout-seconds 60
cargo run -p sextant-mcp -- --real-browsing-smoke --timeout-seconds 240
```

Add `--json` when a script needs the full structured check report.

Preflight summaries include total and per-check timings by default; the same `durationMs` values are present in structured MCP/JSON output.

## Next Integration Step

This MCP layer currently delegates browser work to the native operator binary. The next deeper integration should extract the browser session/operator runtime from `sextant-hull/src/browser.rs` into reusable Rust code so the visible browser and MCP server can share a long-lived session path.
