//! `BrowserApp` page-tab windowing — visible-count / window clamping / pager
//! rects and prev/next paging. Split out of the browser.rs god-impl as a
//! separate `impl BrowserApp` block; `use super::*` brings the struct, its
//! fields (reachable here as a descendant module), and the theme constants.
//! Methods are pub(crate) so the rest of the crate can call them unchanged.

use super::*;

impl BrowserApp {
    pub(crate) fn page_tab_visible_count(&self) -> usize {
        self.engine.get_tabs().len().min(MAX_VISIBLE_PAGE_TABS)
    }

    pub(crate) fn max_page_tab_window_start(&self) -> usize {
        let tabs_len = self.engine.get_tabs().len();
        tabs_len.saturating_sub(tabs_len.min(MAX_VISIBLE_PAGE_TABS))
    }

    pub(crate) fn clamp_page_tab_window(&mut self) {
        self.page_tab_window_start = self
            .page_tab_window_start
            .min(self.max_page_tab_window_start());
    }

    pub(crate) fn show_page_tab(&mut self, tab_id: Uuid) {
        let tabs = self.engine.get_tabs();
        let visible_count = tabs.len().min(MAX_VISIBLE_PAGE_TABS);
        if visible_count == 0 {
            self.page_tab_window_start = 0;
            return;
        }
        let Some(index) = tabs.iter().position(|tab| tab.id == tab_id) else {
            self.clamp_page_tab_window();
            return;
        };
        if index < self.page_tab_window_start {
            self.page_tab_window_start = index;
        } else if index >= self.page_tab_window_start + visible_count {
            self.page_tab_window_start = index + 1 - visible_count;
        }
        self.clamp_page_tab_window();
    }

    pub(crate) fn page_tab_overflowing(&self) -> bool {
        self.engine.get_tabs().len() > self.page_tab_visible_count()
    }

    pub(crate) fn can_page_tabs_previous(&self) -> bool {
        self.page_tab_overflowing() && self.page_tab_window_start > 0
    }

    pub(crate) fn can_page_tabs_next(&self) -> bool {
        self.page_tab_overflowing() && self.page_tab_window_start < self.max_page_tab_window_start()
    }

    pub(crate) fn compute_page_tab_pager_rects(&self, main_right: u32) -> (Rect, Rect) {
        let top = CHROME_H + STRIP_H + TAB_H + 3;
        (
            Rect {
                x: 24,
                y: top,
                w: PAGE_TAB_PAGER_W,
                h: 27,
            },
            Rect {
                x: main_right.saturating_sub(24 + PAGE_TAB_PAGER_W),
                y: top,
                w: PAGE_TAB_PAGER_W,
                h: 27,
            },
        )
    }

    pub(crate) fn compute_page_tab_rects(&self, main_right: u32) -> Vec<PageTabRegion> {
        let tabs = self.engine.get_tabs();
        if tabs.is_empty() {
            return Vec::new();
        }
        let overflowing = tabs.len() > MAX_VISIBLE_PAGE_TABS;
        let left = if overflowing {
            24 + PAGE_TAB_PAGER_W + 8
        } else {
            24
        };
        let top = CHROME_H + STRIP_H + TAB_H + 3;
        let gap = 8;
        let visible_count = tabs.len().min(MAX_VISIBLE_PAGE_TABS);
        let first_visible = self
            .page_tab_window_start
            .min(tabs.len().saturating_sub(visible_count));
        let pager_space = if overflowing {
            (PAGE_TAB_PAGER_W + gap) * 2
        } else {
            0
        };
        let visible_count_u32 = visible_count as u32;
        let available = main_right.saturating_sub(24 + 24 + pager_space);
        let tab_w = ((available.saturating_sub(gap * visible_count_u32.saturating_sub(1)))
            / visible_count_u32)
            .clamp(112, 210);
        tabs.into_iter()
            .skip(first_visible)
            .take(visible_count)
            .enumerate()
            .map(|(index, tab)| PageTabRegion {
                rect: Rect {
                    x: left + index as u32 * (tab_w + gap),
                    y: top,
                    w: tab_w,
                    h: 27,
                },
                tab_id: tab.id,
            })
            .collect()
    }

    pub(crate) fn page_tabs_previous(&mut self) {
        if !self.can_page_tabs_previous() {
            return;
        }
        self.page_tab_window_start = self.page_tab_window_start.saturating_sub(1);
        self.layout(self.window_size);
        self.last_status = self.page_tab_window_status();
        self.last_ok = true;
        self.validation.tab_control_seen = true;
    }

    pub(crate) fn page_tabs_next(&mut self) {
        if !self.can_page_tabs_next() {
            return;
        }
        self.page_tab_window_start =
            (self.page_tab_window_start + 1).min(self.max_page_tab_window_start());
        self.layout(self.window_size);
        self.last_status = self.page_tab_window_status();
        self.last_ok = true;
        self.validation.tab_control_seen = true;
    }

    pub(crate) fn page_tab_window_status(&self) -> String {
        let total = self.engine.get_tabs().len();
        if total == 0 {
            return "No tabs open.".to_string();
        }
        let visible = self.page_tab_rects.len().max(1);
        let first = self.page_tab_window_start + 1;
        let last = (self.page_tab_window_start + visible).min(total);
        format!("Showing tabs {first}-{last} of {total}.")
    }
}
