//! `BrowserApp` page/tab actions — selector fill/click/submit and
//! new/close/reload/back/forward tab operations.

use super::*;

impl BrowserApp {
    pub(crate) fn fill_selector_current_page(
        &mut self,
        selector: &str,
        value: &str,
    ) -> Result<sextant_engine::BrowserInteractionResult, String> {
        let result = self
            .engine
            .fill_selector_current_page(selector, value)
            .map_err(|error| {
                format!("native fill interaction failed for {}: {}", selector, error)
            })?;
        self.check_interaction_guard(&result, "fill")?;
        Ok(result)
    }

    pub(crate) fn click_selector_current_page(
        &mut self,
        selector: &str,
    ) -> Result<sextant_engine::BrowserInteractionResult, String> {
        let result = self
            .engine
            .click_selector_current_page(selector)
            .map_err(|error| {
                format!(
                    "native click interaction failed for {}: {}",
                    selector, error
                )
            })?;
        self.check_interaction_guard(&result, "click")?;
        Ok(result)
    }

    pub(crate) fn submit_selector_current_page(
        &mut self,
        selector: &str,
    ) -> Result<sextant_engine::BrowserInteractionResult, String> {
        let result = self
            .engine
            .submit_selector_current_page(selector)
            .map_err(|error| {
                format!(
                    "native submit interaction failed for {}: {}",
                    selector, error
                )
            })?;
        self.check_interaction_guard(&result, "submit")?;
        Ok(result)
    }

    pub(crate) fn new_tab(&mut self) {
        self.clear_viewport_input_lane();
        let id = self.engine.open_tab();
        self.show_page_tab(id);
        self.address_input = "about:blank".to_string();
        self.latest_frame = None;
        self.last_frame_viewport = None;
        self.last_status = format!("Created tab {}.", short_id(id));
        self.last_ok = true;
        self.validation.tab_control_seen = true;
        self.layout(self.window_size);
        let _ = self.record_log("new tab", LogStatus::Success);
    }

    pub(crate) fn close_tab(&mut self) {
        self.clear_viewport_input_lane();
        let Some(tab) = self.active_tab().cloned() else {
            self.last_status = "No active tab to close.".to_string();
            self.last_ok = false;
            return;
        };

        match self.engine.close_tab(&tab.id) {
            Ok(()) => {
                self.sync_address_to_active_tab();
                if let Some(active_id) = self.active_tab().map(|tab| tab.id) {
                    self.show_page_tab(active_id);
                } else {
                    self.clamp_page_tab_window();
                }
                self.latest_frame = None;
                self.last_frame_viewport = None;
                self.last_status = format!("Closed tab {}.", short_id(tab.id));
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                self.layout(self.window_size);
                self.refresh_render_bridge_frame_after_visible_tab_change();
                let _ = self.record_log("close tab", LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Close failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("close tab", LogStatus::Failure(error));
            }
        }
    }

    pub(crate) fn reload(&mut self) {
        let started = Instant::now();
        match self.engine.reload_active_tab() {
            Ok(status) => {
                let elapsed = started.elapsed();
                self.record_perf("navigation", elapsed, "reload");
                self.perf.frame = None;
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.last_status = format!(
                    "Reloaded active tab with {} in {}. Frame capture queued.",
                    backend(status),
                    format_duration(elapsed)
                );
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("reload", LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "reload failed");
                self.last_status = format!("Reload failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("reload", LogStatus::Failure(error));
            }
        }
    }

    pub(crate) fn back(&mut self) {
        let started = Instant::now();
        match self.engine.go_back_active_tab() {
            Ok(status) => {
                let elapsed = started.elapsed();
                self.record_perf("navigation", elapsed, "back");
                self.perf.frame = None;
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.last_status = format!(
                    "Went back with {} in {}. Frame capture queued.",
                    backend(status),
                    format_duration(elapsed)
                );
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("back", LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "back failed");
                self.last_status = format!("Back unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("back", LogStatus::Failure(error));
            }
        }
    }

    pub(crate) fn forward(&mut self) {
        let started = Instant::now();
        match self.engine.go_forward_active_tab() {
            Ok(status) => {
                let elapsed = started.elapsed();
                self.record_perf("navigation", elapsed, "forward");
                self.perf.frame = None;
                self.sync_address_to_active_tab();
                self.begin_frame_warmup();
                self.last_status = format!(
                    "Went forward with {} in {}. Frame capture queued.",
                    backend(status),
                    format_duration(elapsed)
                );
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log("forward", LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "forward failed");
                self.last_status = format!("Forward unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("forward", LogStatus::Failure(error));
            }
        }
    }
}
