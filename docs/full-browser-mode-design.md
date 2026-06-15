# Full Browser Mode — Design Reference

Purpose: **preserve the look of full browser mode.** The bridge shell
(`sextant-browser`) is the canonical base; the parked Xilem hull
(`SEXTANT_FULL_SHELL=1`) holds dashboard ideas worth salvaging. This file
captures both so the look survives refactors and any future Xilem removal.

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

1. **Color-coded pilot/runtime status** (`runtime_status_color`,
   `views.rs:754`): idle→green, reasoning/navigating/distilling→blue,
   awaiting-consent→amber. The bridge shows `pilot_status` as plain text in the
   AI rail — adopt this state→color mapping there.
2. **Intent bar with preset buttons** (`intent_bar_view`): `PRESET SEARCH`,
   `PRESET CONSENT`, `COMMAND`, `NEW TAB`, `AIR GAP`, `PRIVACY` quick actions.
   The bridge has the address/intent bar but no one-tap presets.
3. **Startup phase status** (`startup_status_color`, `views.rs:765`):
   Ready/Degraded/Booting/WarmingUp coloring — nice for the status bar.
4. **Validation badges/checklist** (`validation_badge`, `validation_item_views`):
   PROVIDER/etc. pass/warn/fail badges. The bridge has a Validation tab; adopt
   the badge styling.
5. **Consent flow surface** (`AwaitingConsent` amber state + next-step prompt) —
   richer than the bridge's current consent line.
6. **Provider picker** (Gemini/OpenAI/Anthropic/Local) — partly covered by the
   new Settings tab (Local backends); the online-provider picker is still
   Xilem-only.

---

## C. Preservation checklist (before any Xilem-stack removal)

- [ ] Capture `xilem-dashboard.png` (first paint) + `bridge-shell-*.png` per tab.
- [ ] Port or explicitly defer each Salvage candidate in §B.
- [ ] Confirm new bridge UI uses only the §A palette/components.
- [ ] Only then remove the `sextant-hull` Xilem bin + vendored vello/masonry/xilem.
