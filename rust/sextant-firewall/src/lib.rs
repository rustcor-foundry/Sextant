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

/// Persona key whose ruleset applies to any persona that has no ruleset of its
/// own. Lets operators define an explicit cross-persona baseline instead of
/// unknown personas silently falling through to the built-in default.
pub const ANY_PERSONA: &str = "*";

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
        // Use the full host (not `Url::domain`) so IP-literal URLs — which have
        // no registrable domain — are still subject to the blacklist and
        // persona rules instead of bypassing the trust boundary entirely.
        let Some(host) = url.host_str() else {
            // Hostless URLs (about:, data:, mailto:, ...) carry no network
            // destination for domain rules to evaluate; scheme policy is
            // enforced by the navigation guard, not the firewall.
            return (
                FirewallAction::Audit,
                "URL has no host; no domain rules apply".into(),
            );
        };
        let host = host.trim_end_matches('.').to_ascii_lowercase();

        // 1. Check Global Blacklist: exact host or subdomain match. (The old
        //    substring `contains` check both over-blocked unrelated domains and
        //    missed lookalikes.)
        if self
            .global_blacklist
            .iter()
            .any(|entry| Self::blacklist_matches(entry, &host))
        {
            return (FirewallAction::Block, "Global blacklist match".into());
        }

        // 2. Check persona rules, falling back to the ANY_PERSONA ("*") ruleset
        //    when this persona has none.
        for rule in self.persona_rules(persona_id) {
            if Self::match_pattern(&rule.domain_pattern, &host) {
                return (rule.action.clone(), rule.reason.clone());
            }
        }

        // Default: Allow but Audit if no rules match
        (FirewallAction::Audit, "No specific rule matched".into())
    }

    /// Blacklist entries match the named host and any of its subdomains, never
    /// unrelated hosts that merely contain the entry as a substring.
    fn blacklist_matches(entry: &str, host: &str) -> bool {
        let entry = entry
            .trim_start_matches("*.")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if entry.is_empty() {
            return false;
        }
        host == entry || host.ends_with(&format!(".{entry}"))
    }

    fn match_pattern(pattern: &str, host: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        let pattern = pattern.trim_end_matches('.').to_ascii_lowercase();
        if let Some(suffix) = pattern.strip_prefix("*.") {
            // Match the apex domain and any subdomain, but never lookalike
            // domains that merely share the suffix: "*.google.com" must not
            // match "evilgoogle.com".
            return host == suffix || host.ends_with(&format!(".{suffix}"));
        }
        pattern == host
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

    /// Rules that apply to `persona_id`: its own ruleset if one exists,
    /// otherwise the ANY_PERSONA ("*") baseline. Personas unknown to the
    /// policy (e.g. the product browser persona with built-in defaults) are
    /// governed by the baseline instead of silently having no rules.
    pub fn persona_rules(&self, persona_id: &str) -> &[FirewallRule] {
        self.persona_rules
            .get(persona_id)
            .or_else(|| self.persona_rules.get(ANY_PERSONA))
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
    fn wildcard_matches_subdomains_and_apex_only() {
        let firewall = SextantFirewall::new();

        // "*.google.com" Allow rule in the Work persona.
        let apex = Url::parse("https://google.com/").unwrap();
        let sub = Url::parse("https://docs.google.com/").unwrap();
        assert_eq!(
            firewall.check_access("Work", &apex).0,
            FirewallAction::Allow
        );
        assert_eq!(firewall.check_access("Work", &sub).0, FirewallAction::Allow);

        // A lookalike domain sharing the suffix must NOT match the Allow rule;
        // it falls through to the default Audit.
        let lookalike = Url::parse("https://evilgoogle.com/").unwrap();
        assert_eq!(
            firewall.check_access("Work", &lookalike).0,
            FirewallAction::Audit
        );
    }

    #[test]
    fn blacklist_is_not_substring_based() {
        let firewall = SextantFirewall::new();

        // Subdomains of a blacklisted host are blocked...
        let sub = Url::parse("https://cdn.malicious-site.net/x").unwrap();
        assert_eq!(
            firewall.check_access("Personal", &sub).0,
            FirewallAction::Block
        );

        // ...but hosts that merely contain the entry as a substring are not.
        let lookalike = Url::parse("https://verymalicious-site.net.example.com/").unwrap();
        assert_ne!(
            firewall.check_access("Personal", &lookalike).1,
            "Global blacklist match"
        );
    }

    #[test]
    fn ip_literal_hosts_do_not_bypass_rules() {
        let mut firewall = SextantFirewall::new();
        firewall.add_global_blacklist("203.0.113.7");
        firewall.add_rule(
            "browser-persona",
            FirewallRule {
                domain_pattern: "198.51.100.9".to_string(),
                action: FirewallAction::Block,
                reason: "blocked appliance".to_string(),
            },
        );

        let blacklisted = Url::parse("https://203.0.113.7/admin").unwrap();
        assert_eq!(
            firewall.check_access("browser-persona", &blacklisted).0,
            FirewallAction::Block
        );

        let rule_blocked = Url::parse("http://198.51.100.9/").unwrap();
        assert_eq!(
            firewall.check_access("browser-persona", &rule_blocked).0,
            FirewallAction::Block
        );

        // Unlisted IPs still get the default Audit rather than an accidental
        // rule match.
        let unlisted = Url::parse("http://192.0.2.1/").unwrap();
        assert_eq!(
            firewall.check_access("browser-persona", &unlisted).0,
            FirewallAction::Audit
        );
    }

    #[test]
    fn any_persona_baseline_applies_to_unknown_personas() {
        let policy = FirewallPolicy {
            persona_rules: HashMap::from([(
                ANY_PERSONA.to_string(),
                vec![FirewallRule {
                    domain_pattern: "*.internal.example".to_string(),
                    action: FirewallAction::Block,
                    reason: "baseline block".to_string(),
                }],
            )]),
            global_blacklist: Vec::new(),
        };
        let firewall = SextantFirewall::from_policy_overlay(policy);
        let url = Url::parse("https://host.internal.example/").unwrap();

        // browser-persona has no ruleset of its own; the "*" baseline governs.
        let (action, reason) = firewall.check_access("browser-persona", &url);
        assert_eq!(action, FirewallAction::Block);
        assert_eq!(reason, "baseline block");
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
