# 2026-04-18 — Workspace Compiles Clean

## Outcome

`cargo check` passes across all 15 crates with zero errors. This was the first clean compile of the full Sextant Rust workspace.

---

## Errors Fixed (by crate)

### sextant-hull

**Problem**: The entire view layer used an idealized Xilem API that didn't match Xilem 0.1.0.

| Error | Fix |
|-------|-----|
| `App`, `AppLauncher`, `WindowHandle` don't exist | `Xilem::new().run_windowed()` pattern |
| `list()` view doesn't exist | `Vec<_>` collect + `flex(vec)` |
| `.scale()`, `.spacing()`, `.padding()` on flex | Removed — not in 0.1.0 |
| Colors as `[f32; 4]` arrays | `Color::rgba(f64, f64, f64, f64)` from `peniko` |
| `View` trait | `MasonryView` trait |
| `Axis` not found | Import from `xilem` crate root |
| `#[tokio::main] async fn main()` | Sync `fn main()` + `Runtime::new().block_on()` |
| `winit::error::EventLoopError` version conflict | Changed to `.expect()`, removed `Result` return |
| `if/else` branches returning different view types | Pre-compute conditional data as strings before view tree |
| `url.as_bytes()` on `&Url` | `url.as_str().as_bytes()` |
| Mutex guard held while calling `&mut self` methods | Scoped blocks so guards drop before `self.add_log()` etc. |
| `identity.did` move out of `Drop` type | `.clone()` |
| Missing deps: sextant-log, sextant-pq, sextant-privacy, sextant-inference, uuid (serde), serde, serde_json | Added to `Cargo.toml` |

### sextant-vault

| Error | Fix |
|-------|-----|
| `bip39::Seed::new()` removed in v2 | `mnemonic.to_seed("") -> [u8; 64]` |
| `HmacSha512` type alias out of scope | Moved to module level |
| `KeyType` doesn't impl `Zeroize` | `#[zeroize(skip)]` on field |
| `HmacSha512::new_from_slice` ambiguous | `<HmacSha512 as Mac>::new_from_slice(...)` |
| `decrypt_data` spurious `?` on last expression | Removed `?` |
| `EcSigningKey::from_bytes` unstable slice method | `(&derived_bytes[..32]).into()` |
| `get_identities` method missing | Added method to `CitadelVault` |

### sextant-engine

| Error | Fix |
|-------|-----|
| `wgpu::DeviceDescriptor` `features`/`limits` | `required_features`/`required_limits` (wgpu 0.20+) |
| `sextant_bridge::PrivacyLevel` not re-exported | Use `sextant_privacy::PrivacyLevel` directly |
| Servo `libclang` required at build | Made Servo an optional feature: `servo-backend` |
| Servo `branch = "master"` doesn't exist | Changed to `branch = "main"` |

### sextant-wake

| Error | Fix |
|-------|-----|
| `rusqlite` FTS5 feature in 0.31 | Removed explicit `fts5` feature (bundled by default) |
| `rusqlite` version conflict with Servo | Upgraded to `0.37` |
| `f32` type ambiguity on `importance` | Explicit `: f32` annotation and `1.0_f32` suffix |

### sextant-log

| Error | Fix |
|-------|-----|
| `rusqlite` version conflict | Upgraded to `0.37` |

### sextant-inference

| Error | Fix |
|-------|-----|
| `serde_json` used but not declared | Added to `Cargo.toml` |
| `.collect()` type ambiguous (3 places) | `.collect::<Vec<f32>>()` |

### sextant-pilot

| Error | Fix |
|-------|-----|
| `sextant_engine::IntentType` doesn't exist | Removed from import |
| `chrono` used but not declared | Added to `Cargo.toml` |
| `navigate_with_fallback` called with wrong arity | Added `persona_id` argument |
| `LocalBrain` non-exhaustive match | Added OpenAI/Anthropic/Gemini arms |
| `self.active_persona` vs `self.active_persona_id` | Fixed field reference |

### sextant-mesh

| Error | Fix |
|-------|-----|
| `wireguard-uapi` (depends on `neli`, Linux-only) | Removed — not used in source |

### sextant-bridge

| Error | Fix |
|-------|-----|
| `.into()` type inference ambiguous | Explicit `Option<String>` annotation + `.to_string()` |

### All crates

| Error | Fix |
|-------|-----|
| `uuid` missing `serde` feature | Added `features = ["v4", "serde"]` to all 13 Cargo.toml files |

---

## Build Environment Setup

Created `D:\Paul\Software Projects\Sextant\rust\.cargo\config.toml` with:

```toml
[target.x86_64-pc-windows-msvc]
linker = "C:\\Program Files (x86)\\Microsoft Visual Studio\\18\\BuildTools\\VC\\Tools\\MSVC\\14.50.35717\\bin\\Hostx64\\x64\\link.exe"

[env]
VCINSTALLDIR = "C:\\Program Files (x86)\\Microsoft Visual Studio\\18\\BuildTools\\VC"
INCLUDE = "..."  # MSVC + Windows SDK include paths
LIB = "..."      # MSVC + Windows SDK lib paths
```

This resolves two persistent Windows build issues:
1. Git Bash's `link.exe` (Unix linker) shadowing MSVC's `link.exe`
2. `mozangle` and other native deps requiring `VCINSTALLDIR` at build time
