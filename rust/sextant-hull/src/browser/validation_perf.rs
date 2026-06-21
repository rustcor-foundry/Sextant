//! `BrowserApp` validation + perf + misc accessors (validation rows, perf
//! sampling, active-tab/action-enabled helpers, address sync, log refresh).

use super::*;

impl BrowserApp {
    pub(crate) fn validation_rows(&self) -> Vec<ValidationRow> {
        let active_url = self
            .active_tab()
            .and_then(|tab| tab.url.as_ref())
            .map(short_url)
            .unwrap_or_else(|| "no URL yet".to_string());
        let distilled_title = self
            .active_tab()
            .and_then(|tab| tab.distilled_page.as_ref())
            .map(|page| page.title.clone());
        let frame_ready = self
            .latest_frame
            .as_ref()
            .map(|frame| frame.width > 0 && frame.height > 0 && !frame.pixels.is_empty())
            .unwrap_or(false);
        let has_page = distilled_title.is_some();

        vec![
            ValidationRow {
                label: "ACTIVE TAB",
                status: if self.active_tab().is_some() {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: format!("{} tab(s) in engine state", self.engine.get_tabs().len()),
            },
            ValidationRow {
                label: "NAVIGATION",
                status: if self.validation.navigation_seen {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: active_url,
            },
            ValidationRow {
                label: "VIEWPORT",
                status: if frame_ready || self.validation.frame_seen {
                    ValidationStatus::Pass
                } else if has_page {
                    ValidationStatus::Attention
                } else {
                    ValidationStatus::Waiting
                },
                detail: if frame_ready {
                    "Servo frame captured in-window".to_string()
                } else if has_page {
                    "Reader fallback visible; Servo frame not captured yet".to_string()
                } else {
                    "Press GO, then wait for frame warm-up".to_string()
                },
            },
            ValidationRow {
                label: "BROWSER INPUT",
                status: if self.validation.browser_input_seen {
                    ValidationStatus::Pass
                } else if self.validation.browser_focus_seen {
                    ValidationStatus::Attention
                } else {
                    ValidationStatus::Waiting
                },
                detail: if self.validation.browser_input_seen {
                    "Mouse, key, or wheel input was forwarded to the viewport".to_string()
                } else if self.validation.browser_focus_seen {
                    "Viewport focused; type, click, or scroll to forward input".to_string()
                } else {
                    "Click the live viewport, then type or scroll".to_string()
                },
            },
            ValidationRow {
                label: "DISTILL",
                status: if !self.capabilities().ai_observe_dom {
                    ValidationStatus::Attention
                } else if self.validation.distill_seen || has_page {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: if !self.capabilities().ai_observe_dom {
                    format!("{} disables AI page observation", self.browser_mode.label())
                } else {
                    distilled_title.unwrap_or_else(|| "No distilled page attached yet".to_string())
                },
            },
            ValidationRow {
                label: "WAKE",
                status: if !self.capabilities().read_wake {
                    ValidationStatus::Attention
                } else if self.validation.wake_seen || !self.wake_results.is_empty() {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: if !self.capabilities().read_wake {
                    format!("{} disables Wake reads", self.browser_mode.label())
                } else {
                    format!("{} visible result(s)", self.wake_results.len())
                },
            },
            ValidationRow {
                label: "CAPTAIN LOG",
                status: if !self.capabilities().write_log_content {
                    ValidationStatus::Attention
                } else if self.validation.log_seen || !self.recent_logs.is_empty() {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: if !self.capabilities().write_log_content {
                    format!(
                        "{} disables content log persistence",
                        self.browser_mode.label()
                    )
                } else {
                    format!("{} recent entry row(s)", self.recent_logs.len())
                },
            },
            ValidationRow {
                label: "TAB CONTROLS",
                status: if self.validation.tab_control_seen {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: "Use new tab, close, reload, back, or forward".to_string(),
            },
            ValidationRow {
                label: "ERROR SURFACE",
                status: if self.validation.error_seen {
                    ValidationStatus::Pass
                } else {
                    ValidationStatus::Waiting
                },
                detail: if self.last_ok {
                    "Trigger a disabled action or failed load to verify status text".to_string()
                } else {
                    truncate(&self.last_status, 72)
                },
            },
        ]
    }

    pub(crate) fn validation_pass_count(&self) -> usize {
        self.validation_rows()
            .iter()
            .filter(|row| matches!(row.status, ValidationStatus::Pass))
            .count()
    }

    pub(crate) fn record_perf(&mut self, phase: &'static str, duration: Duration, label: &str) {
        match phase {
            "navigation" => self.perf.navigation = Some(duration),
            "distill" => self.perf.distill = Some(duration),
            "wake" => self.perf.wake = Some(duration),
            "resize" => self.perf.resize = Some(duration),
            "frame" => self.perf.frame = Some(duration),
            _ => {}
        }
        self.perf_events.push(PerfEvent {
            phase,
            label: label.to_string(),
            duration,
        });
        let overflow = self.perf_events.len().saturating_sub(PERF_HISTORY_LIMIT);
        if overflow > 0 {
            self.perf_events.drain(0..overflow);
        }
    }

    pub(crate) fn slowest_perf_event(&self) -> Option<&PerfEvent> {
        self.perf_events
            .iter()
            .max_by_key(|event| event.duration.as_millis())
    }

    pub(crate) fn slowest_perf_events(&self, limit: usize) -> Vec<&PerfEvent> {
        let mut events = self.perf_events.iter().collect::<Vec<_>>();
        events.sort_by(|left, right| right.duration.cmp(&left.duration));
        events.truncate(limit);
        events
    }

    pub(crate) fn active_tab(&self) -> Option<&Tab> {
        self.engine.get_active_tab()
    }

    pub(crate) fn action_enabled(&self, action: Action) -> bool {
        match action {
            Action::Navigate => !self.address_input.trim().is_empty(),
            Action::RunShowcase => self.proof_workflow_allowed(),
            Action::NewTab => true,
            Action::Back => self
                .active_tab()
                .map(|tab| tab.can_go_back)
                .unwrap_or(false),
            Action::Forward => self
                .active_tab()
                .map(|tab| tab.can_go_forward)
                .unwrap_or(false),
            Action::Reload => self.active_tab().and_then(|tab| tab.url.as_ref()).is_some(),
            Action::CloseTab => self.active_tab().is_some(),
            Action::DistillActive => {
                self.capabilities().ai_observe_dom && self.active_tab().is_some()
            }
            Action::ResetValidation => true,
            Action::SearchWake => {
                self.capabilities().read_wake && !self.wake_query.trim().is_empty()
            }
        }
    }

    pub(crate) fn disabled_reason(&self, action: Action) -> String {
        match action {
            Action::Navigate => "Enter an address, search phrase, or intent first.".to_string(),
            Action::RunShowcase => {
                if !self.proof_workflow_allowed() {
                    format!(
                        "{} mode blocks proof workflows. Switch to Agent or Assisted mode.",
                        self.browser_mode.label()
                    )
                } else {
                    "Showcase is unavailable.".to_string()
                }
            }
            Action::NewTab => "New tab is always available.".to_string(),
            Action::Back => "Back is unavailable for this tab/backend.".to_string(),
            Action::Forward => "Forward is unavailable for this tab/backend.".to_string(),
            Action::Reload => "Active tab has no URL to reload.".to_string(),
            Action::CloseTab => "No active tab to close.".to_string(),
            Action::ResetValidation => "Validation reset is always available.".to_string(),
            Action::DistillActive => {
                if !self.capabilities().ai_observe_dom {
                    format!(
                        "{} mode blocks AI page observation.",
                        self.browser_mode.label()
                    )
                } else {
                    "Open a page before distilling.".to_string()
                }
            }
            Action::SearchWake => {
                if !self.capabilities().read_wake {
                    format!("{} mode blocks Wake reads.", self.browser_mode.label())
                } else {
                    "Enter a Wake query first.".to_string()
                }
            }
        }
    }

    pub(crate) fn sync_address_to_active_tab(&mut self) {
        if let Some(url) = self.active_tab().and_then(|tab| tab.url.clone()) {
            self.address_input = url.to_string();
        } else {
            self.address_input = "about:blank".to_string();
        }
    }

    pub(crate) fn refresh_logs(&mut self) {
        if !self.capabilities().write_log_content {
            self.recent_logs.clear();
            self.pending_log_refresh = None;
            return;
        }
        if self.defer_user_navigation {
            let _ = self.start_visible_log_refresh();
            return;
        }
        self.recent_logs = self
            .log
            .get_entries(&self.persona_id, 5)
            .unwrap_or_default();
    }
}
