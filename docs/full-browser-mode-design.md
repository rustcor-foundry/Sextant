# Full Browser Mode — Design Reference

Purpose: **preserve the look of full browser mode.** The bridge shell
(`sextant-browser`) is the canonical base; the parked Xilem hull
(`SEXTANT_FULL_SHELL=1`) holds dashboard ideas worth salvaging. This file
captures both so the look survives refactors and any future Xilem removal.

## Direction (locked 2026-06-15): high-res retro via egui

Decision: **keep the retro terminal-HUD look, but render it through egui** for
**anti-aliased text + true HiDPI** — the "ironic high-res retro." The current
softbuffer bridge shell's 5×7 bitmap font is what reads low-res; egui gives crisp
AA + native DPI scaling, and "retro" is preserved as an egui *theme* (dark/cyan
palette, sharp corners `Rounding::ZERO`, monospace ALL-CAPS, cyan-edged panels).

Validated by the **`--retro-egui-proof`** mode (`run_retro_egui_proof` /
`apply_retro_egui_theme` / `draw_retro_egui_proof` in `browser.rs`): a retro HUD
rendered through egui on this **GPU-less box via the software rasterizer** —
crisp AA monospace, a `HiDPI x1.0–2.0` toggle that scales razor-sharp, acceptable
CPU perf. egui is already the direct-lane chrome, so this unifies **both lanes**
on egui (themed retro) and lets us retire the hand-rolled bitmap-font renderer.

Enabler kept regardless: `ChromeBackend::render(..., full: bool)` — a `full`
flag so the software rasterizer can clear/rasterize/present the **whole window**
(the strip-only path was built for the thin direct chrome and ghosted a
full-window UI). Real-bridge port = move each `draw_*_panel` onto the egui theme
with the *actual* data (tabs/Wake/Log/Guard/Sense/Perf/Validation/Settings + the
AI rail), reusing the §A palette below as egui `Visuals`. See [[gui_toolkit_decision]].

Rule of thumb: **new UI must stay consistent with the bridge palette/components
below.** Do not delete Xilem view code until its salvage items here are either
ported or have reference screenshots captured.

Reference screenshots (capture during GUI QA, drop in `docs/design/`):
- `bridge-shell-*.png` — `sextant-browser` full mode, each tab.
- `xilem-dashboard.png` — `SEXTANT_FULL_SHELL=1` first paint (renders, then
  crashes on click — first paint is enough for the screenshot).

---

## A. Bridge shell — canonical look (preserve as-is)

Custom softbuffer pixel rendering in `rust/sextant-hull/src/browser.rs`. Dark,
flat, terminal-adjacent; cyan accent + green/amber status semantics; a 5×7
bitmap font.

### Palette (constants near `browser.rs:61-88`)

| Token | Hex | Role |
|---|---|---|
| `BG` | `#10161D` | App background (deep navy-black) |
| `CHROME_BG` | `#0B131A` | Under the chrome strip |
| `PANEL` | `#19232C` | Panel surface |
| `PANEL_ALT` | `#202B35` | Raised panel / button face |
| `PANEL_DARK` | `#0B1016` | Recessed panel |
| `PANEL_HEADER` | `#131D26` | Panel header band |
| `PANEL_SOFT` | `#162028` | Soft panel fill |
| `FIELD` / `FIELD_FOCUS` | `#0D1319` / `#172229` | Text field idle / focused |
| `BUTTON_HOVER` | `#4A7488` | Button hover |
| `BUTTON_BRIGHT` | `#0EC7E8` | **Cyan accent** (highlights, top edge) |
| `BUTTON_ACTIVE` | `#2FBF71` | Active/selected (green) |
| `BUTTON_DISABLED` | `#212A32` | Disabled face |
| `TEXT` | `#DCE7EF` | Primary text |
| `TEXT_SOFT` | `#B8C6D2` | Secondary text |
| `TEXT_DIM` | `#93A4B0` | Tertiary / hints |
| `TEXT_PLACEHOLDER` | `#6F7F8A` | Placeholder |
| `STATUS_OK` | `#2FBF71` | OK / online / pass (green) |
| `STATUS_WARN` | `#D9A441` | Warn / blocked (amber) |
| `BORDER` / `BORDER_SOFT` | `#313D48` / `#24333D` | Strokes |

### Layout constants (`browser.rs:89-99`)

`RAIL_WIDTH 320` (right AI rail) · `CHROME_H 54` (top bar) · `STRIP_H 48`
(tab strip) · `TAB_H 46` · `PAGE_TAB_H 34` (page-tab strip) · `METRIC_H 76`
(metric cards) · `STATUS_BAR_H 30` · `LOAD_BAR_H 5`. Font: 5×7 bitmap,
`GLYPH_W 5` / `GLYPH_GAP 2` (`glyph()` / `draw_char` / `draw_text`).

### Structure (top → bottom, left → right)

- **Top bar** (`draw_top_bar`): address field, mode chips (Agent/Assisted/
  Observe/Direct/Incognito), controls (`draw_controls`).
- **Tab strip** (`draw_pill` per view): `BROWSER WAKE LOG GUARD SENSE PERF
  VALIDATION SETTINGS` + an AI-ready status chip. Active tab = cyan-edged pill.
