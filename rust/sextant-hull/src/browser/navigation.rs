//! `BrowserApp` navigation — visible navigation start/collect, observation
//! warmup scheduling, initial-frame application, and navigation timings.

use super::*;

impl BrowserApp {
    pub fn navigate_to(&mut self, url: Url) {
        let started = Instant::now();
        let guard = match self.check_navigation_guard(&url, "navigate") {
            Ok(guard) => guard,
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "guard blocked");
                self.last_status = error;
                self.last_ok = false;
                self.validation.error_seen = true;
                return;
            }
        };

        match self
            .engine
            .navigate_with_fallback(url.clone(), &self.persona_id)
        {
            Ok(status) => {
                let elapsed = started.elapsed();
                self.record_perf("navigation", elapsed, "open");
                self.perf.frame = None;
                self.address_input = url.to_string();
                self.page_scroll = 0;
                self.validation.navigation_seen = true;
                self.begin_frame_warmup();
                let guard_note = if guard.action == "ALLOW" {
                    String::new()
                } else {
                    format!(" Guard {}.", guard.action)
                };
                self.last_status = format!(
                    "{} {} with {} in {}. Frame capture queued.{}",
                    render_action_label(),
                    short_url(&url),
                    backend(status),
                    format_duration(elapsed),
                    guard_note
                );
                self.last_ok = true;
                let _ = self.record_log(&format!("navigate {}", url), LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "open failed");
                self.last_status = format!("Open failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(&format!("navigate {}", url), LogStatus::Failure(error));
            }
        }
    }

    pub fn start_visible_navigation(&mut self, url: Url) -> bool {
        if self.pending_navigation.is_some() {
            self.last_status = "Navigation is already in progress.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return false;
        }

        let started = Instant::now();
        match self.check_navigation_guard(&url, "navigate") {
            Ok(_) => {}
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), "guard blocked");
                self.last_status = error;
                self.last_ok = false;
                self.validation.error_seen = true;
                return false;
            }
        }

        #[cfg(feature = "servo-backend")]
        {
            let viewport = self.browser_viewport_rect;
            let current_viewport_size = (viewport.w.max(1), viewport.h.max(1));
            self.engine
                .configure_initial_servo_viewport(current_viewport_size.0, current_viewport_size.1);
            let viewport_size = if self.pre_size_visible_navigation {
                Some(current_viewport_size)
            } else {
                None
            };
            match self.engine.navigate_active_tab_async(
                url.clone(),
                self.persona_id.clone(),
                viewport_size,
            ) {
                Ok(result_rx) => {
                    self.perf.frame = None;
                    self.latest_frame = None;
                    self.pending_frame_capture = None;
                    self.last_frame_viewport = None;
                    self.address_input = url.to_string();
                    self.page_scroll = 0;
                    self.pending_navigation = Some(PendingNavigation {
                        url: url.clone(),
                        kind: PendingNavigationKind::Open,
                        result_rx,
                        started,
                        viewport_size,
                    });
                    self.last_status = PendingNavigationKind::Open.pending_status(&url);
                    self.last_ok = true;
                    true
                }
                Err(error) => {
                    self.record_perf("navigation", started.elapsed(), "open start failed");
                    self.last_status = format!("Open failed: {}", error);
                    self.last_ok = false;
                    self.validation.error_seen = true;
                    let _ =
                        self.record_log(&format!("navigate {}", url), LogStatus::Failure(error));
                    false
                }
            }
        }

        #[cfg(not(feature = "servo-backend"))]
        {
            self.navigate_to(url);
            true
        }
    }

    pub fn start_visible_navigation_control(&mut self, kind: PendingNavigationKind) -> bool {
        if self.pending_navigation.is_some() {
            self.last_status = "Navigation is already in progress.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return false;
        }

        let started = Instant::now();
        let Some(url) = self.active_tab().and_then(|tab| tab.url.clone()) else {
            self.last_status = kind.failure_status("active tab has no URL");
            self.last_ok = false;
            self.validation.error_seen = true;
            return false;
        };

        let result = match kind {
            PendingNavigationKind::Open => {
                unreachable!("visible open uses start_visible_navigation")
            }
            PendingNavigationKind::Reload => self.engine.reload_active_tab_async(),
            PendingNavigationKind::Back => self.engine.go_back_active_tab_async(),
            PendingNavigationKind::Forward => self.engine.go_forward_active_tab_async(),
        };

        match result {
            Ok(result_rx) => {
                self.perf.frame = None;
                self.latest_frame = None;
                self.pending_frame_capture = None;
                self.last_frame_viewport = None;
                self.pending_navigation = Some(PendingNavigation {
                    url: url.clone(),
                    kind,
                    result_rx,
                    started,
                    viewport_size: None,
                });
                self.last_status = kind.pending_status(&url);
                self.last_ok = true;
                true
            }
            Err(error) => {
                self.record_perf("navigation", started.elapsed(), kind.failure_perf_label());
                self.last_status = kind.failure_status(&error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(&kind.log_intent(&url), LogStatus::Failure(error));
                false
            }
        }
    }

    pub fn collect_pending_navigation(&mut self) -> Option<Duration> {
        let pending = self.pending_navigation.as_ref()?;
        match pending.result_rx.try_recv() {
            Ok(result) => {
                let pending = self
                    .pending_navigation
                    .take()
                    .expect("pending navigation disappeared");
                let elapsed = pending.started.elapsed();
                match result {
                    Ok(mut result) => {
                        let result_tab_id = result.tab_id;
                        let backend_name = backend(result.status.clone()).to_string();
                        let final_url = result.final_url.clone();
                        let timings = result.timings.clone();
                        let initial_frame = result.initial_frame.take();
                        self.engine.apply_async_navigation_result(result);
                        self.record_perf("navigation", elapsed, pending.kind.perf_label());
                        self.record_navigation_timings(&timings, pending.kind);
                        self.validation.navigation_seen = true;
                        if self.active_tab().map(|tab| tab.id) == Some(result_tab_id) {
                            self.perf.frame = None;
                            self.address_input = final_url.to_string();
                            self.page_scroll = 0;
                            let current_viewport = self.browser_viewport_rect;
                            let current_size =
                                (current_viewport.w.max(1), current_viewport.h.max(1));
                            if pending.viewport_size == Some(current_size) {
                                self.last_frame_viewport = pending.viewport_size;
                            } else {
                                self.last_frame_viewport = None;
                            }
                            self.begin_frame_warmup();
                            if self.apply_initial_navigation_frame(initial_frame) {
                                self.schedule_observation_warmup_if_allowed();
                            } else {
                                self.start_frame_capture_for(FrameCapturePurpose::RenderBridge);
                            }
                            self.last_status =
                                pending
                                    .kind
                                    .success_status(&final_url, &backend_name, elapsed);
                        } else {
                            self.last_status = format!(
                                "Background tab {} loaded {} with {} in {}.",
                                short_id(result_tab_id),
                                short_url(&final_url),
                                backend_name,
                                format_duration(elapsed)
                            );
                        }
                        self.last_ok = true;
                        let _ = self
                            .record_log(&pending.kind.log_intent(&final_url), LogStatus::Success);
                    }
                    Err(error) => {
                        self.record_perf("navigation", elapsed, pending.kind.failure_perf_label());
                        self.last_status = pending.kind.failure_status(&error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                        let _ = self.record_log(
                            &pending.kind.log_intent(&pending.url),
                            LogStatus::Failure(error),
                        );
                    }
                }
                Some(elapsed)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let pending = self
                    .pending_navigation
                    .take()
                    .expect("pending navigation disappeared");
                let elapsed = pending.started.elapsed();
                self.record_perf("navigation", elapsed, pending.kind.dropped_perf_label());
                self.last_status = "Navigation worker dropped the result.".to_string();
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(
                    &pending.kind.log_intent(&pending.url),
                    LogStatus::Failure(self.last_status.clone()),
                );
                Some(elapsed)
            }
        }
    }

    pub fn schedule_observation_warmup_if_allowed(&mut self) -> bool {
        if !self.observation_warmup_enabled
            || !self.capabilities().ai_observe_dom
            || self.scheduled_observation_warmup.is_some()
            || self.pending_observation_warmup.is_some()
        {
            return false;
        }
        let Some(tab_id) = self.active_tab().map(|tab| tab.id) else {
            return false;
        };
        self.scheduled_observation_warmup = Some(ScheduledObservationWarmup {
            tab_id,
            due: Instant::now() + OBSERVATION_WARMUP_IDLE_DELAY,
        });
        true
    }

    pub fn maybe_start_scheduled_observation_warmup(&mut self) -> bool {
        let Some(scheduled) = self.scheduled_observation_warmup.as_ref() else {
            return false;
        };
        if Instant::now() < scheduled.due {
            return false;
        }
        if self.observation_warmup_has_foreground_work() {
            return false;
        }
        let tab_id = scheduled.tab_id;
        if self.active_tab().map(|tab| tab.id) != Some(tab_id) {
            self.scheduled_observation_warmup = None;
            return false;
        }
        self.scheduled_observation_warmup = None;
        self.start_observation_warmup_for(tab_id)
    }

    pub fn observation_warmup_has_foreground_work(&self) -> bool {
        self.pending_user_navigation.is_some()
            || self.pending_user_action.is_some()
            || self.pending_navigation.is_some()
            || self.pending_distillation.is_some()
            || self.pending_persistence.is_some()
            || self.pending_wake_search.is_some()
            || self.pending_frame_capture.is_some()
            || !self.pending_log_writes.is_empty()
            || self.pending_log_refresh.is_some()
            || !self.pending_viewport_input.is_empty()
            || !self.pending_browser_text.is_empty()
    }

    pub fn scheduled_observation_warmup_due(&self) -> Option<Instant> {
        self.scheduled_observation_warmup
            .as_ref()
            .map(|scheduled| scheduled.due)
    }

    pub fn start_observation_warmup_for(&mut self, tab_id: Uuid) -> bool {
        if !self.observation_warmup_enabled
            || !self.capabilities().ai_observe_dom
            || self.pending_observation_warmup.is_some()
        {
            return false;
        }
        match self.engine.eval_probe_tab_async(tab_id) {
            Ok(result_rx) => {
                self.pending_observation_warmup = Some(PendingObservationWarmup {
                    tab_id,
                    result_rx,
                    started: Instant::now(),
                });
                true
            }
            Err(error) => {
                self.record_perf(
                    "eval-warmup",
                    Duration::ZERO,
                    "ai observation warmup start failed",
                );
                self.validation.error_seen = true;
                let _ = self.record_log(
                    "ai observation warmup",
                    LogStatus::Failure(error.to_string()),
                );
                false
            }
        }
    }

    pub fn collect_pending_observation_warmup(&mut self) -> Option<Duration> {
        let pending = self.pending_observation_warmup.as_ref()?;
        match pending.result_rx.try_recv() {
            Ok(result) => {
                let pending = self
                    .pending_observation_warmup
                    .take()
                    .expect("pending observation warmup disappeared");
                let elapsed = pending.started.elapsed();
                match result {
                    Ok(probe) => {
                        self.record_perf(
                            "eval-warmup",
                            Duration::from_millis(probe.eval_ms),
                            &format!(
                                "ai observation warmup tab {} script {}ms queue {}ms elapsed {}",
                                pending.tab_id,
                                probe.script_ms,
                                probe.queue_ms,
                                format_duration(elapsed)
                            ),
                        );
                    }
                    Err(error) => {
                        self.record_perf("eval-warmup", elapsed, "ai observation warmup failed");
                        self.validation.error_seen = true;
                        let _ = self.record_log(
                            "ai observation warmup",
                            LogStatus::Failure(error.to_string()),
                        );
                    }
                }
                Some(elapsed)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let pending = self
                    .pending_observation_warmup
                    .take()
                    .expect("pending observation warmup disappeared");
                let elapsed = pending.started.elapsed();
                self.record_perf("eval-warmup", elapsed, "ai observation warmup dropped");
                self.validation.error_seen = true;
                let _ = self.record_log(
                    "ai observation warmup",
                    LogStatus::Failure("worker dropped".to_string()),
                );
                Some(elapsed)
            }
        }
    }

    pub fn apply_initial_navigation_frame(
        &mut self,
        initial_frame: Option<Result<AsyncFrameCapture, String>>,
    ) -> bool {
        let Some(result) = initial_frame else {
            return false;
        };
        match result {
            Ok(capture) => {
                self.record_perf(
                    "frame-queue",
                    capture.queue,
                    FrameCapturePurpose::RenderBridge.queue_label(),
                );
                if let Some(resize) = capture.resize {
                    self.record_perf(
                        "resize",
                        resize,
                        FrameCapturePurpose::RenderBridge.resize_async_label(),
                    );
                } else {
                    let current_viewport = self.browser_viewport_rect;
                    let current_size = (current_viewport.w.max(1), current_viewport.h.max(1));
                    if (capture.frame.width, capture.frame.height) == current_size {
                        self.record_perf(
                            "resize",
                            Duration::ZERO,
                            FrameCapturePurpose::RenderBridge.resize_label(),
                        );
                        self.last_frame_viewport = Some(current_size);
                    }
                }
                self.record_perf(
                    "frame",
                    capture.capture,
                    FrameCapturePurpose::RenderBridge.capture_label(),
                );
                let frame = capture.frame;
                if frame.width > 0 && frame.height > 0 && !frame.pixels.is_empty() {
                    self.validation.frame_seen = true;
                }
                self.latest_frame = Some(frame);
                self.last_frame_refresh = Instant::now();
                self.frame_refresh_budget = self.frame_refresh_budget.saturating_sub(1);
                self.frame_dirty = self.frame_refresh_budget > 0;
                true
            }
            Err(error) => {
                self.record_perf(
                    "frame",
                    Duration::ZERO,
                    FrameCapturePurpose::RenderBridge.capture_failed_label(),
                );
                self.last_status = format!("Initial Servo frame unavailable: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                false
            }
        }
    }

    pub fn record_navigation_timings(
        &mut self,
        timings: &AsyncNavigationTimings,
        kind: PendingNavigationKind,
    ) {
        let label = kind.phase_label();
        if let Some(duration) = timings.firewall {
            self.record_perf("nav-phase", duration, &format!("{label} firewall"));
        }
        if let Some(duration) = timings.servo_navigation {
            self.record_perf("nav-phase", duration, &format!("{label} servo navigation"));
        }
        if let Some(duration) = timings.servo_url_wait {
            self.record_perf("nav-phase", duration, &format!("{label} servo url wait"));
        }
        if let Some(duration) = timings.servo_load_wait {
            self.record_perf("nav-phase", duration, &format!("{label} servo load wait"));
        }
        if let Some(duration) = timings.servo_inspect {
            self.record_perf("nav-phase", duration, &format!("{label} servo inspect"));
        }
        if let Some(duration) = timings.reader_fallback {
            self.record_perf("nav-phase", duration, &format!("{label} reader fallback"));
        }
    }
}
