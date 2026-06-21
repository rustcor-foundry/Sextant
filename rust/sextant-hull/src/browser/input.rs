//! `BrowserApp` input handling — click/key/shortcut/mouse routing, viewport
//! input forwarding + debounced text, scroll, and focus helpers.

use super::*;

impl BrowserApp {
    pub fn click(&mut self, x: f64, y: f64) {
        if self.address_rect.contains(x, y) {
            self.focus = FocusTarget::Address;
            return;
        }
        if self.wake_input_hit_rect().contains(x, y) {
            self.focus = FocusTarget::Wake;
            self.last_status = "Wake search focused.".to_string();
            self.last_ok = true;
            return;
        }
        if let Some(mode) = self
            .mode_rects
            .iter()
            .find(|region| region.rect.contains(x, y))
            .map(|region| region.mode)
        {
            self.set_browser_mode(mode);
            return;
        }
        if self.browser_tab_rect.contains(x, y) {
            self.main_view = MainView::Browser;
            return;
        }
        if self.wake_tab_rect.contains(x, y) {
            self.main_view = MainView::Wake;
            return;
        }
        if self.log_tab_rect.contains(x, y) {
            self.main_view = MainView::Log;
            return;
        }
        if self.guard_tab_rect.contains(x, y) {
            self.main_view = MainView::Guard;
            return;
        }
        if self.perception_tab_rect.contains(x, y) {
            self.main_view = MainView::Perception;
            return;
        }
        if self.perf_tab_rect.contains(x, y) {
            self.main_view = MainView::Perf;
            return;
        }
        if self.validation_tab_rect.contains(x, y) {
            self.main_view = MainView::Validation;
            return;
        }
        #[cfg(feature = "xilem-shell")]
        if self.settings_tab_rect.contains(x, y) {
            self.main_view = MainView::Settings;
            return;
        }
        if self.can_page_tabs_previous() && self.page_tab_prev_rect.contains(x, y) {
            self.page_tabs_previous();
            return;
        }
        if self.can_page_tabs_next() && self.page_tab_next_rect.contains(x, y) {
            self.page_tabs_next();
            return;
        }
        if self.pending_consent.is_some() && self.consent_authorize_rect.contains(x, y) {
            self.authorize_pilot_consent();
            return;
        }
        if self.pending_consent.is_some() && self.consent_deny_rect.contains(x, y) {
            self.deny_pilot_consent();
            return;
        }
        #[cfg(feature = "xilem-shell")]
        if self.pending_consent.is_none() && self.native_intent_allowed() {
            if self.consent_authorize_rect.contains(x, y) {
                self.run_preset_intent(AI_RAIL_PRESETS[0].1);
                return;
            }
            if self.consent_deny_rect.contains(x, y) {
                self.run_preset_intent(AI_RAIL_PRESETS[1].1);
                return;
            }
        }
        if let Some(tab_id) = self
            .page_tab_rects
            .iter()
            .find(|tab| tab.rect.contains(x, y))
            .map(|tab| tab.tab_id)
        {
            self.switch_to_page_tab(tab_id);
            return;
        }

        if self.main_view == MainView::Guard && self.handle_guard_panel_click(x, y) {
            return;
        }

        #[cfg(feature = "xilem-shell")]
        if self.main_view == MainView::Settings && self.handle_settings_panel_click(x, y) {
            return;
        }

        let Some(action) = self
            .buttons
            .iter()
            .find(|button| button.rect.contains(x, y))
            .map(|button| button.action)
        else {
            self.last_status = "Click missed a control.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        };

        if !self.action_enabled(action) {
            self.last_status = self.disabled_reason(action);
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        }

        self.run_action(action);
    }

    pub fn handle_guard_panel_click(&mut self, x: f64, y: f64) -> bool {
        let panel = main_panel_rect(
            right_rail_x(self.window_size.width.max(1)),
            self.window_size.height.max(1),
        );
        if guard_appliance_refresh_rect(panel).contains(x, y) {
            self.refresh_appliance_cert_trust();
            return true;
        }
        if guard_appliance_forget_rect(panel).contains(x, y) {
            self.forget_selected_appliance_cert();
            return true;
        }
        #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
        {
            for (index, rect) in guard_appliance_row_rects(panel, self.appliance_cert_entries.len())
            {
                if rect.contains(x, y) {
                    self.selected_appliance_cert = Some(index);
                    if let Some(entry) = self.appliance_cert_entries.get(index) {
                        self.last_status =
                            format!("Selected appliance certificate trust for {}.", entry.origin);
                        self.appliance_cert_status = self.last_status.clone();
                        self.last_ok = true;
                    }
                    return true;
                }
            }
        }
        false
    }

    #[cfg(feature = "xilem-shell")]
    pub fn handle_settings_panel_click(&mut self, x: f64, y: f64) -> bool {
        let panel = main_panel_rect(
            right_rail_x(self.window_size.width.max(1)),
            self.window_size.height.max(1),
        );
        for (backend, rect) in settings_backend_button_rects(panel) {
            if rect.contains(x, y) {
                self.apply_ai_backend(backend);
                return true;
            }
        }
        false
    }

    /// Switch the local AI backend, persist it, and rebuild the brain lane so the
    /// next intent plans through the newly selected server.
    #[cfg(feature = "xilem-shell")]
    pub fn apply_ai_backend(&mut self, backend: &str) {
        if self.pending_pilot_plan.is_some() {
            // Rebuilding the lane joins its worker thread, which would block on an
            // in-flight model call. Make the user finish the current intent first.
            self.last_status =
                "Finish the current intent before changing the AI backend.".to_string();
            self.last_ok = false;
            self.validation.error_seen = true;
            return;
        }
        if self.ai_config.backend == backend {
            self.last_status = format!(
                "{} backend is already active.",
                AiLocalConfig::backend_label(backend)
            );
            self.last_ok = true;
            return;
        }
        let config = AiLocalConfig::default_for(backend);
        match config.save(&self.profile_data_dir) {
            Ok(()) => {
                self.ai_config = config;
                self.pilot_brain_lane = PilotBrainLane::new(&self.ai_config);
                self.last_status = format!(
                    "AI backend set to {} @ {} ({}).",
                    AiLocalConfig::backend_label(&self.ai_config.backend),
                    self.ai_config.endpoint,
                    self.ai_config.model
                );
                self.last_ok = true;
                let _ = self.record_log(
                    &format!("ai backend {}", self.ai_config.backend),
                    LogStatus::Success,
                );
            }
            Err(error) => {
                self.last_status = format!("Failed to save AI settings: {error}");
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    pub fn wake_input_hit_rect(&self) -> Rect {
        Rect {
            x: self.wake_rect.x,
            y: self.wake_rect.y.saturating_sub(24),
            w: self.wake_rect.w,
            h: self.wake_rect.h.saturating_add(24),
        }
    }

    pub fn switch_to_page_tab(&mut self, tab_id: Uuid) {
        self.flush_pending_browser_text();
        self.flush_viewport_input_lane();
        self.clear_viewport_input_lane();
        match self.engine.switch_to_tab(tab_id) {
            Ok(()) => {
                self.show_page_tab(tab_id);
                self.sync_address_to_active_tab();
                self.main_view = MainView::Browser;
                self.page_scroll = 0;
                self.begin_frame_warmup();
                self.last_frame_viewport = None;
                self.refresh_render_bridge_frame_after_visible_tab_change();
                self.layout(self.window_size);
                self.last_status = format!("Switched to tab {}.", short_id(tab_id));
                self.last_ok = true;
                self.validation.tab_control_seen = true;
                let _ = self.record_log(&format!("switch tab {}", tab_id), LogStatus::Success);
            }
            Err(error) => {
                self.last_status = format!("Tab switch failed: {}", error);
                self.last_ok = false;
                self.validation.error_seen = true;
                let _ =
                    self.record_log(&format!("switch tab {}", tab_id), LogStatus::Failure(error));
            }
        }
    }

    pub fn handle_key(&mut self, event: KeyEvent) {
        if event.state == ElementState::Pressed && self.handle_shortcut(&event.logical_key) {
            return;
        }

        if self.focus == FocusTarget::Browser && self.latest_frame.is_some() {
            self.forward_browser_key(&event);
            return;
        }

        if event.state != ElementState::Pressed {
            return;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => match self.focus {
                FocusTarget::Address => self.navigate_input(),
                FocusTarget::Wake => self.search_wake(),
                FocusTarget::Browser => {}
            },
            Key::Named(NamedKey::Tab) => {
                self.focus = match self.focus {
                    FocusTarget::Address => FocusTarget::Wake,
                    FocusTarget::Wake => FocusTarget::Address,
                    FocusTarget::Browser => FocusTarget::Address,
                };
            }
            Key::Named(NamedKey::Backspace) => {
                self.focused_text_mut().pop();
            }
            Key::Named(NamedKey::Space) => {
                self.focused_text_mut().push(' ');
            }
            Key::Character(value) => {
                if value.chars().all(|c| !c.is_control()) {
                    self.focused_text_mut().push_str(value);
                }
            }
            _ => {}
        }
    }

    pub fn handle_shortcut(&mut self, key: &Key) -> bool {
        let Some(shortcut) = browser_shortcut_for_key(self.modifiers, key) else {
            return false;
        };
        match shortcut {
            BrowserShortcut::FocusAddress => {
                self.focus = FocusTarget::Address;
                self.main_view = MainView::Browser;
                self.last_status = "Address bar focused.".to_string();
                self.last_ok = true;
            }
            BrowserShortcut::NewTab => self.run_action(Action::NewTab),
            BrowserShortcut::CloseTab => self.run_action(Action::CloseTab),
            BrowserShortcut::Reload => self.run_shortcut_action(Action::Reload),
            BrowserShortcut::Back => self.run_shortcut_action(Action::Back),
            BrowserShortcut::Forward => self.run_shortcut_action(Action::Forward),
        }
        true
    }

    pub fn run_shortcut_action(&mut self, action: Action) {
        if self.action_enabled(action) {
            self.run_action(action);
        } else {
            self.last_status = self.disabled_reason(action);
            self.last_ok = false;
            self.validation.error_seen = true;
        }
    }

    pub fn handle_mouse_input(&mut self, x: f64, y: f64, state: ElementState) -> bool {
        if self.main_view != MainView::Browser || !self.browser_viewport_rect.contains(x, y) {
            return false;
        }

        self.flush_pending_browser_text();
        self.focus = FocusTarget::Browser;
        self.last_status = "Browser viewport focused.".to_string();
        self.last_ok = true;
        self.validation.browser_focus_seen = true;
        let Some((local_x, local_y)) = self.browser_point(x, y) else {
            return true;
        };
        let moved = self.forward_browser_mouse_move(local_x, local_y, true);
        let clicked = self.queue_viewport_input(ViewportInputEvent::MouseButton {
            x: local_x,
            y: local_y,
            pressed: state == ElementState::Pressed,
        });
        if moved || clicked {
            self.validation.browser_input_seen = true;
        }
        true
    }

    pub fn handle_mouse_move(&mut self, x: f64, y: f64) {
        if self.focus != FocusTarget::Browser || !self.browser_viewport_rect.contains(x, y) {
            return;
        }
        if let Some((local_x, local_y)) = self.browser_point(x, y) {
            self.forward_browser_mouse_move(local_x, local_y, false);
        }
    }

    pub fn should_forward_browser_mouse_move(&self, local_x: f32, local_y: f32) -> bool {
        let Some(last_forward) = self.last_viewport_mouse_move_forward else {
            return true;
        };
        if last_forward.elapsed() < VIEWPORT_MOUSE_MOVE_MIN_INTERVAL {
            return false;
        }
        let Some((last_x, last_y)) = self.last_viewport_mouse_move_point else {
            return true;
        };
        (local_x - last_x).abs() >= VIEWPORT_MOUSE_MOVE_MIN_DISTANCE_PX
            || (local_y - last_y).abs() >= VIEWPORT_MOUSE_MOVE_MIN_DISTANCE_PX
    }

    pub fn forward_browser_mouse_move(&mut self, local_x: f32, local_y: f32, force: bool) -> bool {
        if !force && !self.should_forward_browser_mouse_move(local_x, local_y) {
            return false;
        }
        if !self.queue_viewport_input(ViewportInputEvent::MouseMove {
            x: local_x,
            y: local_y,
        }) {
            return false;
        }
        self.last_viewport_mouse_move_forward = Some(Instant::now());
        self.last_viewport_mouse_move_point = Some((local_x, local_y));
        true
    }

    pub fn forward_browser_key(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        match &event.logical_key {
            Key::Character(value) if value.chars().all(|c| !c.is_control()) => {
                if !pressed {
                    return;
                }
                self.queue_browser_text(value);
            }
            Key::Named(NamedKey::Space) => {
                if !pressed {
                    return;
                }
                self.queue_browser_text(" ");
            }
            Key::Named(named) => {
                if let Some(key) = browser_key_from_winit(named) {
                    self.flush_pending_browser_text();
                    if self.queue_viewport_input(ViewportInputEvent::KeyNamed { key, pressed }) {
                        self.validation.browser_input_seen = true;
                    }
                }
            }
            _ => {}
        }
    }

    pub fn queue_browser_text(&mut self, value: &str) {
        let Some(active_tab_id) = self.active_tab().map(|tab| tab.id) else {
            return;
        };
        if self.pending_browser_text_tab != Some(active_tab_id) {
            self.pending_browser_text.clear();
            self.pending_browser_text_tab = Some(active_tab_id);
        }
        self.pending_browser_text.push_str(value);
        self.last_browser_text_input = Some(Instant::now());
        self.validation.browser_input_seen = true;
    }

    pub fn pending_browser_text_due(&self) -> Option<Instant> {
        if self.pending_browser_text.is_empty() {
            return None;
        }
        self.last_browser_text_input
            .map(|last_input| last_input + VIEWPORT_TEXT_INPUT_DEBOUNCE)
    }

    pub fn flush_pending_browser_text_if_due(&mut self) -> bool {
        let Some(due) = self.pending_browser_text_due() else {
            return false;
        };
        if Instant::now() < due {
            return false;
        }
        self.flush_pending_browser_text()
    }

    pub fn flush_pending_browser_text(&mut self) -> bool {
        if self.pending_browser_text.is_empty() {
            return false;
        }
        if self.pending_browser_text_tab != self.active_tab().map(|tab| tab.id) {
            self.pending_browser_text.clear();
            self.pending_browser_text_tab = None;
            self.last_browser_text_input = None;
            return false;
        }
        let text = std::mem::take(&mut self.pending_browser_text);
        self.pending_browser_text_tab = None;
        self.last_browser_text_input = None;
        if !self.queue_viewport_input(ViewportInputEvent::Text { text }) {
            return false;
        }
        self.validation.browser_input_seen = true;
        true
    }

    pub fn queue_viewport_input(&mut self, event: ViewportInputEvent) -> bool {
        let Some(active_tab_id) = self.active_tab().map(|tab| tab.id) else {
            return false;
        };
        if self.pending_viewport_input_tab != Some(active_tab_id) {
            self.pending_viewport_input.clear();
            self.pending_viewport_input_tab = Some(active_tab_id);
        }
        match event {
            ViewportInputEvent::MouseMove { x, y } => {
                if let Some(ViewportInputEvent::MouseMove {
                    x: last_x,
                    y: last_y,
                }) = self.pending_viewport_input.back_mut()
                {
                    *last_x = x;
                    *last_y = y;
                } else {
                    self.pending_viewport_input
                        .push_back(ViewportInputEvent::MouseMove { x, y });
                }
            }
            ViewportInputEvent::Wheel {
                delta_x,
                delta_y,
                pixel_mode,
            } => {
                if let Some(ViewportInputEvent::Wheel {
                    delta_x: last_delta_x,
                    delta_y: last_delta_y,
                    pixel_mode: last_pixel_mode,
                }) = self.pending_viewport_input.back_mut()
                {
                    if *last_pixel_mode == pixel_mode {
                        *last_delta_x += delta_x;
                        *last_delta_y += delta_y;
                    } else {
                        self.pending_viewport_input
                            .push_back(ViewportInputEvent::Wheel {
                                delta_x,
                                delta_y,
                                pixel_mode,
                            });
                    }
                } else {
                    self.pending_viewport_input
                        .push_back(ViewportInputEvent::Wheel {
                            delta_x,
                            delta_y,
                            pixel_mode,
                        });
                }
            }
            ViewportInputEvent::Text { text } => {
                if text.is_empty() {
                    return false;
                }
                if let Some(ViewportInputEvent::Text { text: last_text }) =
                    self.pending_viewport_input.back_mut()
                {
                    last_text.push_str(&text);
                } else {
                    self.pending_viewport_input
                        .push_back(ViewportInputEvent::Text { text });
                }
            }
            other => self.pending_viewport_input.push_back(other),
        }
        self.validation.browser_input_seen = true;
        self.begin_interaction_frame_warmup();
        true
    }

    pub fn clear_viewport_input_lane(&mut self) {
        self.pending_viewport_input.clear();
        self.pending_viewport_input_tab = None;
    }

    pub fn flush_viewport_input_lane(&mut self) -> bool {
        let active_tab_id = self.active_tab().map(|tab| tab.id);
        if self.pending_viewport_input_tab != active_tab_id {
            self.clear_viewport_input_lane();
            return false;
        }
        let started = Instant::now();
        let mut flushed = false;
        while let Some(event) = self.pending_viewport_input.pop_front() {
            let result = match event {
                ViewportInputEvent::MouseMove { x, y } => {
                    self.engine.enqueue_mouse_move_current_viewport(x, y)
                }
                ViewportInputEvent::MouseButton { x, y, pressed } => self
                    .engine
                    .enqueue_mouse_button_current_viewport(x, y, pressed),
                ViewportInputEvent::Wheel {
                    delta_x,
                    delta_y,
                    pixel_mode,
                } => self
                    .engine
                    .enqueue_wheel_current_viewport(delta_x, delta_y, pixel_mode),
                ViewportInputEvent::KeyNamed { key, pressed } => {
                    self.engine.enqueue_key_named_current_viewport(key, pressed)
                }
                ViewportInputEvent::Text { text } => self
                    .engine
                    .enqueue_key_character_current_viewport(text, true),
            };
            if result.is_ok() {
                flushed = true;
            }
        }
        self.pending_viewport_input_tab = None;
        if flushed {
            self.last_viewport_input_flush = Some(Instant::now());
            self.record_perf("input", started.elapsed(), "viewport enqueue");
        }
        flushed
    }

    pub fn browser_point(&self, x: f64, y: f64) -> Option<(f32, f32)> {
        if !self.browser_viewport_rect.contains(x, y) {
            return None;
        }
        Some((
            (x - self.browser_viewport_rect.x as f64).max(0.0) as f32,
            (y - self.browser_viewport_rect.y as f64).max(0.0) as f32,
        ))
    }

    pub fn scroll_at(&mut self, x: f64, y: f64, delta: &MouseScrollDelta, size: PhysicalSize<u32>) {
        let rail_x = right_rail_x(size.width.max(1));
        let tab_strip = Rect {
            x: 0,
            y: CHROME_H + STRIP_H + TAB_H,
            w: rail_x,
            h: PAGE_TAB_H,
        };
        if tab_strip.contains(x, y) {
            match delta {
                MouseScrollDelta::LineDelta(_, dy) if *dy > 0.0 => self.page_tabs_previous(),
                MouseScrollDelta::LineDelta(_, dy) if *dy < 0.0 => self.page_tabs_next(),
                MouseScrollDelta::PixelDelta(position) if position.y > 0.0 => {
                    self.page_tabs_previous()
                }
                MouseScrollDelta::PixelDelta(position) if position.y < 0.0 => self.page_tabs_next(),
                _ => {}
            }
            return;
        }
        if self.main_view != MainView::Browser {
            return;
        }
        let panel = main_panel_rect(rail_x, size.height.max(1));
        let viewport = browser_viewport_rect(panel);
        if !viewport.contains(x, y) {
            return;
        }
        let (delta_x, delta_y, pixel_mode) = match delta {
            MouseScrollDelta::LineDelta(dx, dy) => {
                ((*dx * 76.0) as f64, (*dy * 76.0) as f64, false)
            }
            MouseScrollDelta::PixelDelta(position) => (position.x, position.y, true),
        };
        if self.latest_frame.is_some()
            && self.queue_viewport_input(ViewportInputEvent::Wheel {
                delta_x,
                delta_y,
                pixel_mode,
            })
        {
            self.validation.browser_input_seen = true;
            return;
        }
        self.page_scroll = (self.page_scroll - delta_y as i32).clamp(0, 4000);
    }

    pub fn focused_text_mut(&mut self) -> &mut String {
        match self.focus {
            FocusTarget::Address => &mut self.address_input,
            FocusTarget::Wake => &mut self.wake_query,
            FocusTarget::Browser => unreachable!("browser focus does not edit shell text"),
        }
    }

    pub fn run_action(&mut self, action: Action) {
        if self.defer_user_navigation && is_deferred_user_navigation_action(action) {
            self.pending_user_action = Some(action);
            self.last_status = deferred_user_navigation_status(action).to_string();
            self.last_ok = true;
            self.main_view = MainView::Browser;
            return;
        }

        let mut refresh_logs_after = true;
        match action {
            Action::Navigate => self.navigate_input(),
            Action::RunShowcase => self.run_showcase_visible(),
            Action::NewTab => self.new_tab(),
            Action::Back => self.back(),
            Action::Forward => self.forward(),
            Action::Reload => self.reload(),
            Action::CloseTab => self.close_tab(),
            Action::ResetValidation => self.reset_validation(),
            Action::DistillActive => self.distill_active(),
            Action::SearchWake => {
                if self.defer_user_navigation {
                    self.start_visible_wake_search();
                    refresh_logs_after = false;
                } else {
                    self.search_wake();
                }
            }
        }
        if refresh_logs_after && !self.defer_user_navigation {
            self.refresh_logs();
        }
    }
}