- **Page-tab strip** (`draw_page_tab_strip`) + load bar (`draw_page_load_bar`).
- **Metric cards** (`draw_metric_cards`): backend/threads/layout/mem row.
- **Main panel** (per `MainView`): `draw_page_panel`, `draw_wake_panel`,
  `draw_log_panel`, `draw_guard_panel`, `draw_perception_panel`,
  `draw_perf_panel`, `draw_validation_panel`, `draw_settings_panel`.
- **Right AI rail** (`draw_ai_rail`, width 320): pilot/Wake side surface.
- **Status bar** (`draw_status_bar`): `last_status` line.

### Reusable components (match these for any new UI)

- `draw_pill(rect, label, active)` — tab pills.
- `draw_button(rect, label, hovered, enabled)` — face `PANEL_ALT`, hover
  `BUTTON_HOVER`, cyan/`BORDER_SOFT` top edge, centered text.
- `draw_panel_surface(rect, accent)` — panel bg + accent header.
- `draw_text` / `draw_text_centered` / `fill_rect` / `stroke_rect`.
- `main_panel_rect(rail_x, height)` — standard content area; `right_rail_x(width)`.

The **Settings tab** (`draw_settings_panel` / `settings_backend_button_rects`,
added 2026-06-15) is the reference for "new panel done right" — it uses only
the components above.

---

## B. Xilem hull dashboard — salvage source (capture before deleting)

Masonry/Xilem retained UI in `rust/sextant-hull/src/views.rs` (parked; see
[[gui_toolkit_decision]] in memory and `current-state.md`). Richer dashboard
layout with strong **color-coded state semantics**. Its palette (Masonry
`Color::rgba`, 0-1 floats):

| Role | rgba | ~Hex |
|---|---|---|
| Ready / idle-ok | `0.0, 0.95, 0.45` | `#00F273` |
| Info / in-progress | `0.7, 0.85, 1.0` | `#B3D9FF` |
| Warn / booting | `1.0, 0.8, 0.3` | `#FFCC4D` |
| Consent pending | `1.0, 0.75, 0.3` | `#FFBF4D` |
| Error | `1.0, 0.3, 0.3` | `#FF4D4D` |
| Title | `0.9, 0.9, 0.95` | `#E6E6F2` |
| Muted | `0.6, 0.6, 0.6` | `#999999` |

### Salvage candidates (fold into the bridge shell over time)

1. ✅ **Color-coded pilot/runtime status** — *ported 2026-06-15.*
   `pilot_status_color()` colors the AI-rail status chip: green idle/complete/
   authorized, cyan (`STATUS_INFO`) in-progress (reasoning/navigating/distilling/
   perceiving/planning), amber consent/blocked, red (`STATUS_ERROR`) failed.
   Replaced the old green/amber binary and fixed the doubled `PILOT PILOT` label.
   (Original Xilem source: `runtime_status_color`, `views.rs:754`.)
2. ✅ **Intent-bar presets** — *ported 2026-06-15.* `AI_RAIL_PRESETS` shows a
   QUICK INTENTS row (SUMMARIZE / DISTILL HERE) in the AI rail's consent band
   when no consent is pending and the mode allows native intents; clicking runs
   the intent through the local model brain (`run_preset_intent`). (Original
   Xilem source: `intent_bar_view` — `PRESET SEARCH`/`COMMAND`/etc.)
3. **Startup phase status** (`startup_status_color`, `views.rs:765`):
   Ready/Degraded/Booting/WarmingUp coloring. *N/A for the bridge* — it has no
   `StartupPhase` concept; readiness already shows via the status-bar `last_ok`
   dot + `ai_status_label()`.
4. ✅ **Validation badges** — *already present in the bridge.*
   `draw_validation_panel` color-codes each row via `validation_status_color`
   (Pass→green, Attention→amber, Waiting→dim) with a status dot — matches the
   Xilem `validation_badge` look. No port needed.
5. ✅ **Consent surface** — *already present.* The bridge AI rail shows
   `AUTHORIZE`/`DENY` buttons and the amber `AWAITING CONSENT` state (now via
   `pilot_status_color`) plus the consent message. No port needed.
6. **Provider picker** — Local backends done via the **Settings tab** (#1). The
   online-provider picker (Gemini/OpenAI/Anthropic + API-key entry) is still
   Xilem-only and **deferred**: a larger feature (secret entry/storage) beyond
   look-preservation, to revisit when online providers are wired into the bridge.

---

## C. Preservation checklist (before any Xilem-stack removal)

- [ ] Capture `xilem-dashboard.png` (first paint) + `bridge-shell-*.png` per tab.
- [x] Port or explicitly defer each Salvage candidate in §B *(2026-06-15: #1
  pilot-status color + #2 intent presets ported; #4 badges / #5 consent already
  in the bridge; #3 N/A; #6 online provider picker deferred).*
- [x] Confirm new bridge UI uses only the §A palette/components *(color salvage,
  presets, and Settings tab all use the existing palette/`draw_*` helpers).*
- [ ] **GUI QA pass** of the bridge full mode (status colors cycle; QUICK INTENTS
  placement; per-tab look) + capture the screenshots above.
- [ ] Only then remove the `sextant-hull` Xilem bin + vendored vello/masonry/xilem.
