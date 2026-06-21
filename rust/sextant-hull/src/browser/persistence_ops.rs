//! `BrowserApp` distill/persist/wake — async distillation, persistence,
//! Wake search, and Captain's Log write/refresh collection.

use super::*;

impl BrowserApp {
    pub fn distill_active(&mut self) {
        let capabilities = self.capabilities();
        if !capabilities.ai_observe_dom {
            self.last_status = format!(
                "{} blocks DOM observation/distillation.",
                self.browser_mode.label()
            );
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        }

        let started = Instant::now();
        match self.engine.distill_current_page() {
            Ok(page) => {
                let distill_elapsed = started.elapsed();
                self.record_perf("distill", distill_elapsed, "active tab");
                self.finish_distilled_page(page, distill_elapsed);
            }
            Err(error) => {
                self.record_perf("distill", started.elapsed(), "active tab failed");
                self.last_status = format!("Distill failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("distill active", LogStatus::Failure(error));
            }
        }
    }

    pub fn start_visible_distillation(&mut self) -> bool {
        if self.pending_distillation.is_some() {
            self.last_status = "Distillation is already in progress.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return false;
        }
        if !self.capabilities().ai_observe_dom {
            self.last_status = format!(
                "{} blocks DOM observation/distillation.",
                self.browser_mode.label()
            );
            self.last_ok = false;
            self.validation.error_seen = true;
            return false;
        }

        let started = Instant::now();
        match self.engine.distill_active_tab_async() {
            Ok(result_rx) => {
                self.pending_distillation = Some(PendingDistillation { result_rx, started });
                self.last_status = "Distilling active page...".to_string();
                self.last_ok = true;
                true
            }
            Err(error) => {
                self.record_perf("distill", started.elapsed(), "active distill start failed");
                self.last_status = format!("Distill failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("distill active", LogStatus::Failure(error));
                false
            }
        }
    }

    pub fn collect_pending_distillation(&mut self) -> Option<Duration> {
        let pending = self.pending_distillation.as_ref()?;
        match pending.result_rx.try_recv() {
            Ok(result) => {
                let pending = self
                    .pending_distillation
                    .take()
                    .expect("pending distillation disappeared");
                let elapsed = pending.started.elapsed();
                match result {
                    Ok(result) => {
                        let page = result.page.clone();
                        self.engine.apply_async_distill_result(result);
                        self.record_perf("distill", elapsed, "active tab async");
                        if self.capabilities().ai_observe_dom {
                            self.start_visible_distill_persistence(page, elapsed);
                        } else {
                            self.last_status = format!(
                                "{} blocks DOM observation/distillation.",
                                self.browser_mode.label()
                            );
                            self.last_ok = false;
                            self.validation.error_seen = true;
                        }
                    }
                    Err(error) => {
                        self.record_perf("distill", elapsed, "active tab async failed");
                        self.last_status = format!("Distill failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                        let _ = self.record_log("distill active", LogStatus::Failure(error));
                    }
                }
                Some(elapsed)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let pending = self
                    .pending_distillation
                    .take()
                    .expect("pending distillation disappeared");
                let elapsed = pending.started.elapsed();
                self.record_perf("distill", elapsed, "active distill worker dropped");
                self.last_status = "Distillation worker dropped the result.".to_string();
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(
                    "distill active",
                    LogStatus::Failure(self.last_status.clone()),
                );
                Some(elapsed)
            }
        }
    }

    pub fn start_visible_distill_persistence(
        &mut self,
        page: DistilledPage,
        distill_elapsed: Duration,
    ) {
        let capabilities = self.capabilities();
        if !capabilities.write_wake {
            self.page_scroll = 0;
            self.begin_frame_warmup();
            self.wake_query = page.title.clone();
            self.last_status = format!(
                "Observed '{}' in {}. Wake writes are disabled in {} mode.",
                page.title,
                format_duration(distill_elapsed),
                self.browser_mode.label()
            );
            self.last_ok = true;
            self.validation.distill_seen = true;
            return;
        }

        let page_title = page.title.clone();
        self.page_scroll = 0;
        self.begin_frame_warmup();
        self.wake_query = page_title.clone();
        self.validation.distill_seen = true;
        let started = Instant::now();
        match self
            .persistence_lane
            .record_distilled_page(self.persona_id.clone(), page)
        {
            Ok(result_rx) => {
                self.pending_persistence = Some(PendingPersistence {
                    result_rx,
                    started,
                    page_title: page_title.clone(),
                });
                self.last_status = format!(
                    "Distilled '{}' in {}. Recording in Persistence lane...",
                    page_title,
                    format_duration(distill_elapsed)
                );
                self.last_ok = true;
            }
            Err(error) => {
                self.record_perf("wake", started.elapsed(), "persistence lane start failed");
                self.last_status = format!("Persistence lane failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    pub fn collect_pending_persistence(&mut self) -> Option<Duration> {
        let pending = self.pending_persistence.as_ref()?;
        match pending.result_rx.try_recv() {
            Ok(result) => {
                let pending = self
                    .pending_persistence
                    .take()
                    .expect("pending persistence disappeared");
                let elapsed = pending.started.elapsed();
                match result {
                    Ok(result) => {
                        self.record_perf("wake", elapsed, "persistence lane distill");
                        self.wake_results = result.wake_results;
                        self.recent_logs = result.recent_logs;
                        self.validation.wake_seen = !self.wake_results.is_empty();
                        self.validation.log_seen = !self.recent_logs.is_empty();
                        self.last_status = format!(
                            "Distilled '{}' into Wake via Persistence lane. {}.",
                            pending.page_title,
                            self.perf.summary()
                        );
                        self.last_ok = true;
                    }
                    Err(error) => {
                        self.record_perf("wake", elapsed, "persistence lane failed");
                        self.last_status = format!("Persistence lane failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                    }
                }
                Some(elapsed)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let pending = self
                    .pending_persistence
                    .take()
                    .expect("pending persistence disappeared");
                let elapsed = pending.started.elapsed();
                self.record_perf("wake", elapsed, "persistence lane dropped");
                self.last_status = "Persistence lane dropped the result.".to_string();
                self.last_ok = false;
                self.validation.error_seen = true;
                Some(elapsed)
            }
        }
    }

    pub fn finish_distilled_page(&mut self, page: DistilledPage, distill_elapsed: Duration) {
        let capabilities = self.capabilities();
        if !capabilities.write_wake {
            self.page_scroll = 0;
            self.begin_frame_warmup();
            self.wake_query = page.title.clone();
            self.last_status = format!(
                "Observed '{}' in {}. Wake writes are disabled in {} mode.",
                page.title,
                format_duration(distill_elapsed),
                self.browser_mode.label()
            );
            self.last_ok = true;
            self.validation.distill_seen = true;
            return;
        }
        let wake_started = Instant::now();
        match self.wake.record(&self.persona_id, &page, None) {
            Ok(()) => {
                self.record_perf("wake", wake_started.elapsed(), "record page");
                self.page_scroll = 0;
                self.begin_frame_warmup();
                self.wake_query = page.title.clone();
                let title = page.title.clone();
                self.last_status = format!(
                    "Distilled '{}' into Wake in {}.",
                    title,
                    format_duration(distill_elapsed)
                );
                self.last_ok = true;
                self.validation.distill_seen = true;
                let _ = self.record_log("distill active", LogStatus::Success);
                self.search_wake();
                if self.last_ok {
                    self.last_status =
                        format!("Distilled '{}' into Wake. {}.", title, self.perf.summary());
                }
            }
            Err(error) => {
                self.record_perf("wake", wake_started.elapsed(), "record failed");
                self.last_status = format!("Wake record failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("distill active", LogStatus::Failure(error.to_string()));
            }
        }
    }

    pub fn search_wake(&mut self) {
        if !self.capabilities().read_wake {
            self.last_status = format!("{} blocks Wake reads.", self.browser_mode.label());
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        }
        let query = self.wake_query.trim().to_string();
        let started = Instant::now();
        match self.wake.search(&self.persona_id, &query) {
            Ok(results) => {
                let elapsed = started.elapsed();
                self.record_perf("wake", elapsed, "search");
                self.last_status = format!(
                    "Wake search found {} result(s) in {}.",
                    results.len(),
                    format_duration(elapsed)
                );
                self.last_ok = true;
                self.validation.wake_seen = !results.is_empty();
                self.wake_results = results;
                let _ = self.record_log(&format!("search Wake '{}'", query), LogStatus::Success);
            }
            Err(error) => {
                self.record_perf("wake", started.elapsed(), "search failed");
                self.last_status = format!("Wake search failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log("search Wake", LogStatus::Failure(error.to_string()));
            }
        }
    }

    pub fn start_visible_wake_search(&mut self) -> bool {
        if self.pending_wake_search.is_some() {
            self.last_status = "Wake search is already in progress.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return false;
        }
        if !self.capabilities().read_wake {
            self.last_status = format!("{} blocks Wake reads.", self.browser_mode.label());
            self.last_ok = false;
            self.validation.error_seen = true;
            return false;
        }
        let query = self.wake_query.trim().to_string();
        if query.is_empty() {
            self.last_status = "Enter a Wake query first.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return false;
        }

        let started = Instant::now();
        match self
            .persistence_lane
            .search_wake(self.persona_id.clone(), query.clone())
        {
            Ok(result_rx) => {
                self.pending_wake_search = Some(PendingWakeSearch {
                    result_rx,
                    started,
                    query: query.clone(),
                });
                self.last_status = format!("Searching Wake for '{}' in Persistence lane...", query);
                self.last_ok = true;
                true
            }
            Err(error) => {
                self.record_perf("wake", started.elapsed(), "persistence search start failed");
                self.last_status = format!("Wake search failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                false
            }
        }
    }

    pub fn collect_pending_wake_search(&mut self) -> Option<Duration> {
        let pending = self.pending_wake_search.as_ref()?;
        match pending.result_rx.try_recv() {
            Ok(result) => {
                let pending = self
                    .pending_wake_search
                    .take()
                    .expect("pending Wake search disappeared");
                let elapsed = pending.started.elapsed();
                match result {
                    Ok(result) => {
                        self.record_perf("wake", elapsed, "persistence lane search");
                        self.wake_results = result.wake_results;
                        self.recent_logs = result.recent_logs;
                        self.validation.wake_seen = !self.wake_results.is_empty();
                        self.validation.log_seen = !self.recent_logs.is_empty();
                        self.last_status = format!(
                            "Wake search found {} result(s) via Persistence lane in {}.",
                            self.wake_results.len(),
                            format_duration(elapsed)
                        );
                        self.last_ok = true;
                    }
                    Err(error) => {
                        self.record_perf("wake", elapsed, "persistence search failed");
                        self.last_status = format!("Wake search failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                    }
                }
                Some(elapsed)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let pending = self
                    .pending_wake_search
                    .take()
                    .expect("pending Wake search disappeared");
                let elapsed = pending.started.elapsed();
                self.record_perf("wake", elapsed, "persistence search dropped");
                self.last_status =
                    format!("Persistence lane dropped Wake search '{}'.", pending.query);
                self.last_ok = false;
                self.validation.error_seen = true;
                Some(elapsed)
            }
        }
    }

    pub fn collect_pending_log_writes(&mut self) -> bool {
        let mut changed = false;
        let mut index = 0;
        while index < self.pending_log_writes.len() {
            match self.pending_log_writes[index].result_rx.try_recv() {
                Ok(result) => {
                    let pending = self.pending_log_writes.remove(index);
                    let elapsed = pending.started.elapsed();
                    match result {
                        Ok(result) => {
                            self.record_perf("wake", elapsed, "persistence lane log");
                            self.recent_logs = result.recent_logs;
                            self.validation.log_seen = !self.recent_logs.is_empty();
                            changed = true;
                        }
                        Err(error) => {
                            self.record_perf("wake", elapsed, "persistence log failed");
                            self.last_status = format!(
                                "Captain's Log write '{}' failed: {}",
                                pending.intent, error
                            );
                            self.last_ok = false;
                            self.validation.error_seen = true;
                            changed = true;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let pending = self.pending_log_writes.remove(index);
                    let elapsed = pending.started.elapsed();
                    self.record_perf("wake", elapsed, "persistence log dropped");
                    self.last_status =
                        format!("Persistence lane dropped log write '{}'.", pending.intent);
                    self.last_ok = false;
                    self.validation.error_seen = true;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn start_visible_log_refresh(&mut self) -> bool {
        if !self.capabilities().write_log_content {
            self.recent_logs.clear();
            self.pending_log_refresh = None;
            return false;
        }
        if !self.pending_log_writes.is_empty() || self.pending_log_refresh.is_some() {
            return false;
        }

        let started = Instant::now();
        match self
            .persistence_lane
            .refresh_logs(self.persona_id.clone(), 5)
        {
            Ok(result_rx) => {
                self.pending_log_refresh = Some(PendingLogRefresh { result_rx, started });
                true
            }
            Err(error) => {
                self.record_perf(
                    "wake",
                    started.elapsed(),
                    "persistence log refresh start failed",
                );
                self.last_status = format!("Captain's Log refresh failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                false
            }
        }
    }

    pub fn collect_pending_log_refresh(&mut self) -> bool {
        let Some(pending) = self.pending_log_refresh.as_ref() else {
            return false;
        };
        match pending.result_rx.try_recv() {
            Ok(result) => {
                let pending = self
                    .pending_log_refresh
                    .take()
                    .expect("pending log refresh disappeared");
                let elapsed = pending.started.elapsed();
                match result {
                    Ok(result) => {
                        self.record_perf("wake", elapsed, "persistence lane log refresh");
                        self.recent_logs = result.recent_logs;
                        self.validation.log_seen = !self.recent_logs.is_empty();
                    }
                    Err(error) => {
                        self.record_perf("wake", elapsed, "persistence log refresh failed");
                        self.last_status = format!("Captain's Log refresh failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                    }
                }
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                let pending = self
                    .pending_log_refresh
                    .take()
                    .expect("pending log refresh disappeared");
                self.record_perf(
                    "wake",
                    pending.started.elapsed(),
                    "persistence log refresh dropped",
                );
                self.last_status = "Persistence lane dropped Captain's Log refresh.".to_string();
                self.last_ok = false;
                self.validation.error_seen = true;
                true
            }
        }
    }
}
