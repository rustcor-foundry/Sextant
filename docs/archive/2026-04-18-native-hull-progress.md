# 2026-04-18 — Native Hull Progress

This session pushed the native Rust hull from "workspace compiles" into a much more honest end-to-end browser workflow.

## What Moved Forward

- The native hull now launches and stays up long enough to sanity-check startup changes.
- The hull UI was split into focused modules:
  - `app_core.rs`
  - `state.rs`
  - `views.rs`
  - `poller.rs`
  - `util.rs`
  - `deferred.rs`
- The async bridge is now standardized around toolkit-agnostic app commands and events instead of mixed `block_on(...)` paths.
- The AI/settings lane now has:
  - explicit provider selection
  - apply/test/save/load actions
  - active vs selected provider state
  - readiness/error feedback in the UI
- The hull now surfaces:
  - last command summary
  - pending async job count
  - real Captain's Log entries
  - separate runtime/system log output
  - clearer consent status and preflight failure messaging

## Real Workflow Fixes

- `PilotAction::OpenTab` now switches to the newly created tab before navigating.
- Wake hybrid search was fixed in two places:
  - qualified column names in the FTS join
  - normalized FTS query input so punctuation and hyphenated intents do not break searches
- Engine multi-tab perception now writes distilled pages back into tab state instead of only returning them.
- `url` serde support was enabled for `sextant-engine`, which unblocked engine lib tests.

## Regression Coverage Added

The Pilot now has focused tests for real workflow slices:

- opening a target in a new tab
- navigate -> distill -> write to Wake
- request consent -> await -> deny
- request consent -> authorize -> resume plan -> write to Wake

## Small Cleanup

- removed a handful of obvious unused imports in:
  - `sextant-airgap`
  - `sextant-wake`
  - `sextant-sync`
  - `sextant-vault`
- replaced deprecated `base64::encode/decode` calls in `sextant-sync`

## Current Practical State

The native product path is no longer just scaffolded. We now have a real command pipeline, a more truthful hull, persisted Wake/Log storage, and regression coverage around the main Pilot loop.

The biggest remaining gaps are still around broader UI exercise and the larger platform lanes:

- richer in-window workflow validation
- remaining workspace warning cleanup
- CI
- future rendering/backend work like Servo
