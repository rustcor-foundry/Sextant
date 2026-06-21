//! `BrowserApp` frame capture/refresh — the adaptive redraw cadence and
//! softbuffer/render-bridge frame capture collection (UI-thread hot path).

use super::*;

impl BrowserApp {
    pub(crate) fn refresh_frame(&mut self) {
        self.refresh_frame_for(FrameCapturePurpose::AiObservation);
    }

    pub(crate) fn refresh_render_bridge_frame(&mut self) {
        self.refresh_frame_for(FrameCapturePurpose::RenderBridge);
    }

    pub(crate) fn refresh_render_bridge_frame_after_visible_tab_change(&mut self) {
        if self.defer_user_navigation {
            self.drop_pending_frame_capture_for_tab_change();
            self.start_frame_capture_for(FrameCapturePurpose::RenderBridge);
        } else {
            self.refresh_render_bridge_frame();
        }
    }

    pub(crate) fn drop_pending_frame_capture_for_tab_change(&mut self) -> bool {
        self.pending_frame_capture.take().is_some()
    }

    pub(crate) fn frame_capture_allowed(&self, purpose: FrameCapturePurpose) -> bool {
        match purpose {
            FrameCapturePurpose::RenderBridge => true,
            FrameCapturePurpose::AiObservation => self.capabilities().ai_observe_frame,
        }
    }

    pub(crate) fn block_frame_capture(&mut self, purpose: FrameCapturePurpose) {
        self.last_status = match purpose {
            FrameCapturePurpose::RenderBridge => {
                "Servo render bridge frame capture is unavailable.".to_string()
            }
            FrameCapturePurpose::AiObservation => format!(
                "AI frame observation is disabled in {} mode.",
                self.browser_mode.label()
            ),
        };
        self.last_ok = false;
        self.validation.error_seen = true;
    }

