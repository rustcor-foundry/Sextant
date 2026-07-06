//! # SIMULATED biometric authentication — NOT a real authenticator
//!
//! This crate is a UI/architecture prototype. It performs no OS biometric
//! prompt (Windows Hello / Touch ID / Face ID) and holds no hardware-backed
//! key: the "hardware key" is a compile-time constant embedded in every build,
//! so ANY process can mint a `BioProof` that `verify_proof` accepts.
//!
//! Consequently a `BioProof` proves nothing about the user's presence or
//! identity, and nothing security-critical may rely on it. The vault treats it
//! accordingly: `CitadelVault::unlock_with_bio` can never release the master
//! key of a locked vault.

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

/// SIMULATED biometric proof. Forgeable by any process (the signing key is a
/// public compile-time constant); carries no authentication value.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BioProof {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub method: BioMethod,
    pub hardware_signature: String, // Simulated hardware-backed signature
}

/// SIMULATED biometric authenticator. See the crate-level docs.
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

    /// SIMULATED: no OS biometric prompt occurs; the proof is minted from a
    /// public compile-time constant and is forgeable by any process.
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

    /// SIMULATED: checks only that the proof was minted with the same public
    /// constant key; it does NOT establish user presence or identity.
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
