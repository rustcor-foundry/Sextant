use serde::{Serialize, Deserialize};
use url::Url;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum FirewallAction {
    Allow,
    Block,
    Audit, // Allow but log extensively
    Isolate, // Allow only in a high-security sandbox
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FirewallRule {
    pub domain_pattern: String,
    pub action: FirewallAction,
    pub reason: String,
}

pub struct SextantFirewall {
    persona_rules: HashMap<String, Vec<FirewallRule>>,
    global_blacklist: Vec<String>,
}

impl SextantFirewall {
    pub fn new() -> Self {
        let mut persona_rules = HashMap::new();
        
        // Default Work Persona Rules
        persona_rules.insert("Work".to_string(), vec![
            FirewallRule {
                domain_pattern: "*.google.com".into(),
                action: FirewallAction::Allow,
                reason: "Productivity suite".into(),
            },
            FirewallRule {
                domain_pattern: "github.com".into(),
                action: FirewallAction::Allow,
                reason: "Source control".into(),
            },
            FirewallRule {
                domain_pattern: "facebook.com".into(),
                action: FirewallAction::Block,
                reason: "Social media restricted in Work persona".into(),
            },
        ]);

        // Default Personal Persona Rules
        persona_rules.insert("Personal".to_string(), vec![
            FirewallRule {
                domain_pattern: "*".into(),
                action: FirewallAction::Allow,
                reason: "Unrestricted personal browsing".into(),
            },
        ]);

        Self {
            persona_rules,
            global_blacklist: vec!["malicious-site.net".into(), "tracking-pixel.com".into()],
        }
    }

    pub fn check_access(&self, persona_id: &str, url: &Url) -> (FirewallAction, String) {
        let domain = url.domain().unwrap_or("");

        // 1. Check Global Blacklist
        if self.global_blacklist.iter().any(|b| domain.contains(b)) {
            return (FirewallAction::Block, "Global blacklist match".into());
        }

        // 2. Check Persona Rules
        if let Some(rules) = self.persona_rules.get(persona_id) {
            for rule in rules {
                if self.match_pattern(&rule.domain_pattern, domain) {
                    return (rule.action.clone(), rule.reason.clone());
                }
            }
        }

        // Default: Allow but Audit if no rules match
        (FirewallAction::Audit, "No specific rule matched".into())
    }

    fn match_pattern(&self, pattern: &str, domain: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if pattern.starts_with("*.") {
            let suffix = &pattern[2..];
            return domain.ends_with(suffix);
        }
        pattern == domain
    }

    pub fn add_rule(&mut self, persona_id: &str, rule: FirewallRule) {
        self.persona_rules.entry(persona_id.to_string()).or_default().push(rule);
    }
}
