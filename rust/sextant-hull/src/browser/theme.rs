//! Palette, layout metrics, and timing constants for the sextant-browser shell.
//!
//! Extracted verbatim from the `browser.rs` crate root as the first cut of the
//! browser module split. These are `pub` so the root and its sibling
//! chrome modules can glob-import them with `use crate::theme::*`.

use std::time::Duration;

pub const BG: u32 = 0x0010161d;
/// Background under the egui chrome strip (matches the panel frame fill); only
/// visible in sub-pixel gaps since egui paints its own panel background.
#[cfg(all(target_os = "windows", feature = "servo-backend"))]
pub const CHROME_BG: u32 = 0x000b131a;
#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub const APPLIANCE_CERT_TRUST_FILE: &str = "appliance-cert-trust.json";
pub const GUARD_CERT_SECTION_Y: u32 = 250;
#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub const GUARD_CERT_ROW_H: u32 = 42;
pub const PANEL: u32 = 0x0019232c;
pub const PANEL_ALT: u32 = 0x00202b35;
pub const PANEL_DARK: u32 = 0x000b1016;
pub const PANEL_HEADER: u32 = 0x00131d26;
pub const PANEL_SOFT: u32 = 0x00162028;
pub const FIELD: u32 = 0x000d1319;
pub const FIELD_FOCUS: u32 = 0x00172229;
pub const BUTTON_HOVER: u32 = 0x004a7488;
pub const BUTTON_BRIGHT: u32 = 0x000ec7e8;
pub const BUTTON_ACTIVE: u32 = 0x002fbf71;
pub const BUTTON_DISABLED: u32 = 0x00212a32;
pub const TEXT: u32 = 0x00dce7ef;
pub const TEXT_DIM: u32 = 0x0093a4b0;
pub const TEXT_SOFT: u32 = 0x00b8c6d2;
pub const TEXT_PLACEHOLDER: u32 = 0x006f7f8a;
pub const STATUS_OK: u32 = 0x002fbf71;
pub const STATUS_WARN: u32 = 0x00d9a441;
pub const STATUS_ERROR: u32 = 0x00e0524a;
// In-progress accent for pilot/runtime status (Xilem dashboard salvage). Reuses
// the cyan button accent so the bridge stays on its existing palette.
pub const STATUS_INFO: u32 = BUTTON_BRIGHT;
pub const BORDER: u32 = 0x00313d48;
pub const BORDER_SOFT: u32 = 0x0024333d;
pub const RAIL_WIDTH: u32 = 320;
pub const STATUS_BAR_H: u32 = 30;
pub const CHROME_H: u32 = 54;
pub const STRIP_H: u32 = 48;
pub const TAB_H: u32 = 46;
pub const PAGE_TAB_H: u32 = 34;
pub const PAGE_TAB_PAGER_W: u32 = 26;
pub const MAX_VISIBLE_PAGE_TABS: usize = 6;
pub const LOAD_BAR_H: u32 = 5;
pub const METRIC_Y: u32 = CHROME_H + STRIP_H + TAB_H + PAGE_TAB_H + 12;
pub const METRIC_H: u32 = 76;
pub const GLYPH_W: u32 = 5;
pub const GLYPH_GAP: u32 = 2;
pub const FRAME_REFRESH_IDLE: Duration = Duration::from_millis(1500);
pub const FRAME_REFRESH_DIRTY: Duration = Duration::from_millis(250);
pub const FRAME_REFRESH_INTERACTION: Duration = Duration::from_millis(48);
pub const FRAME_WARMUP_BUDGET: u8 = 6;
pub const FRAME_INTERACTION_WARMUP_BUDGET: u8 = 2;
pub const VIEWPORT_MOUSE_MOVE_MIN_INTERVAL: Duration = Duration::from_millis(33);
pub const VIEWPORT_MOUSE_MOVE_MIN_DISTANCE_PX: f32 = 2.0;
pub const VIEWPORT_TEXT_INPUT_DEBOUNCE: Duration = Duration::ZERO;
pub const VIEWPORT_INPUT_CAPTURE_SETTLE: Duration = Duration::from_millis(16);
pub const WINDOW_INPUT_SMOKE_FRAME_SETTLE: Duration = Duration::from_millis(64);
pub const OBSERVATION_WARMUP_IDLE_DELAY: Duration = Duration::from_millis(150);
pub const PERF_HISTORY_LIMIT: usize = 24;
pub const OPERATOR_DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
pub const WINDOW_SMOKE_DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
pub const DIRECT_INCOGNITO_CLEANUP_ARG: &str = "--cleanup-direct-incognito";
pub const HOSTED_DIRECT_CHROME_H: u32 = 72;
pub const HOSTED_DIRECT_LOG_DIR: &str = "hosted-direct-logs";
pub const HOSTED_DIRECT_SUMMARY_REFRESH: Duration = Duration::from_millis(750);
pub const HOSTED_DIRECT_DEBUG_TELEMETRY: bool = false;
pub const HOSTED_DIRECT_LOG_HEAD_BYTES: u64 = 64 * 1024;
pub const HOSTED_DIRECT_LOG_TAIL_BYTES: u64 = 256 * 1024;
