//! `BrowserApp` intent + pilot execution — showcase/real/shell runs, native
//! intent dispatch, heuristic + pilot planning, action execution, and consent.

use super::*;

impl BrowserApp {
    pub fn run_showcase_visible(&mut self) {
        if !self.proof_workflow_allowed() {
            self.block_mode_control_work("launch showcase");
            return;
        }
        self.last_status = "Running launch showcase workflow.".to_string();
        self.last_ok = true;
        match run_showcase_workflow(self) {
            Ok(report) => {
                self.last_status =
                    "Showcase complete: Intent, Wake, Log, interaction, and Servo frame are ready."
                        .to_string();
                self.pilot_status = "SHOWCASE READY".to_string();
                self.pilot_result = report
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "Sextant launch showcase run passed".to_string());
                self.pilot_plan = report
                    .iter()
                    .cloned()
                    .filter(|line| !line.starts_with("isolated data dir"))
                    .take(4)
                    .collect();
                self.showcase_report = report;
                self.last_ok = true;
                self.main_view = MainView::Validation;
                self.refresh_logs();
            }
            Err(error) => {
                self.last_status = format!("Showcase failed: {}", error);
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = error;
                self.showcase_report = vec![self.last_status.clone()];
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    pub fn run_real_browsing_visible(&mut self) {
        if !self.proof_workflow_allowed() {
            self.block_mode_control_work("real browsing smoke");
            return;
        }
        self.last_status = "Running real browsing workflow.".to_string();
        self.last_ok = true;
        match run_real_browsing_workflow(self) {
            Ok(report) => {
                self.last_status =
                    "Real browsing complete: Google, docs, history, Wake, Log, and Servo frame are ready."
                        .to_string();
                self.pilot_status = "BROWSING READY".to_string();
                self.pilot_result = report
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "real browsing smoke suite passed".to_string());
                self.pilot_plan = report
                    .iter()
                    .cloned()
                    .filter(|line| !line.starts_with("isolated data dir"))
                    .take(4)
                    .collect();
                self.showcase_report = report;
                self.last_ok = true;
                self.main_view = MainView::Validation;
                self.refresh_logs();
            }
            Err(error) => {
                self.last_status = format!("Real browsing failed: {}", error);
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = error;
                self.showcase_report = vec![self.last_status.clone()];
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    pub fn run_shell_interaction_visible(&mut self) {
        if !self.proof_workflow_allowed() {
            self.block_mode_control_work("shell interaction smoke");
            return;
        }
        self.last_status = "Running shell interaction smoke.".to_string();
        self.last_ok = true;
        match run_shell_interaction_workflow(self) {
            Ok(report) => {
                self.last_status =
                    "Shell interaction complete: chrome clicks, tabs, Sense, Perf, viewport focus, Wake, Log, and frame are ready."
                        .to_string();
                self.pilot_status = "SHELL READY".to_string();
                self.pilot_result = report
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "visible shell interaction smoke passed".to_string());
                self.pilot_plan = report.iter().cloned().take(4).collect();
                self.showcase_report = report;
                self.last_ok = true;
                self.main_view = MainView::Validation;
                self.refresh_logs();
            }
            Err(error) => {
                self.last_status = format!("Shell interaction failed: {}", error);
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = error;
                self.showcase_report = vec![self.last_status.clone()];
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    pub fn navigate_input(&mut self) {
        let raw = self.address_input.trim().to_string();
        if raw.is_empty() {
            self.last_status = "Enter a URL, search phrase, or intent first.".to_string();
            self.last_ok = false;
            return;
        }

        if self.defer_user_navigation {
            self.pending_user_navigation = Some(raw.clone());
            self.last_status = format!("Opening {}...", truncate(&raw, 80));
            self.last_ok = true;
            self.main_view = MainView::Browser;
            return;
        }

        if native_intent_body(&raw).is_some() {
            self.run_native_intent(&raw);
            return;
        }

        match parse_navigation_target(&raw) {
            Ok(url) => self.navigate_to(url),
            Err(error) => {
                self.last_status = error;
                self.last_ok = false;
            }
        }
    }

    /// Live agent-intent entry (interactive address/intent bar): plans with the
    /// local model brain.
    pub fn run_native_intent(&mut self, raw: &str) {
        self.dispatch_native_intent(raw, true);
    }

    /// Deterministic, synchronous intent entry for smokes, the MCP `--intent-run`
    /// operator mode, and tests: plans with the built-in heuristic so behavior is
    /// reproducible and does not depend on a running model server.
    pub fn run_native_intent_sync(&mut self, raw: &str) {
        self.dispatch_native_intent(raw, false);
    }

    /// Run a quick-intent preset (AI-rail launcher): mirror the intent into the
    /// address field, show the browser view, and plan it through the local model
    /// brain (same path as the interactive intent bar).
    #[cfg(feature = "xilem-shell")]
    pub fn run_preset_intent(&mut self, intent: &str) {
        self.address_input = intent.to_string();
        self.main_view = MainView::Browser;
        self.run_native_intent(intent);
    }

    pub fn dispatch_native_intent(&mut self, raw: &str, use_brain: bool) {
        let Some(intent) = native_intent_body(raw) else {
            self.last_status = "That input did not resolve to a native intent.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        };

        if !self.native_intent_allowed() {
            self.last_intent = intent.clone();
            self.pilot_status = "BLOCKED".to_string();
            self.pilot_result = format!(
                "{} mode blocks native AI intent execution.",
                self.browser_mode.label()
            );
            self.pilot_plan = vec![
                "Read intent".to_string(),
                "Stop at active browser mode boundary".to_string(),
            ];
            self.pending_consent = None;
            self.last_status = format!(
                "{} blocks native intents. Switch to Agent or Assisted mode to run this plan.",
                self.browser_mode.label()
            );
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        }

        self.last_intent = intent.clone();
        self.pilot_status = "PLANNING".to_string();
        self.pilot_result = "Planning native browser work.".to_string();
        self.pilot_plan = vec![
            "Read intent".to_string(),
            "Resolve navigation target".to_string(),
        ];
        self.pending_consent = None;

        #[cfg(feature = "xilem-shell")]
        {
            if use_brain {
                self.run_pilot_action_intent(&intent);
            } else {
                self.plan_and_execute_heuristic(&intent);
            }
        }

        #[cfg(not(feature = "xilem-shell"))]
        {
            let _ = use_brain;
            if intent_needs_consent(&intent) {
                self.pilot_status = "AWAITING CONSENT".to_string();
                self.pilot_plan
                    .push("Hold before performing sensitive action".to_string());
                self.pilot_result =
                    "Sensitive intent paused. Consent UX will own this path next.".to_string();
                self.pending_consent = Some(PendingConsent {
                    intent: intent.clone(),
                    message: self.pilot_result.clone(),
                    #[cfg(feature = "xilem-shell")]
                    remaining_actions: Vec::new(),
                });
                self.last_status = format!("Intent paused for consent: {}", truncate(&intent, 56));
                self.last_ok = true;
                let _ = self.record_log(
                    &format!("native intent {}", intent),
                    LogStatus::AwaitingConsent,
                );
                return;
            }

            let plan = match plan_native_intent(&intent) {
                Ok(plan) => plan,
                Err(error) => {
                    self.pilot_status = "FAILED".to_string();
                    self.pilot_result = error.clone();
                    self.last_status = error.clone();
                    self.last_ok = false;
                    self.validation.error_seen = true;
                    let _ = self.record_log(
                        &format!("native intent {}", intent),
                        LogStatus::Failure(error),
                    );
                    return;
                }
            };

            self.pilot_plan = plan.steps.clone();
            self.pilot_status = "NAVIGATING".to_string();
            self.navigate_to(plan.target.clone());
            if !self.last_ok {
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = self.last_status.clone();
                let _ = self.record_log(
                    &format!("native intent {}", plan.intent),
                    LogStatus::Failure(self.last_status.clone()),
                );
                return;
            }

            if plan.should_distill {
                self.pilot_status = "DISTILLING".to_string();
                self.distill_active();
                if !self.last_ok {
                    self.pilot_status = "FAILED".to_string();
                    self.pilot_result = self.last_status.clone();
                    let _ = self.record_log(
                        &format!("native intent {}", plan.intent),
                        LogStatus::Failure(self.last_status.clone()),
                    );
                    return;
                }
            }

            self.pilot_status = "COMPLETE".to_string();
            self.pilot_result = if plan.should_distill {
                format!(
                    "Opened {} and stored the distilled page in Wake.",
                    short_url(&plan.target)
                )
            } else {
                format!("Opened {}.", short_url(&plan.target))
            };
            self.last_status = format!("Intent complete: {}", truncate(&plan.intent, 58));
            self.last_ok = true;
            let _ = self.record_log(
                &format!("native intent {}", plan.intent),
                LogStatus::Success,
            );
        }
    }

    #[cfg(feature = "xilem-shell")]
    pub fn run_pilot_action_intent(&mut self, intent: &str) {
        // Dispatch the intent to the local model brain off-thread; the resulting
        // plan is picked up by `collect_pending_pilot_plan` and executed then. If
        // the brain is unreachable we fall back to the built-in heuristic planner
        // so the agent keeps working when no model server is up.
        if self.pending_pilot_plan.is_some() {
            self.last_status = "A pilot intent is already being planned.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        }

        self.pilot_status = "PILOT REASONING".to_string();
        self.pilot_result = format!("Reasoning with {}...", self.pilot_brain_lane.describe());
        let context = self.wake_results.clone();
        match self.pilot_brain_lane.reason(intent.to_string(), context) {
            Ok(result_rx) => {
                self.last_status =
                    format!("Planning intent with local model: {}", truncate(intent, 48));
                self.last_ok = true;
                self.pilot_plan = vec!["Reasoning with local model".to_string()];
                self.pending_pilot_plan = Some(PendingPilotPlan {
                    intent: intent.to_string(),
                    result_rx,
                    started: Instant::now(),
                });
            }
            Err(error) => {
                self.last_status =
                    format!("Local model unavailable ({error}); using built-in planner.");
                self.validation.error_seen = true;
                self.plan_and_execute_heuristic(intent);
            }
        }
    }

    /// Built-in deterministic planner used as a fallback when the local model is
    /// unreachable or returns an error. This is the legacy `run_pilot_action_intent`
    /// behavior, preserved so the agent degrades gracefully instead of failing hard.
    #[cfg(feature = "xilem-shell")]
    pub fn plan_and_execute_heuristic(&mut self, intent: &str) {
        match pilot_action_plan(intent) {
            Ok(actions) => self.execute_planned_actions(intent, actions, "builtin-heuristic", 0),
            Err(error) => {
                self.pilot_status = "FAILED".to_string();
                self.pilot_result = error.clone();
                self.last_status = error.clone();
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(
                    &format!("pilot intent {}", intent),
                    LogStatus::Failure(error),
                );
            }
        }
    }

    /// Execute a resolved action plan (from either the brain or the heuristic) and
    /// capture a structured `PilotRunArtifact` of the run for the shell/dashboard.
    #[cfg(feature = "xilem-shell")]
    pub fn execute_planned_actions(
        &mut self,
        intent: &str,
        actions: Vec<PilotAction>,
        planner: &str,
        reason_ms: u128,
    ) {
        self.pilot_plan = actions.iter().map(pilot_action_step_label).collect();
        let plan_records: Vec<PilotStepRecord> = actions.iter().map(pilot_action_record).collect();
        let mut analysis = Vec::new();
        let completed = self.execute_pilot_actions(intent, actions, &mut analysis);
        if completed {
            self.finish_successful_pilot_intent(intent, analysis.clone());
        }
        let artifact = PilotRunArtifact {
            intent: intent.to_string(),
            planner: planner.to_string(),
            plan: plan_records,
            analysis,
            status: self.pilot_status.clone(),
            reason_ms,
        };
        // Emit the structured run as JSON: proves the structured stream and gives a
        // future visual dashboard / MCP a stable artifact to consume.
        if let Ok(json) = serde_json::to_string(&artifact) {
            println!("[pilot-run] {json}");
        }
        self.last_pilot_run = Some(artifact);
    }

    /// Whether a local-model plan is currently in flight. Always defined so the
    /// event-loop control-flow chain compiles without the `xilem-shell` feature.
    pub fn pilot_plan_pending(&self) -> bool {
        #[cfg(feature = "xilem-shell")]
        {
            self.pending_pilot_plan.is_some()
        }
        #[cfg(not(feature = "xilem-shell"))]
        {
            false
        }
    }

    /// Per-frame poll of the brain lane (mirrors `collect_pending_distillation`).
    #[cfg(feature = "xilem-shell")]
    pub fn collect_pending_pilot_plan(&mut self) -> Option<Duration> {
        let pending = self.pending_pilot_plan.as_ref()?;
        match pending.result_rx.try_recv() {
            Ok(result) => {
                let pending = self
                    .pending_pilot_plan
                    .take()
                    .expect("pending pilot plan disappeared");
                let elapsed = pending.started.elapsed();
                let intent = pending.intent;
                match result {
                    Ok(actions) => {
                        self.execute_planned_actions(
                            &intent,
                            actions,
                            "local-model",
                            elapsed.as_millis(),
                        );
                    }
                    Err(error) => {
                        self.last_status = format!(
                            "Local model planning failed ({error}); using built-in planner."
                        );
                        self.validation.error_seen = true;
                        self.plan_and_execute_heuristic(&intent);
                    }
                }
                Some(elapsed)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let pending = self
                    .pending_pilot_plan
                    .take()
                    .expect("pending pilot plan disappeared");
                let elapsed = pending.started.elapsed();
                let intent = pending.intent;
                self.last_status =
                    "Local model lane dropped the plan; using built-in planner.".to_string();
                self.validation.error_seen = true;
                self.plan_and_execute_heuristic(&intent);
                Some(elapsed)
            }
        }
    }

    #[cfg(feature = "xilem-shell")]
    pub fn execute_pilot_actions(
        &mut self,
        intent: &str,
        actions: Vec<PilotAction>,
        analysis: &mut Vec<String>,
    ) -> bool {
        for (index, action) in actions.iter().cloned().enumerate() {
            match action {
                PilotAction::Navigate(url) => {
                    self.pilot_status = "PILOT NAVIGATING".to_string();
                    self.navigate_to(url);
                    if !self.last_ok {
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                }
                PilotAction::OpenTab(url) => {
                    self.pilot_status = "PILOT OPENING TAB".to_string();
                    self.new_tab();
                    self.navigate_to(url);
                    if !self.last_ok {
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                }
                PilotAction::Distill => {
                    self.pilot_status = "PILOT DISTILLING".to_string();
                    self.distill_active();
                    if !self.last_ok {
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                }
                PilotAction::Perceive => {
                    self.pilot_status = "PILOT PERCEIVING".to_string();
                    self.distill_active();
                    if !self.last_ok {
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                    if let Some(page) = self
                        .active_tab()
                        .and_then(|tab| tab.distilled_page.as_ref())
                    {
                        analysis.push(page_perception_summary(page));
                        self.main_view = MainView::Perception;
                    }
                }
                PilotAction::PerceiveMultiModal => {
                    self.pilot_status = "PILOT PERCEIVING".to_string();
                    analysis.push(
                        "Multimodal perception is queued for the Neural Bridge lane.".to_string(),
                    );
                }
                PilotAction::Analyze(message) => {
                    analysis.push(message);
                }
                PilotAction::RequestConsent(message) => {
                    let remaining_actions = actions[index + 1..].to_vec();
                    self.pilot_status = "AWAITING CONSENT".to_string();
                    self.pilot_result = message.clone();
                    self.pending_consent = Some(PendingConsent {
                        intent: intent.to_string(),
                        message,
                        remaining_actions,
                    });
                    self.last_status = format!("Pilot awaiting consent: {}", truncate(intent, 56));
                    self.last_ok = true;
                    let _ = self.record_log(
                        &format!("pilot intent {}", intent),
                        LogStatus::AwaitingConsent,
                    );
                    return false;
                }
                PilotAction::SwitchTab(tab_id) => match self.engine.switch_to_tab(tab_id) {
                    Ok(()) => {
                        self.show_page_tab(tab_id);
                        self.sync_address_to_active_tab();
                        self.refresh_frame();
                    }
                    Err(error) => {
                        self.last_status = format!("Pilot tab switch failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                },
                PilotAction::CloseTab(tab_id) => match self.engine.switch_to_tab(tab_id) {
                    Ok(()) => {
                        self.close_tab();
                        if !self.last_ok {
                            self.finish_failed_pilot_intent(intent);
                            return false;
                        }
                    }
                    Err(error) => {
                        self.last_status = format!("Pilot tab close failed: {}", error);
                        self.last_ok = false;
                        self.validation.error_seen = true;
                        self.finish_failed_pilot_intent(intent);
                        return false;
                    }
                },
            }
        }
        true
    }

    #[cfg(feature = "xilem-shell")]
    pub fn finish_successful_pilot_intent(&mut self, intent: &str, analysis: Vec<String>) {
        self.pilot_status = "PILOT COMPLETE".to_string();
        self.pilot_result = analysis
            .last()
            .cloned()
            .unwrap_or_else(|| "Pilot actions completed.".to_string());
        self.last_status = format!("Pilot intent complete: {}", truncate(intent, 58));
        self.last_ok = true;
        let _ = self.record_log(&format!("pilot intent {}", intent), LogStatus::Success);
    }

    #[cfg(feature = "xilem-shell")]
    pub fn finish_failed_pilot_intent(&mut self, intent: &str) {
        self.pilot_status = "FAILED".to_string();
        self.pilot_result = self.last_status.clone();
        let _ = self.record_log(
            &format!("pilot intent {}", intent),
            LogStatus::Failure(self.last_status.clone()),
        );
    }

    pub fn authorize_pilot_consent(&mut self) {
        let Some(pending) = self.pending_consent.clone() else {
            self.last_status = "No Pilot consent request is pending.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        };

        let signature = match self.sign_pending_consent(&pending) {
            Ok(signature) => signature,
            Err(error) => {
                self.pilot_status = "CONSENT FAILED".to_string();
                self.pilot_result = error.clone();
                self.last_status = format!("Captain's Key authorization failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ = self.record_log(
                    &format!("pilot consent authorize {}", pending.intent),
                    LogStatus::Failure(error),
                );
                return;
            }
        };

        self.pending_consent = None;
        self.pilot_status = "CONSENT AUTHORIZED".to_string();
        self.pilot_result = format!(
            "Captain's Key authorized. Signature {}.",
            truncate(&signature, 18)
        );
        self.last_status = format!(
            "Authorized Pilot consent for: {}",
            truncate(&pending.intent, 52)
        );
        self.last_ok = true;
        let _ = self.record_log_with_consent(
            &format!("pilot consent authorize {}", pending.intent),
            LogStatus::Success,
            Some(signature.clone()),
        );
        self.refresh_logs();

        #[cfg(feature = "xilem-shell")]
        {
            if !pending.remaining_actions.is_empty() {
                self.pilot_status = "CONSENT RESUMING".to_string();
                self.pilot_result =
                    "Captain's Key authorized. Resuming gated browser plan.".to_string();
                self.pilot_plan = pending
                    .remaining_actions
                    .iter()
                    .map(pilot_action_step_label)
                    .collect();
                let mut analysis = vec![format!(
                    "Captain's Key Authorized with signature {}.",
                    truncate(&signature, 18)
                )];
                if self.execute_pilot_actions(
                    &pending.intent,
                    pending.remaining_actions.clone(),
                    &mut analysis,
                ) {
                    self.finish_successful_pilot_intent(&pending.intent, analysis);
                    let _ = self.record_log_with_consent(
                        &format!("pilot consent resume {}", pending.intent),
                        LogStatus::Success,
                        Some(signature),
                    );
                }
                self.refresh_logs();
            }
        }
    }

    pub fn deny_pilot_consent(&mut self) {
        let Some(pending) = self.pending_consent.take() else {
            self.last_status = "No Pilot consent request is pending.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        };

        self.pilot_status = "CONSENT DENIED".to_string();
        self.pilot_result = format!("Captain's Key denied: {}", truncate(&pending.message, 72));
        self.last_status = format!(
            "Denied Pilot consent for: {}",
            truncate(&pending.intent, 52)
        );
        self.last_ok = true;
        let _ = self.record_log(
            &format!("pilot consent deny {}", pending.intent),
            LogStatus::Aborted,
        );
        self.refresh_logs();
    }

    pub fn sign_pending_consent(&self, pending: &PendingConsent) -> Result<String, String> {
        #[cfg(feature = "xilem-shell")]
        {
            self.consent_vault.sign_consent(&pending.payload())
        }
        #[cfg(not(feature = "xilem-shell"))]
        {
            Ok(format!(
                "reader-consent-{}-{}",
                Uuid::new_v4(),
                truncate(&pending.payload(), 12)
            ))
        }
    }
}
