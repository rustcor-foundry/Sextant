//! `BrowserApp` guard decisions — navigation/interaction policy checks.

use super::*;

impl BrowserApp {
    pub fn guard_decision(&self, url: &Url) -> GuardDecision {
        #[cfg(feature = "xilem-shell")]
        {
            let airgap = SextantAirGap::new();
            let network_required = matches!(url.scheme(), "http" | "https");
            let network_allowed = !network_required || airgap.check_network_allowed();
            if !network_allowed {
                return GuardDecision {
                    action: "BLOCK",
                    reason: format!(
                        "{:?} air-gap blocks network navigation",
                        airgap.get_status()
                    ),
                    allowed: false,
                    network_allowed,
                };
            }

            let (action, reason) = self.guard_firewall.check_access(&self.persona_id, url);
            let allowed = !matches!(action, FirewallAction::Block);
            GuardDecision {
                action: firewall_action_label(&action),
                reason,
                allowed,
                network_allowed,
            }
        }

        #[cfg(not(feature = "xilem-shell"))]
        {
            let _ = url;
            GuardDecision {
                action: "OBSERVE",
                reason: "reader lane guard crates disabled".to_string(),
                allowed: true,
                network_allowed: true,
            }
        }
    }

    pub fn check_navigation_guard(
        &mut self,
        url: &Url,
        activity: &str,
    ) -> Result<GuardDecision, String> {
        let guard = self.guard_decision(url);
        if guard.allowed {
            if guard.action != "ALLOW" && guard.action != "OBSERVE" {
                let _ = self.record_log(&format!("guard {activity} {}", url), LogStatus::Success);
            }
            return Ok(guard);
        }

        let message = format!(
            "Local guard blocked {activity} to {}: {}",
            short_url(url),
            guard.summary()
        );
        let _ = self.record_log(
            &format!("guard {activity} {}", url),
            LogStatus::Failure(message.clone()),
        );
        Err(message)
    }

    pub fn check_interaction_guard(
        &mut self,
        result: &sextant_engine::BrowserInteractionResult,
        activity: &str,
    ) -> Result<(), String> {
        let Some(url) = result.current_url.as_ref() else {
            return Ok(());
        };
        let guard = self.guard_decision(url);
        if guard.allowed {
            return Ok(());
        }

        let message = format!(
            "Local guard blocked {activity} result at {}: {}",
            short_url(url),
            guard.summary()
        );
        self.last_status = message.clone();
        self.last_ok = false;
        self.validation.error_seen = true;
        let _ = self.record_log(
            &format!("guard {activity} {}", url),
            LogStatus::Failure(message.clone()),
        );
        Err(message)
    }
}
