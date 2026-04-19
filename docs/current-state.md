# Current State

Date of this snapshot: `2026-04-19`

## How To Read This

Use this together with:

- [next-work.md](next-work.md) for the active execution order
- [architecture.md](architecture.md) for the stable system model
- [development-guardrails.md](development-guardrails.md) for project rules
- [completed.md](completed.md) for dated history

## Executive Summary

Sextant now has a real native Rust product path with a launching Xilem hull, a toolkit-agnostic async command runtime, persistent Wake and Captain's Log storage, and working intent/consent regression coverage in the Pilot.

The product is no longer just a compile-clean scaffold. The native hull can:

- launch
- show tabs, viewport, wake, settings, and audit information
- route commands through async app commands/events
- surface provider readiness and configuration state
- show real Captain's Log entries and runtime status

The biggest remaining gaps are no longer foundational compilation issues. They are mostly around deeper interactive validation, broader warning cleanup, CI, and later rendering/backend work.

## What Is Working

| Capability | State | Notes |
|------------|-------|-------|
| Rust workspace compile | ✅ | `cargo check` passes |
| Native hull launch | ✅ | hull stays up in real launch checks |
| Hull modular structure | ✅ | `app_core`, `state`, `views`, `poller`, `util`, `deferred` split in place |
| Async hull command runtime | ✅ | commands/events/snapshots route through toolkit-agnostic app core |
| Persistent Wake storage | ✅ | SQLite file-backed |
| Persistent Captain's Log | ✅ | SQLite file-backed |
| Provider control panel | ✅ | selected/active/tested/readiness state surfaced |
| Provider apply/test/save/load | ✅ | wired through async command path |
| Intent -> Pilot -> Engine -> Wake | ✅ baseline | covered by Pilot regression tests |
| Consent gating | ✅ baseline | await/deny/authorize flow covered by tests |
| Real page distillation | ✅ baseline | fetch + parse + semantic extraction path exists |
| Tab create/switch/close | ✅ | wired through hull async path |

## Regression Coverage In Place

Pilot tests currently cover:

- opening a target in a new tab
- navigate -> distill -> write to Wake
- request consent -> deny
- request consent -> authorize -> resume plan

Engine tests currently cover:

- multi-tab perception updates tab state

## Current UX State

The hull now tells the truth more clearly than earlier passes:

- command preflight failures are surfaced instead of silently ignored
- consent buttons explain when no consent request is pending
- dashboard shows last command summary and async job count
- Captain's Log shows persisted audit entries
- System Log is separate from the audit trail
- AI settings show selected vs active provider and readiness/test state

## Known Gaps

| Gap | Priority | Notes |
|-----|----------|-------|
| Full in-window interactive workflow exercise | High | backend path is stronger than live click-through validation |
| Remaining workspace warnings | Medium | mostly `sextant-bridge` and `sextant-mesh`, plus a few low-noise leftovers |
| CI workflow | Medium | repo is now on Gitea and ready for a build/test workflow |
| Servo backend work | Medium | still optional and not the current blocker |
| Local/mesh/sync/PQ feature surfaces | Low | intentionally deferred behind the core browser loop |

## Canonical Repo State

This project now has a valid git repo and canonical remote on Gitea:

- `rustcor/Sextant`

That remote should be treated as the main source of truth going forward.

## Practical Recommendation

The next best work should keep following [next-work.md](next-work.md) and bias toward:

1. real user-visible workflow validation
2. high-signal warning cleanup
3. CI
4. later backend/rendering expansion
