use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use url::Url;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum FirewallAction {
    Allow,
    Block,
    Audit,   // Allow but log extensively
    Isolate, // Allow only in a high-security sandbox
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FirewallRule {
    pub domain_pattern: String,
    pub action: FirewallAction,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct FirewallPolicy {
    #[serde(default)]
    pub persona_rules: HashMap<String, Vec<FirewallRule>>,
    #[serde(default)]
    pub global_blacklist: Vec<String>,
}

pub struct SextantFirewall {
    persona_rules: HashMap<String, Vec<FirewallRule>>,
    global_blacklist: Vec<String>,
}

impl SextantFirewall {
    pub fn new() -> Self {
        let mut persona_rules = HashMap::new();

        // Default Work Persona Rules
        persona_rules.insert(
            "Work".to_string(),
            vec![
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
            ],
        );

        // Default Personal Persona Rules
        persona_rules.insert(
            "Personal".to_string(),
            vec![FirewallRule {
                domain_pattern: "*".into(),
                action: FirewallAction::Allow,
                reason: "Unrestricted personal browsing".into(),
            }],
        );

        Self {
            persona_rules,
            global_blacklist: vec!["malicious-site.net".into(), "tracking-pixel.com".into()],
        }
    }

    pub fn from_policy_overlay(policy: FirewallPolicy) -> Self {
        let mut firewall = Self::new();
        for domain in policy.global_blacklist {
            firewall.add_global_blacklist(domain);
        }
        for (persona_id, mut rules) in policy.persona_rules {
            let existing = firewall.persona_rules.entry(persona_id).or_default();
            rules.append(existing);
            *existing = rules;
        }
        firewall
    }

    pub fn load_policy_overlay(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|error| {
            format!(
                "Failed to read firewall policy {}: {}",
                path.display(),
                error
            )
        })?;
        let policy = serde_json::from_str::<FirewallPolicy>(&raw).map_err(|error| {
            format!(
                "Failed to parse firewall policy {}: {}",
                path.display(),
                error
            )
        })?;
        Ok(Self::from_policy_overlay(policy))
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
        self.persona_rules
            .entry(persona_id.to_string())
            .or_default()
            .push(rule);
    }

    pub fn add_global_blacklist(&mut self, domain_pattern: impl Into<String>) {
        let domain_pattern = domain_pattern.into();
        if !self.global_blacklist.contains(&domain_pattern) {
            self.global_blacklist.push(domain_pattern);
        }
    }

    pub fn persona_rules(&self, persona_id: &str) -> &[FirewallRule] {
        self.persona_rules
            .get(persona_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn global_blacklist(&self) -> &[String] {
        &self.global_blacklist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_blacklist_blocks_before_persona_rules() {
        let firewall = SextantFirewall::new();
        let url = Url::parse("https://malicious-site.net/path").unwrap();

        let (action, reason) = firewall.check_access("Personal", &url);

        assert_eq!(action, FirewallAction::Block);
        assert_eq!(reason, "Global blacklist match");
    }

    #[test]
    fn work_persona_blocks_social_media() {
        let firewall = SextantFirewall::new();
        let url = Url::parse("https://facebook.com/messages").unwrap();

        let (action, reason) = firewall.check_access("Work", &url);

        assert_eq!(action, FirewallAction::Block);
        assert!(reason.contains("Social media"));
    }

    #[test]
    fn unmatched_persona_audits_by_default() {
        let firewall = SextantFirewall::new();
        let url = Url::parse("https://example.com").unwrap();

        let (action, reason) = firewall.check_access("Unknown", &url);

        assert_eq!(action, FirewallAction::Audit);
        assert_eq!(reason, "No specific rule matched");
    }

    #[test]
    fn policy_overlay_adds_persona_rules() {
        let policy = FirewallPolicy {
            persona_rules: HashMap::from([(
                "browser-persona".to_string(),
                vec![FirewallRule {
                    domain_pattern: "example.com".to_string(),
                    action: FirewallAction::Block,
                    reason: "QA block".to_string(),
                }],
            )]),
            global_blacklist: Vec::new(),
        };
        let firewall = SextantFirewall::from_policy_overlay(policy);
        let url = Url::parse("https://example.com").unwrap();

        let (action, reason) = firewall.check_access("browser-persona", &url);

        assert_eq!(action, FirewallAction::Block);
        assert_eq!(reason, "QA block");
    }

    #[test]
    fn policy_overlay_persona_rules_take_precedence() {
        let policy = FirewallPolicy {
            persona_rules: HashMap::from([(
                "Work".to_string(),
                vec![FirewallRule {
                    domain_pattern: "facebook.com".to_string(),
                    action: FirewallAction::Audit,
                    reason: "Allow only with audit for launch demo".to_string(),
                }],
            )]),
            global_blacklist: Vec::new(),
        };
        let firewall = SextantFirewall::from_policy_overlay(policy);
        let url = Url::parse("https://facebook.com/messages").unwrap();

        let (action, reason) = firewall.check_access("Work", &url);

        assert_eq!(action, FirewallAction::Audit);
        assert!(reason.contains("launch demo"));
    }

    #[test]
    fn loads_policy_overlay_from_json() {
        let path = std::env::temp_dir().join(format!(
            "sextant-firewall-policy-{}.json",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(
            &path,
            r#"{
                "persona_rules": {
                    "browser-persona": [
                        {
                            "domain_pattern": "docs.example.com",
                            "action": "Block",
                            "reason": "temporary docs block"
                        }
                    ]
                },
                "global_blacklist": ["ads.example.net"]
            }"#,
        )
        .unwrap();

        let firewall = SextantFirewall::load_policy_overlay(&path).unwrap();
        let docs = Url::parse("https://docs.example.com/page").unwrap();
        let ads = Url::parse("https://ads.example.net/pixel").unwrap();

        assert_eq!(
            firewall.check_access("browser-persona", &docs).0,
            FirewallAction::Block
        );
        assert_eq!(
            firewall.check_access("browser-persona", &ads).0,
            FirewallAction::Block
        );

        let _ = std::fs::remove_file(path);
    }
}
