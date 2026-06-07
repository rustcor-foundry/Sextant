# Development Guardrails

These are the working guardrails for Sextant development. They are here to keep the project moving without repeatedly rediscovering the same layout, workflow, and scope traps.

## Read This Before Editing the Hull

The Rust native hull is the product target. The old TypeScript simulator has been removed, so architecture and implementation decisions should stay anchored in the Rust product path.

When choosing what to work on next:

1. Follow [next-work.md](next-work.md) as the active execution guide.
2. Use [architecture.md](architecture.md) to preserve the intended system shape.
3. Treat the roadmap as background strategy unless the team explicitly decides to re-prioritize.

## Xilem Layout Guardrails

The hull must stay modular. Large inline view trees create tuple-size limit failures, make layout bugs harder to isolate, and slow down iteration.

Use these rules when editing `sextant-hull`:

1. Keep each `flex((...))` or tuple-based view group small.
2. Aim for about 7-8 direct children max before extracting a sub-view.
3. Do not keep growing `app_logic_native` inline.
4. Break major sections into sub-view functions with clear ownership.
5. Use data-driven containers for repeated content such as tabs, logs, and Wake results instead of hand-expanding tuples.
6. If a view starts feeling structural rather than local, extract it before adding more controls.

Recommended hull structure:

- `header_view(...)`
- `intent_bar_view(...)`
- `dashboard_view(...)`
- `viewport_view(...)`
- `wake_panel_view(...)`
- additional focused sub-views as new sections become real

## Workflow Guardrails

Sextant should advance through real product plumbing, not fake polish.

1. Prefer real integrations over simulated behavior when the choice matters.
2. Avoid spending too long polishing one panel while the core browser loop is still incomplete.
3. Finish a lane to a reasonable stopping point, then return to the next item in [next-work.md](next-work.md).
4. Build user-facing status and error feedback when wiring real systems so failures are visible and debuggable.
5. Persist important state to disk instead of relying on in-memory placeholders once a feature starts becoming real.

## Servo Patch Guardrails

Servo is currently pulled from the Cargo-locked upstream git revision. Sextant carries one documented local patch for the Bing live-search `NodeList` panic:

- Patch artifact: [servo-nodelist-bing-panic.patch](patches/servo-nodelist-bing-panic.patch)
- Helper: `rust/scripts/apply-servo-patches.ps1`

Run the helper after a fresh Cargo Servo checkout or after moving the locked Servo revision:

```powershell
powershell -ExecutionPolicy Bypass -File .\rust\scripts\apply-servo-patches.ps1
```

Keep this as a temporary bridge. Prefer an upstream Servo PR or an explicit pinned/forked Servo dependency over relying on an edited Cargo checkout long term.

## Runtime Guardrails

Keep Sextant's execution model close to the product logic, not tightly coupled to the current UI toolkit.

1. Define app commands, events, snapshots, and background execution in a toolkit-agnostic core layer.
2. Treat the hull as an adapter that translates user actions into commands and renders current state.
3. Keep toolkit-specific polling, redraw, or widget tricks in a thin bridge layer.
4. Do not bury Pilot, Engine, Wake, or Vault orchestration logic inside widget callbacks.
5. Prefer interfaces that would still make sense if the UI layer changed later.

## Current UX Guardrails

Settings and control surfaces should help the user understand the system, not hide it.

1. Show what is selected, what is active, and what is merely configured.
2. Surface readiness, failure, and pending states in the UI instead of relying on logs alone.
3. Keep unfinished panels honest. If something is not implemented yet, say so plainly.
4. Favor clear system status over decorative UI expansion.

## AI Provider Guardrails

Cloud inference is optional and must stay under user control.

1. Start with Gemini as the first-class cloud provider.
2. The user should be able to see which provider is active.
3. Provider configuration should eventually be managed through the vault.
4. The UI should make room for cloud and local backends in the same control plane.
5. Missing credentials should fail clearly rather than silently falling back to demo behavior.

## Definition of "On Guardrails"

Before merging or moving on from a hull change, sanity-check these questions:

1. Did we keep the layout modular?
2. Did we avoid adding more tuple-heavy inline structure?
3. Did this move the real product workflow forward?
4. Does the UI tell the truth about system state?
5. Are we still aligned with [next-work.md](next-work.md)?
