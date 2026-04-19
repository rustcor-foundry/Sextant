# Next Work

Active priority stack. See [completed.md](completed.md) for session history.

**North Star:** A working sovereign browser where the user can type an intent, the Pilot reasons about it, navigates to a page, distills the content, and records it in the Digital Wake — all without touching a cloud service unless explicitly configured.

---

## Priority Stack

### 0. Verify `cargo run` and Window Launch (immediate, ~15 min)

The workspace compiles but `cargo run -p sextant-hull` has not been confirmed. Run it, fix any runtime panics (GPU init, missing resources, etc.).

```bash
cd "D:/Paul/Software Projects/Sextant/rust"
cargo run -p sextant-hull
```

Watch for: GPU init failures, Xilem layout panics, thread/async runtime issues.

### 1. Wire Async UI Actions (~2-3 hours)

All buttons currently log "async wiring pending." The state methods exist and are correct — they just need to be called. The pattern is: button callback queues a command string, a separate async loop reads commands and calls the appropriate `state.*` method.

Xilem doesn't support async directly in callbacks. The standard approach is:
- Store a `tokio::sync::mpsc::Sender<Command>` in state
- Button callbacks send a `Command` variant
- A background task receives commands, runs the async method, sends results back via another channel

Actions to wire, in priority order:
1. **Toggle Air-Gap** — `state.toggle_airgap()`
2. **Cycle Privacy** — `state.cycle_privacy()`
3. **Wake Search** — `state.search_wake()`
4. **Wake Consolidate** — `state.wake.lock().consolidate()`
5. **Navigate via Pilot** — `state.pilot.navigate_with_fallback(url, persona_id)`
6. **Mesh Chat Send** — `state.send_mesh_chat()`

### 2. Intent → Pilot → Engine → Wake Loop (~4-6 hours)

The core browser loop. When user submits an intent:
1. Pilot reasons about it (`reason_about_intent`)
2. Pilot navigates (`navigate_with_fallback`) — currently simulated in engine
3. Engine returns `DistilledPage`
4. Wake records the page
5. Hull updates `active_page` and `engine_status`

This requires the command channel pattern from #1 to be in place first.

### 3. Persist Wake and Log to Disk (~30 min)

Currently both use `new_in_memory()`. Switch to `DigitalWake::open(path)` and `CaptainsLog::open(path)` with a user data directory (e.g. `%APPDATA%\Sextant\` on Windows).

```rust
let data_dir = dirs::data_dir().unwrap().join("Sextant");
std::fs::create_dir_all(&data_dir).ok();
let wake = DigitalWake::open(data_dir.join("wake.db"))?;
```

Add `dirs = "5"` to hull's `Cargo.toml`.

### 4. Real Inference Backend (~2-4 hours)

The `GeminiBrain` is created with a placeholder key. To make it actually work:
- Read the API key from `state.gemini_key` (which can be persisted to vault)
- The `InferenceBrain` trait already has `reason_about_intent()` — just call it

For local inference, `LocalBrain` points to `http://localhost:8080` (llama.cpp). Wire up the AI-gap toggle to auto-switch to local brain.

### 5. Servo Rendering Backend (~1-2 days)

Enable the `servo-backend` feature to get real page rendering. Requires:
1. Install LLVM/clang for `bindgen` (used by mozangle, a Servo dep)
   ```bash
   choco install llvm
   ```
2. Build with feature: `cargo build -p sextant-engine --features servo-backend`
3. Wire `SextantEngine::navigate()` to actually call Servo's `WebView`

Until then, the simulated distiller in `sextant-engine` returns mocked content.

### 6. CI Pipeline

Add a GitHub Actions / Gitea workflow:
```yaml
- cargo check --workspace
- cargo test --workspace
```

---

## Known Blockers

| Blocker | Affects | Notes |
|---------|---------|-------|
| Xilem has no async callback support | Items 1-4 | Need command-channel pattern |
| No Gemini key wired | Real AI | Placeholder "sk-placeholder" in use |
| libclang not installed | Servo feature | `choco install llvm` resolves it |

---

## Nice-to-Have

- **Settings panel**: UI is toggled (`is_settings_open`) but the settings view is not implemented
- **Mesh panel**: Same — `is_mesh_open` flag exists, no render
- **Tab close/new**: Tab list renders but add/close not wired
- **PQ identity display**: `active_pq_identity` in state, not shown in hull
- **Sync UI**: `SextantSync` is wired in state but no import/export UI
