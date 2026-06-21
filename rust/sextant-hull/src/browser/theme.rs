//! Palette, layout metrics, and timing constants for the sextant-browser shell.
//!
//! Extracted verbatim from the `browser.rs` crate root as the first cut of the
//! browser module split. These are `pub(crate)` so the root and its sibling
//! chrome modules can glob-import them with `use crate::theme::*`.

use std::time::Duration;

pub(crate) const BG: u32 = 0x0010161d;
/// Background under the egui chrome strip (matches the panel frame fill); only
/// visible in sub-pixel gaps since egui paints its own panel background.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub(crate) const CHROME_BG: u32 = 0x000b131a;
#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub(crate) const APPLIANCE_CERT_TRUST_FILE: &str = "appliance-cert-trust.json";
pub(crate) const GUARD_CERT_SECTION_Y: u32 = 250;
#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub(crate) const GUARD_CERT_ROW_H: u32 = 42;
pub(crate) const PANEL: u32 = 0x0019232c;
pub(crate) const PANEL_ALT: u32 = 0x00202b35;
pub(crate) const PANEL_DARK: u32 = 0x000b1016;
pub(crate) const PANEL_HEADER: u32 = 0x00131d26;
pub(crate) const PANEL_SOFT: u32 = 0x00162028;
pub(crate) const FIELD: u32 = 0x000d1319;
pub(crate) const FIELD_FOCUS: u32 = 0x00172229;
pub(crate) const BUTTON_HOVER: u32 = 0x004a7488;
pub(crate) const BUTTON_BRIGHT: u32 = 0x000ec7e8;
pub(crate) const BUTTON_ACTIVE: u32 = 0x002fbf71;
pub(crate) const BUTTON_DISABLED: u32 = 0x00212a32;
pub(crate) const TEXT: u32 = 0x00dce7ef;
pub(crate) const TEXT_DIM: u32 = 0x0093a4b0;
pub(crate) const TEXT_SOFT: u32 = 0x00b8c6d2;
pub(crate) const TEXT_PLACEHOLDER: u32 = 0x006f7f8a;
pub(crate) const STATUS_OK: u32 = 0x002fbf71;
pub(crate) const STATUS_WARN: u32 = 0x00d9a441;
pub(crate) const STATUS_ERROR: u32 = 0x00e0524a;
// In-progress accent for pilot/runtime status (Xilem dashboard salvage). Reuses
// the cyan button accent so the bridge stays on its existing palette.
pub(crate) const STATUS_INFO: u32 = BUTTON_BRIGHT;
pub(crate) const BORDER: u32 = 0x00313d48;
pub(crate) const BORDER_SOFT: u32 = 0x0024333d;
pub(crate) const RAIL_WIDTH: u32 = 320;
pub(crate) const STATUS_BAR_H: u32 = 30;
pub(crate) const CHROME_H: u32 = 54;
pub(crate) const STRIP_H: u32 = 48;
pub(crate) const TAB_H: u32 = 46;
pub(crate) const PAGE_TAB_H: u32 = 34;
pub(crate) const PAGE_TAB_PAGER_W: u32 = 26;
pub(crate) const MAX_VISIBLE_PAGE_TABS: usize = 6;
pub(crate) const LOAD_BAR_H: u32 = 5;
pub(crate) const METRIC_Y: u32 = CHROME_H + STRIP_H + TAB_H + PAGE_TAB_H + 12;
pub(crate) const METRIC_H: u32 = 76;
pub(crate) const GLYPH_W: u32 = 5;
pub(crate) const GLYPH_GAP: u32 = 2;
pub(crate) const FRAME_REFRESH_IDLE: Duration = Duration::from_millis(1500);
pub(crate) const FRAME_REFRESH_DIRTY: Duration = Duration::from_millis(250);
pub(crate) const FRAME_REFRESH_INTERACTION: Duration = Duration::from_millis(48);
pub(crate) const FRAME_WARMUP_BUDGET: u8 = 6;
pub(crate) const FRAME_INTERACTION_WARMUP_BUDGET: u8 = 2;
pub(crate) const VIEWPORT_MOUSE_MOVE_MIN_INTERVAL: Duration = Duration::from_millis(33);
pub(crate) const VIEWPORT_MOUSE_MOVE_MIN_DISTANCE_PX: f32 = 2.0;
pub(crate) const VIEWPORT_TEXT_INPUT_DEBOUNCE: Duration = Duration::ZERO;
pub(crate) const VIEWPORT_INPUT_CAPTURE_SETTLE: Duration = Duration::from_millis(16);
pub(crate) const WINDOW_INPUT_SMOKE_FRAME_SETTLE: Duration = Duration::from_millis(64);
pub(crate) const OBSERVATION_WARMUP_IDLE_DELAY: Duration = Duration::from_millis(150);
pub(crate) const PERF_HISTORY_LIMIT: usize = 24;
pub(crate) const OPERATOR_DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const WINDOW_SMOKE_DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const DIRECT_INCOGNITO_CLEANUP_ARG: &str = "--cleanup-direct-incognito";
pub(crate) const HOSTED_DIRECT_CHROME_H: u32 = 72;
pub(crate) const HOSTED_DIRECT_LOG_DIR: &str = "hosted-direct-logs";
pub(crate) const HOSTED_DIRECT_SUMMARY_REFRESH: Duration = Duration::from_millis(750);
pub(crate) const HOSTED_DIRECT_DEBUG_TELEMETRY: bool = false;
pub(crate) const HOSTED_DIRECT_LOG_HEAD_BYTES: u64 = 64 * 1024;
pub(crate) const HOSTED_DIRECT_LOG_TAIL_BYTES: u64 = 256 * 1024;
