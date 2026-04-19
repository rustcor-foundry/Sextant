# Next Work

Active priority stack. See [completed.md](completed.md) for session history.

**North Star:** A working sovereign browser where the user can type an intent, the Pilot reasons about it, navigates to a page, distills the content, and records it in the Digital Wake — all without touching a cloud service unless explicitly configured.

---

## Priority Stack

### 0. Keep `cargo run` Launch Healthy (immediate, ~15 min per pass)

`cargo check --workspace` and the Pilot regression suite currently pass. `cargo run -p sextant-hull` has also been exercised again and did not panic inside the launch-check timeout window, so this lane is now about preserving that state whenever hull/runtime changes land.

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull
```

Watch for: GPU init failures, Xilem layout panics, thread/async runtime issues.

### 1. Full In-Window Workflow Validation (~1-2 hours)

The async command-channel pattern is already in place, and the hull already wires command submission, air-gap/privacy controls, Wake search/consolidation, provider settings, consent actions, and tab actions. The validation panel is now session-aware, tracks coverage across the main workflows, shows remaining checks, and supports `RESET VALIDATION` for a fresh run. The next highest-value work is a real click-through pass that exercises the app end-to-end in the window and fixes anything that still only works in tests or partial runtime paths.

Focus checks:
1. start with `RESET VALIDATION` so the checklist reflects the current run cleanly
2. submit intent from the hull and verify the result updates dashboard, viewport, Wake, and Captain's Log
3. exercise consent authorize/deny from the hull and confirm the UI truthfully updates
4. switch tabs, close tabs, and verify active-page/engine-status synchronization
5. test provider apply/test/save/load from the settings panel
6. confirm air-gap and privacy changes surface clearly in the UI and runtime state

### 2. Truthful UX and Error Surfacing (~2-4 hours)

The native hull is already much more honest than earlier passes, so this lane is now mostly follow-up based on what real validation uncovers:
1. tighten any stale or misleading status text
2. surface startup/runtime failures in-window instead of only via stderr/logs
3. make unfinished panels explicit without implying they are wired
4. remove any remaining "looks wired but is not" edge cases

### 3. Real Inference Configuration Path (~2-4 hours)

Provider switching and settings controls are implemented, and the hull loads environment keys on startup. The remaining work is to make the configured provider path fully real and clearly persisted:
- verify applied provider changes always switch the active Pilot brain
- harden vault save/load behavior for provider settings
- confirm local inference and air-gap interactions behave as intended
- remove any leftover placeholder-model assumptions where they still exist

### 4. Servo Rendering Backend (~1-2 days)

Enable the `servo-backend` feature to get real page rendering. Requires:
1. Install LLVM/clang for `bindgen` (used by mozangle, a Servo dep)
   ```bash
   choco install llvm
   ```
2. Build with feature: `cargo build -p sextant-engine --features servo-backend`
3. Wire `SextantEngine::navigate()` to actually call Servo's `WebView`

Until then, the simulated distiller in `sextant-engine` returns mocked content.

### 5. Warning and Dependency Cleanup (~1-2 hours)

The workspace is currently green, but there is still follow-up cleanup worth doing:
- reduce remaining low-signal warnings in less-active crates
- inspect the `xml5ever v0.16.2` future-incompatibility notice and decide whether to upgrade or track it explicitly
- keep the workspace warning budget low enough that new regressions stand out

### 6. CI Depth

Basic Rust workspace CI is already in place on Gitea. The next CI pass should add depth rather than bootstrap:
- consider a dedicated hull smoke check strategy
- add focused crate test jobs if failures become noisy
- keep CI aligned with the native product path rather than the legacy simulator

---

## Known Blockers

| Blocker | Affects | Notes |
|---------|---------|-------|
| Interactive native launch validation is still manual | Items 0-2 | We can compile/test automatically, and the hull now has a session-aware checklist, but full click-through still needs operator exercise |
| Real cloud/local provider validation depends on credentials/services | Item 3 | Behavior is partly environment-dependent |
| libclang not installed | Servo feature | `choco install llvm` resolves it |

---

## Nice-to-Have

- **Mesh panel**: `is_mesh_open` exists, but mesh remains a later native pass
- **PQ identity display**: `active_pq_identity` in state, not shown in hull
- **Sync UI**: `SextantSync` is wired in state but no import/export UI
