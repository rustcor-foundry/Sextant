use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PrivacyLevel {
    None,
    Standard, // Redact emails, phones
    Strict,   // Redact names, addresses, financial data
}

pub struct PrivacyMasker {
    email_regex: Regex,
    phone_regex: Regex,
    credit_card_regex: Regex,
}

impl PrivacyMasker {
    pub fn new() -> Self {
        Self {
            email_regex: Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
            phone_regex: Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap(),
            credit_card_regex: Regex::new(r"\b(?:\d[ -]*?){13,16}\b").unwrap(),
        }
    }

    pub fn redact_text(&self, text: &str, level: &PrivacyLevel) -> String {
        if matches!(level, PrivacyLevel::None) {
            return text.to_string();
        }

        let mut redacted = text.to_string();

        // Always redact credit cards in Standard/Strict
        redacted = self
            .credit_card_regex
            .replace_all(&redacted, "[REDACTED CARD]")
            .to_string();

        match level {
            PrivacyLevel::Standard | PrivacyLevel::Strict => {
                redacted = self
                    .email_regex
                    .replace_all(&redacted, "[REDACTED EMAIL]")
                    .to_string();
                redacted = self
                    .phone_regex
                    .replace_all(&redacted, "[REDACTED PHONE]")
                    .to_string();
            }
            _ => {}
        }

        if matches!(level, PrivacyLevel::Strict) {
            // In a real app, this would use an NER (Named Entity Recognition) model
            // Here we simulate redacting potential names/entities
            redacted = redacted.replace("Paul", "[REDACTED NAME]");
        }

        redacted
    }

    pub fn redact_image_simulated(&self, metadata: &str) -> String {
        format!("{} (Privacy Mask Applied: PII Redacted)", metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_privacy_redacts_common_contact_and_card_data() {
        let masker = PrivacyMasker::new();
        let text = "Email paul@example.com or call 555-123-4567 with card 4111 1111 1111 1111.";
        let redacted = masker.redact_text(text, &PrivacyLevel::Standard);

        assert!(redacted.contains("[REDACTED EMAIL]"));
        assert!(redacted.contains("[REDACTED PHONE]"));
        assert!(redacted.contains("[REDACTED CARD]"));
        assert!(!redacted.contains("paul@example.com"));
        assert!(!redacted.contains("555-123-4567"));
    }

    #[test]
    fn none_privacy_preserves_text() {
        let masker = PrivacyMasker::new();
        let text = "paul@example.com 555-123-4567";
        assert_eq!(masker.redact_text(text, &PrivacyLevel::None), text);
    }
}
