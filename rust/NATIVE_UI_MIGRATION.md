# Sextant Native UI Migration

This document maps the AI Studio web preview to the real native Hull described in the ADD.

## Keep

- Nautical instrument-panel visual language
- Intent-first interaction model instead of a classic URL bar
- Three-zone shell structure: status rail, viewport, memory rail
- High-signal labels, telemetry, authorization prompts, and log surfaces
- Dense operational UI with clear subsystem boundaries

## Discard

- The TypeScript simulator as an application surface
- Fake MCP, vault, wake, and WebLLM implementations
- Simulated browser content masquerading as a real page renderer
- Browser-only environment assumptions and web build plumbing
- Any UI state whose only purpose was to make the preview feel more complete

## Rebuild In The Hull

### Core Surfaces

- Intent Bar
- Distilled Viewport
- Sovereignty Dashboard
- Wake Panel
- Captain's Key prompt
- System log console

### State Sources

- `SextantPilot` for reasoning and authorization status
- `SextantEngine` for tabs, active page, and backend telemetry
- `DigitalWake` for local memory retrieval
- `CitadelVault` for identity and security posture
- `CaptainsLog` for audit status

## Preview To Native Mapping

| Preview UI | Native Hull target | Notes |
| --- | --- | --- |
| Intent input bar | Top command rail | Becomes the primary command surface for Pilot actions |
| Left sidebar status cards | Sovereignty Dashboard | Persona, vault, privacy, air-gap, engine state |
| Center browser simulation | Distilled Viewport | Must render real distilled page data from Engine |
| Right settings / memory tabs | Wake Panel | Split between local memory, audit, and tool state |
| Captain's Key dialog | Inline authorization prompt | Native prompt tied to Pilot consent state |
| Bottom logs | System log console | Shared operational trace for Hull / Pilot / Engine |

## First Native Build Slice

1. Build a stable shell with dashboard, viewport, wake rail, and logs.
2. Render real `active_page` and `engine_status` data in the viewport.
3. Surface `PilotStatus::AwaitingConsent` as an inline authorization panel.
4. Keep command actions explicit and honest when still stubbed.
5. Remove duplicate or simulator-era controls from the Hull.

## Immediate Next Steps

1. Finish the first-pass Xilem shell layout.
2. Connect top-bar actions to real async Pilot and Engine commands.
3. Add a dedicated distilled renderer for headings, paragraphs, code, and semantic nodes.
4. Move mesh, sync, and advanced settings into secondary native panels.
5. Replace placeholder Servo integration with a working engine embedding path.