    pub(crate) fn refresh_frame_for(&mut self, purpose: FrameCapturePurpose) {
        if !self.frame_capture_allowed(purpose) {
            self.block_frame_capture(purpose);
            return;
        }

        let viewport = self.browser_viewport_rect;
        let viewport_size = (viewport.w.max(1), viewport.h.max(1));
        if self.last_frame_viewport != Some(viewport_size) {
            let resize_started = Instant::now();
            match self
                .engine
                .resize_current_viewport(viewport_size.0, viewport_size.1)
            {
                Ok(()) => {
                    self.record_perf("resize", resize_started.elapsed(), purpose.resize_label());
                    self.last_frame_viewport = Some(viewport_size);
                }
                Err(error) => {
                    self.record_perf(
                        "resize",
                        resize_started.elapsed(),
                        purpose.resize_failed_label(),
                    );
                    self.last_frame_refresh = Instant::now();
                    self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                    self.frame_dirty = self.frame_refresh_budget > 0;
                    self.last_status = format!("Servo viewport resize failed: {}", error);
                    self.last_ok = false;
                    self.validation.error_seen = true;
                    return;
                }
            }
        } else {
            self.record_perf("resize", Duration::ZERO, purpose.resize_label());
        }

        let started = Instant::now();
        match self.engine.capture_current_frame() {
            Ok(frame) => {
                self.record_perf("frame", started.elapsed(), purpose.capture_label());
                if frame.width > 0 && frame.height > 0 && !frame.pixels.is_empty() {
                    self.validation.frame_seen = true;
                }
                self.latest_frame = Some(frame);
                self.last_frame_refresh = Instant::now();
                self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                self.frame_dirty = self.frame_refresh_budget > 0;
            }
            Err(error) => {
                self.record_perf("frame", started.elapsed(), purpose.capture_failed_label());
                self.last_frame_refresh = Instant::now();
                self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                self.frame_dirty = self.frame_refresh_budget > 0;
                self.last_status = format!("Servo frame unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    pub(crate) fn maybe_refresh_frame(&mut self) -> bool {
        if self.collect_pending_frame_capture() {
            return true;
        }

        if self.main_view != MainView::Browser
            || self.pending_navigation.is_some()
            || self.pending_distillation.is_some()
            || self.pending_observation_warmup.is_some()
            || (self.latest_frame.is_none() && self.frame_refresh_budget == 0)
            || self.pending_frame_capture.is_some()
        {
            return false;
        }

        if self
            .viewport_input_capture_delay()
            .is_some_and(|delay| !delay.is_zero())
        {
            return false;
        }

        let elapsed = self.last_frame_refresh.elapsed();
        let due = if self.frame_refresh_budget > 0 {
            self.last_viewport_input_flush.is_some() || elapsed >= FRAME_REFRESH_INTERACTION
        } else if self.frame_dirty {
            elapsed >= FRAME_REFRESH_DIRTY
        } else {
            self.focus == FocusTarget::Browser && elapsed >= FRAME_REFRESH_IDLE
        };

        if due {
            if self.start_frame_capture_for(FrameCapturePurpose::RenderBridge) {
                self.last_viewport_input_flush = None;
            }
        }
        false
    }

    pub(crate) fn viewport_input_capture_delay(&self) -> Option<Duration> {
        let elapsed = self.last_viewport_input_flush?.elapsed();
        Some(VIEWPORT_INPUT_CAPTURE_SETTLE.saturating_sub(elapsed))
    }

    pub(crate) fn start_frame_capture_for(&mut self, purpose: FrameCapturePurpose) -> bool {
        if self.pending_frame_capture.is_some() {
            return false;
        }

        if !self.frame_capture_allowed(purpose) {
            self.block_frame_capture(purpose);
            return false;
        }

        let Some(tab_id) = self.active_tab().map(|tab| tab.id) else {
            return false;
        };
        let viewport = self.browser_viewport_rect;
        let viewport_size = (viewport.w.max(1), viewport.h.max(1));
        let requested_resize = self.last_frame_viewport != Some(viewport_size);
        if !requested_resize {
            self.record_perf("resize", Duration::ZERO, purpose.resize_label());
        }

        let started = Instant::now();
        let resize = requested_resize.then_some(viewport_size);
        match self
            .engine
            .capture_tab_frame_with_resize_async(tab_id, resize)
        {
            Ok(result_rx) => {
                self.pending_frame_capture = Some(PendingFrameCapture {
                    tab_id,
                    result_rx,
                    started,
                    viewport_size,
                    requested_resize,
                    purpose,
                });
                self.last_frame_refresh = Instant::now();
                true
            }
            Err(error) => {
                self.record_perf(
                    "frame",
                    started.elapsed(),
                    purpose.capture_start_failed_label(),
                );
                self.last_frame_refresh = Instant::now();
                self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                self.frame_dirty = self.frame_refresh_budget > 0;
                self.last_status = format!("Servo frame unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                false
            }
        }
    }

    pub(crate) fn collect_pending_frame_capture(&mut self) -> bool {
        let Some(pending) = self.pending_frame_capture.as_ref() else {
            return false;
        };
        match pending.result_rx.try_recv() {
            Ok(result) => {
                let pending = self
                    .pending_frame_capture
                    .take()
                    .expect("pending frame capture disappeared");
                if self.active_tab().map(|tab| tab.id) != Some(pending.tab_id) {
                    return false;
                }
                match result {
                    Ok(capture) => {
                        self.record_perf(
                            "frame-queue",
                            capture.queue,
                            pending.purpose.queue_label(),
                        );
                        if let Some(resize) = capture.resize {
                            self.record_perf(
                                "resize",
                                resize,
                                pending.purpose.resize_async_label(),
                            );
                            let current_viewport = self.browser_viewport_rect;
                            let current_size =
                                (current_viewport.w.max(1), current_viewport.h.max(1));
                            if current_size == pending.viewport_size {
                                self.last_frame_viewport = Some(pending.viewport_size);
                            }
                        } else if pending.requested_resize {
                            self.record_perf(
                                "resize",
                                Duration::ZERO,
                                pending.purpose.resize_async_missing_label(),
                            );
                        }
                        self.record_perf(
                            "frame-total",
                            pending.started.elapsed(),
                            pending.purpose.capture_label(),
                        );
                        self.record_perf("frame", capture.capture, pending.purpose.capture_label());
                        let frame = capture.frame;
                        if frame.width > 0 && frame.height > 0 && !frame.pixels.is_empty() {
                            self.validation.frame_seen = true;
                        }
                        self.latest_frame = Some(frame);
                        self.last_frame_refresh = Instant::now();
                        self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                        self.frame_dirty = self.frame_refresh_budget > 0;
                    }
                    Err(error) => {
                        self.record_perf(
                            "frame",
                            pending.started.elapsed(),
                            pending.purpose.capture_failed_label(),
                        );
                        self.last_frame_refresh = Instant::now();
                        self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                        self.frame_dirty = self.frame_refresh_budget > 0;
                        self.last_status = format!("Servo frame unavailable: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                    }
                }
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                let pending = self
                    .pending_frame_capture
                    .take()
                    .expect("pending frame capture disappeared");
                self.record_perf(
                    "frame",
                    pending.started.elapsed(),
                    pending.purpose.capture_dropped_label(),
                );
                self.last_frame_refresh = Instant::now();
                self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                self.frame_dirty = self.frame_refresh_budget > 0;
                self.last_status = "Servo frame capture worker dropped the result.".to_string();
                self.last_ok = false;
                self.validation.error_seen = true;
                true
            }
        }
    }

    pub(crate) fn begin_frame_warmup(&mut self) {
        self.frame_refresh_budget = FRAME_WARMUP_BUDGET;
        self.frame_dirty = true;
    }

    pub(crate) fn begin_interaction_frame_warmup(&mut self) {
        self.frame_refresh_budget = self
            .frame_refresh_budget
            .max(FRAME_INTERACTION_WARMUP_BUDGET);
        self.last_frame_refresh = Instant::now();
        self.frame_dirty = true;
    }

    pub(crate) fn record_log(&mut self, intent: &str, status: LogStatus) -> Result<(), String> {
        self.record_log_with_consent(intent, status, None)
    }

    pub(crate) fn record_log_with_consent(
        &mut self,
        intent: &str,
        status: LogStatus,
        consent_signature: Option<String>,
    ) -> Result<(), String> {
        if !self.capabilities().write_log_content {
            return Ok(());
        }
        let entry = LogEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            persona_id: self.persona_id.clone(),
            intent: intent.to_string(),
            plan_json: "{}".to_string(),
            signature: "native-browser-shell".to_string(),
            consent_signature,
            status,
        };
        if self.defer_user_navigation {
            let started = Instant::now();
            match self.persistence_lane.record_log(entry) {
                Ok(result_rx) => {
                    self.pending_log_writes.push(PendingLogWrite {
                        result_rx,
                        started,
                        intent: intent.to_string(),
                    });
                    Ok(())
                }
                Err(error) => Err(error),
            }
        } else {
            match self.log.record(&entry) {
                Ok(()) => {
                    self.validation.log_seen = true;
                    Ok(())
                }
                Err(error) => Err(error.to_string()),
            }
        }
    }
}
