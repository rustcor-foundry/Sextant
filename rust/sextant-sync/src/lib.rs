//! # Cross-device persona sync — PROTOTYPE
//!
//! Payloads are now signed with the persona's Ed25519 identity key and
//! verified against the public key embedded in the payload's `did:key` DID, so
//! any holder of a payload can check that it was produced by the DID's private
//! key holder (previously "verification" recomputed the local vault's HMAC,
//! which only ever matched payloads minted by the same seed and which any
//! local holder could forge).
//!
//! What this module does NOT yet provide:
//! - Transport: there is no networking; payloads move out-of-band.
//! - Pairing/authorization: `apply_sync_payload` requires the signing DID to
//!   already exist in the local vault (devices provisioned from the same
//!   mnemonic). There is no cross-device key exchange or trust ceremony.
//! - Merging: applying a payload wholesale replaces local vault contents.

use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature as EdSignature, Verifier, VerifyingKey as EdVerifyingKey};
use serde::{Deserialize, Serialize};
use sextant_vault::CitadelVault;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncPayload {
    pub device_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub encrypted_data: String, // Base64 encoded encrypted vault export
    pub signature: String,      // Base64 Ed25519 signature by the persona identity
    pub did: String,            // did:key DID whose key signed this payload
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

    /// Prepares an encrypted sync payload for a specific persona, signed with
    /// the persona's Ed25519 identity so receivers can verify authenticity
    /// against the DID embedded in the payload.
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

        // 2. Pick an Ed25519 identity for this persona to sign the payload
        //    (did:key verification below currently supports Ed25519).
        let identities = vault.get_identities(persona_id);
        let identity = identities
            .iter()
            .find(|identity| identity.did.starts_with(ED25519_DID_PREFIX))
            .ok_or(SyncError::IdentityMismatch)?;

        // 3. Create the payload
        let timestamp = Utc::now();
        let message = format!(
            "{}:{}:{}",
            self.device_id,
            timestamp.to_rfc3339(),
            encrypted_data
        );

        // 4. Sign with the identity's private key: a publicly verifiable
        //    signature, unlike the seed-keyed consent MAC.
        let signature_bytes = vault
            .sign_with_identity(&identity.id, message.as_bytes())
            .map_err(|_| SyncError::InvalidSignature)?;

        Ok(SyncPayload {
            device_id: self.device_id,
            timestamp,
            encrypted_data: base64::engine::general_purpose::STANDARD.encode(encrypted_data),
            signature: base64::engine::general_purpose::STANDARD.encode(signature_bytes),
            did: identity.did.clone(),
        })
    }

    /// Processes an incoming sync payload and applies it to the local vault.
    ///
    /// The signature is verified against the public key in the payload's DID,
    /// and the DID must already be known to the local vault (same-mnemonic
    /// provisioning) before any state is replaced.
    pub fn apply_sync_payload(
        &self,
        vault: &mut CitadelVault,
        payload: SyncPayload,
    ) -> Result<(), SyncError> {
        if vault.is_locked() {
            return Err(SyncError::VaultLocked);
        }

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

        // 1. Verify the Ed25519 signature against the DID's public key.
        let verifying_key = ed25519_key_from_did(&payload.did)?;
        let signature_bytes = base64::engine::general_purpose::STANDARD
            .decode(&payload.signature)
            .map_err(|_| SyncError::InvalidSignature)?;
        let signature =
            EdSignature::from_slice(&signature_bytes).map_err(|_| SyncError::InvalidSignature)?;
        verifying_key
            .verify(message.as_bytes(), &signature)
            .map_err(|_| SyncError::InvalidSignature)?;

        // 2. Authorization floor: only accept payloads signed by an identity
        //    this vault already knows. Any valid keypair can produce a
        //    verifiable payload; without this check an attacker could replace
        //    the whole local vault with their own signed state.
        let known_did = vault
            .get_personas()
            .iter()
            .flat_map(|persona| vault.get_identities(&persona.id))
            .any(|identity| identity.did == payload.did);
        if !known_did {
            return Err(SyncError::IdentityMismatch);
        }

        // 3. Import the encrypted data into the vault
        vault
            .import_encrypted(&encrypted_str)
            .map_err(|_| SyncError::DecryptionFailed)?;

        Ok(())
    }
}

/// `did:key` prefix produced for Ed25519 public keys (multicodec 0xed 0x01).
const ED25519_DID_PREFIX: &str = "did:key:z";

/// Extracts the Ed25519 verifying key from a `did:key:z...` DID.
fn ed25519_key_from_did(did: &str) -> Result<EdVerifyingKey, SyncError> {
    let encoded = did
        .strip_prefix(ED25519_DID_PREFIX)
        .ok_or(SyncError::InvalidSignature)?;
    let multicodec = bs58::decode(encoded)
        .with_alphabet(bs58::Alphabet::BITCOIN)
        .into_vec()
        .map_err(|_| SyncError::InvalidSignature)?;
    // Multicodec prefix for Ed25519 public keys is 0xed 0x01.
    let key_bytes = multicodec
        .strip_prefix(&[0xed, 0x01][..])
        .ok_or(SyncError::InvalidSignature)?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| SyncError::InvalidSignature)?;
    EdVerifyingKey::from_bytes(&key_array).map_err(|_| SyncError::InvalidSignature)
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

    #[test]
    fn tampered_payload_data_is_rejected() {
        let (mut vault, persona_id) = unlocked_vault_with_identity();
        let sync = SextantSync::new();
        let mut payload = sync
            .prepare_sync_payload(&vault, &persona_id)
            .expect("payload should be prepared");
        // Alter the encrypted body after signing; the signature no longer covers it.
        let mut data = base64::engine::general_purpose::STANDARD
            .decode(&payload.encrypted_data)
            .unwrap();
        data.push(b' ');
        payload.encrypted_data = base64::engine::general_purpose::STANDARD.encode(data);

        let result = sync.apply_sync_payload(&mut vault, payload);

        assert_eq!(result, Err(SyncError::InvalidSignature));
    }

    #[test]
    fn payload_from_unknown_identity_is_rejected() {
        // A payload signed by a different vault verifies against its own DID,
        // but must not be allowed to replace this vault's state.
        let (mut local_vault, _) = unlocked_vault_with_identity();
        let (foreign_vault, foreign_persona) = unlocked_vault_with_identity();
        let sync = SextantSync::new();
        let payload = sync
            .prepare_sync_payload(&foreign_vault, &foreign_persona)
            .expect("payload should be prepared");

        let result = sync.apply_sync_payload(&mut local_vault, payload);

        assert_eq!(result, Err(SyncError::IdentityMismatch));
    }
}
