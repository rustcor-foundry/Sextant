//! # SIMULATED post-quantum crypto — NOT real cryptography
//!
//! This crate is a UI/architecture prototype. It does NOT implement ML-DSA,
//! Falcon, SPHINCS+, or any other post-quantum scheme, and it provides NO
//! cryptographic security whatsoever:
//!
//! - "Keys" are SHA3 hashes of a random UUID; there is no private key.
//! - "Signatures" are a SHA3 hash of the *public* identity plus the message,
//!   so anyone holding the public identity can forge a valid "signature".
//! - `verify_pq` simply recomputes that hash.
//!
//! Nothing security-critical may rely on this crate. It exists so the product
//! surfaces (persona "PQ upgrade" panels) can be exercised before a real,
//! audited ML-DSA implementation is wired in. All produced artifacts are
//! prefixed `simulated-` to make their nature visible in logs and UIs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum PqAlgorithm {
    /// Label only — the simulation does not implement ML-DSA.
    Dilithium2,
    Falcon512,
    SphincsPlus,
}

/// SIMULATED post-quantum identity. See the crate-level docs: this has no
/// private key and grants no security.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PqIdentity {
    pub id: Uuid,
    pub algorithm: PqAlgorithm,
    pub public_key_pq: String, // Simulated PQ public key (hash, not a real key)
    pub created_at: DateTime<Utc>,
}

/// SIMULATED post-quantum signer/verifier. See the crate-level docs.
pub struct SextantPqCore {
    active_algorithm: PqAlgorithm,
}

impl SextantPqCore {
    pub fn new() -> Self {
        Self {
            active_algorithm: PqAlgorithm::Dilithium2,
        }
    }

    /// SIMULATED: generates a hash-based placeholder identity, not a keypair.
    pub fn generate_identity(&self) -> PqIdentity {
        let id = Uuid::new_v4();
        let mut hasher = Sha3_256::new();
        hasher.update(id.as_bytes());
        hasher.update(b"PQ_KEY_GEN_SALT");
        let hash = hasher.finalize();

        PqIdentity {
            id,
            algorithm: self.active_algorithm.clone(),
            public_key_pq: format!("simulated-pq-pk-{}", hex::encode(hash)),
            created_at: Utc::now(),
        }
    }

    /// SIMULATED: hashes the *public* identity and message. Forgeable by any
    /// holder of the public identity; carries no authentication value.
    pub fn sign_pq(&self, identity: &PqIdentity, message: &[u8]) -> String {
        let mut hasher = Sha3_256::new();
        hasher.update(identity.public_key_pq.as_bytes());
        hasher.update(message);
        hasher.update(b"PQ_SIG_DOMAIN_SEPARATOR");
        let hash = hasher.finalize();

        format!("simulated-pq-sig-{}", hex::encode(hash))
    }

    /// SIMULATED: recomputes the forgeable hash; proves nothing about who
    /// produced the signature.
    pub fn verify_pq(&self, identity: &PqIdentity, message: &[u8], signature: &str) -> bool {
        let expected = self.sign_pq(identity, message);
        signature == expected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_artifacts_are_labelled_as_simulated() {
        let core = SextantPqCore::new();
        let identity = core.generate_identity();
        assert!(identity.public_key_pq.starts_with("simulated-pq-pk-"));

        let signature = core.sign_pq(&identity, b"message");
        assert!(signature.starts_with("simulated-pq-sig-"));
        assert!(core.verify_pq(&identity, b"message", &signature));
        assert!(!core.verify_pq(&identity, b"other", &signature));
    }

    #[test]
    fn simulated_signatures_are_forgeable_by_public_identity_holders() {
        // Documents the simulation's core limitation: signing requires only
        // the PUBLIC identity, so verification carries no authenticity.
        let core = SextantPqCore::new();
        let identity = core.generate_identity();
        let public_only = identity.clone();
        let forged = core.sign_pq(&public_only, b"attacker message");
        assert!(core.verify_pq(&identity, b"attacker message", &forged));
    }
}
