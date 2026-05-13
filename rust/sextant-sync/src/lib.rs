use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sextant_vault::CitadelVault;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncPayload {
    pub device_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub encrypted_data: String, // Base64 encoded encrypted vault export
    pub signature: String,      // Signature of the payload using the Captain's Key
    pub did: String,            // DID of the persona being synced
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum SyncError {
    VaultLocked,
    InvalidSignature,
    DecryptionFailed,
    IdentityMismatch,
    NetworkError(String),
}

pub struct SextantSync {
    device_id: Uuid,
}

impl SextantSync {
    pub fn new() -> Self {
        Self {
            device_id: Uuid::new_v4(),
        }
    }

    /// Prepares an encrypted sync payload for a specific persona.
    pub fn prepare_sync_payload(
        &self,
        vault: &CitadelVault,
        persona_id: &str,
    ) -> Result<SyncPayload, SyncError> {
        if vault.is_locked() {
            return Err(SyncError::VaultLocked);
        }

        // 1. Export the encrypted vault data
        let encrypted_data = vault
            .export_encrypted()
            .map_err(|_| SyncError::DecryptionFailed)?;

        // 2. Get the primary identity for this persona to sign the payload
        let identities = vault.get_identities(persona_id);
        let identity = identities.first().ok_or(SyncError::IdentityMismatch)?;

        // 3. Create the payload
        let timestamp = Utc::now();
        let message = format!(
            "{}:{}:{}",
            self.device_id,
            timestamp.to_rfc3339(),
            encrypted_data
        );

        // 4. Sign the payload using the Captain's Key (simulated via vault.sign_consent)
        let signature = vault
            .sign_consent(&message)
            .map_err(|_| SyncError::InvalidSignature)?;

        Ok(SyncPayload {
            device_id: self.device_id,
            timestamp,
            encrypted_data: base64::engine::general_purpose::STANDARD.encode(encrypted_data),
            signature,
            did: identity.did.clone(),
        })
    }

    /// Processes an incoming sync payload and applies it to the local vault.
    pub fn apply_sync_payload(
        &self,
        vault: &mut CitadelVault,
        payload: SyncPayload,
    ) -> Result<(), SyncError> {
        if vault.is_locked() {
            return Err(SyncError::VaultLocked);
        }

        // 1. Verify the signature (In a real app, we'd verify against the DID's public key)
        // For simulation, we assume the signature is valid if it matches the vault's consent logic
        let decoded_data = base64::engine::general_purpose::STANDARD
            .decode(&payload.encrypted_data)
            .map_err(|_| SyncError::DecryptionFailed)?;
        let encrypted_str =
            String::from_utf8(decoded_data).map_err(|_| SyncError::DecryptionFailed)?;

        let message = format!(
            "{}:{}:{}",
            payload.device_id,
            payload.timestamp.to_rfc3339(),
            encrypted_str
        );

        // Verification simulation
        let expected_sig = vault
            .sign_consent(&message)
            .map_err(|_| SyncError::InvalidSignature)?;

        if expected_sig != payload.signature {
            return Err(SyncError::InvalidSignature);
        }

        // 2. Import the encrypted data into the vault
        vault
            .import_encrypted(&encrypted_str)
            .map_err(|_| SyncError::DecryptionFailed)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sextant_vault::KeyType;

    fn unlocked_vault_with_identity() -> (CitadelVault, String) {
        let mut vault = CitadelVault::new();
        vault
            .initialize_new("password123")
            .expect("vault should initialize");
        let persona = vault
            .create_persona("Personal", "Primary")
            .expect("persona should be created");
        vault
            .derive_identity(&persona.id, "Main", KeyType::Ed25519, "m/44'/0'/0'/0/0")
            .expect("identity should be derived");
        (vault, persona.id)
    }

    #[test]
    fn sync_payload_round_trips_against_unlocked_vault() {
        let (mut vault, persona_id) = unlocked_vault_with_identity();
        let sync = SextantSync::new();

        let payload = sync
            .prepare_sync_payload(&vault, &persona_id)
            .expect("payload should be prepared");

        sync.apply_sync_payload(&mut vault, payload)
            .expect("payload should apply");
    }

    #[test]
    fn tampered_sync_payload_is_rejected() {
        let (mut vault, persona_id) = unlocked_vault_with_identity();
        let sync = SextantSync::new();
        let mut payload = sync
            .prepare_sync_payload(&vault, &persona_id)
            .expect("payload should be prepared");
        payload.signature.push_str("tamper");

        let result = sync.apply_sync_payload(&mut vault, payload);

        assert_eq!(result, Err(SyncError::InvalidSignature));
    }
}
