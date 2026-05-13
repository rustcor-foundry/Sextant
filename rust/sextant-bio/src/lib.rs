use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum BioMethod {
    TouchID,
    FaceID,
    WindowsHello,
    Fingerprint,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BioProof {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub method: BioMethod,
    pub hardware_signature: String, // Simulated hardware-backed signature
}

pub struct SextantBioAuth {
    enrolled_methods: Vec<BioMethod>,
    hardware_key: Vec<u8>,
}

impl SextantBioAuth {
    pub fn new() -> Self {
        Self {
            enrolled_methods: vec![BioMethod::TouchID, BioMethod::FaceID],
            hardware_key: b"simulated-secure-enclave-key-0xDEADBEEF".to_vec(),
        }
    }

    pub fn enroll(&mut self, method: BioMethod) {
        if !self.enrolled_methods.contains(&method) {
            self.enrolled_methods.push(method);
        }
    }

    pub fn authenticate(&self, method: BioMethod) -> Result<BioProof, String> {
        if !self.enrolled_methods.contains(&method) {
            return Err(format!("Method {:?} not enrolled on this device.", method));
        }

        // Simulate hardware interaction (e.g., Secure Enclave / TPM)
        // In a real app, this would call OS-specific biometric APIs
        let timestamp = Utc::now();
        let payload = format!("{:?}-{}", method, timestamp.to_rfc3339());

        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.hardware_key).map_err(|e| e.to_string())?;
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());

        Ok(BioProof {
            id: Uuid::new_v4(),
            timestamp,
            method,
            hardware_signature: signature,
        })
    }

    pub fn verify_proof(&self, proof: &BioProof) -> bool {
        let payload = format!("{:?}-{}", proof.method, proof.timestamp.to_rfc3339());
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(&self.hardware_key) else {
            return false;
        };
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        let expected_signature = hex::encode(result.into_bytes());

        proof.hardware_signature == expected_signature
    }

    pub fn get_available_methods(&self) -> Vec<BioMethod> {
        self.enrolled_methods.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_authentic_proof_and_rejects_tampering() {
        let auth = SextantBioAuth::new();
        let mut proof = auth.authenticate(BioMethod::TouchID).unwrap();

        assert!(auth.verify_proof(&proof));

        proof.hardware_signature.push_str("tamper");
        assert!(!auth.verify_proof(&proof));
    }
}
