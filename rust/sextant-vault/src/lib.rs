use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use bip39::{Language, Mnemonic};
use ed25519_dalek::{
    Signature as EdSignature, Signer as EdSigner, SigningKey as EdSigningKey,
    VerifyingKey as EdVerifyingKey,
};
use hmac::{Hmac, Mac};
use p256::ecdsa::{
    Signature as EcSignature, SigningKey as EcSigningKey, VerifyingKey as EcVerifyingKey,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use zeroize::{Zeroize, ZeroizeOnDrop};
type HmacSha512 = Hmac<Sha512>;
use argon2::{password_hash::SaltString, Argon2};
use sextant_bio::{BioProof, SextantBioAuth};
use sextant_pq::{PqIdentity, SextantPqCore};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum KeyType {
    Ed25519,
    EcdsaP256,
}

/// Represents a logical grouping of identities (e.g., "Personal", "Work").
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Persona {
    pub id: String,
    pub label: String,
    pub description: String,
    pub created_at: u64,
}

/// Represents a cryptographic identity within the Citadel.
#[derive(Serialize, Deserialize, Clone, Zeroize, ZeroizeOnDrop)]
pub struct VaultIdentity {
    pub id: String,
    pub persona_id: String,
    pub label: String,
    pub did: String,
    #[zeroize(skip)]
    pub key_type: KeyType,
    #[serde(default, skip_serializing)]
    pub secret_key: Vec<u8>,
    pub public_key: Vec<u8>,
    pub derivation_path: String,
    pub created_at: u64,
}

/// The secure container for all user identities and secrets.
pub struct CitadelVault {
    personas: HashMap<String, Persona>,
    identities: HashMap<String, VaultIdentity>,
    secrets: HashMap<String, Vec<u8>>,
    master_seed: Option<[u8; 64]>,
    master_key: Option<[u8; 32]>,
    is_locked: bool,
    salt: Option<String>,
    bio_auth: SextantBioAuth,
    pq_core: SextantPqCore,
    pq_identities: HashMap<String, PqIdentity>,
}

impl CitadelVault {
    pub fn new() -> Self {
        Self {
            personas: HashMap::new(),
            identities: HashMap::new(),
            secrets: HashMap::new(),
            master_seed: None,
            master_key: None,
            is_locked: true,
            salt: None,
            bio_auth: SextantBioAuth::new(),
            pq_core: SextantPqCore::new(),
            pq_identities: HashMap::new(),
        }
    }

    /// Initializes a new vault with a fresh mnemonic.
    pub fn initialize_new(&mut self, passphrase: &str) -> Result<String, String> {
        let mnemonic = Mnemonic::generate_in(Language::English, 24).map_err(|e| e.to_string())?;
        let phrase = mnemonic.to_string();

        // We don't store the phrase, we derive the seed immediately
        let seed = mnemonic.to_seed("");
        self.master_seed = Some(seed);

        // Derive master encryption key from passphrase
        let salt = SaltString::generate(&mut OsRng);
        let salt_str = salt.to_string();
        self.salt = Some(salt_str.clone());
        self.unlock(passphrase, &salt_str)?;

        Ok(phrase)
    }

    pub fn unlock(&mut self, passphrase: &str, salt: &str) -> Result<(), String> {
        let salt_obj = SaltString::from_b64(salt).map_err(|e| e.to_string())?;
        let argon2 = Argon2::default();

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(
                passphrase.as_bytes(),
                salt_obj.as_str().as_bytes(),
                &mut key,
            )
            .map_err(|e| e.to_string())?;

        self.master_key = Some(key);
        self.salt = Some(salt.to_string());
        self.is_locked = false;
        Ok(())
    }

    pub fn unlock_with_bio(&mut self, proof: &BioProof) -> Result<(), String> {
        if self.bio_auth.verify_proof(proof) {
            // In a real app, the master key would be stored in the Secure Enclave
            // and released only upon successful biometric authentication.
            // Here we simulate this by using a "hardware-backed" key if it was previously set.
            if self.master_key.is_some() {
                self.is_locked = false;
                Ok(())
            } else {
                Err(
                    "Vault must be initialized with a passphrase before Bio-Auth can be used."
                        .into(),
                )
            }
        } else {
            Err("Biometric verification failed.".into())
        }
    }

    pub fn lock(&mut self) {
        if let Some(mut key) = self.master_key.take() {
            key.zeroize();
        }
        if let Some(mut seed) = self.master_seed.take() {
            seed.zeroize();
        }
        self.is_locked = true;
    }

    pub fn is_locked(&self) -> bool {
        self.is_locked
    }

    pub fn create_persona(&mut self, label: &str, description: &str) -> Result<Persona, String> {
        if self.is_locked {
            return Err("Vault is locked.".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let persona = Persona {
            id: id.clone(),
            label: label.to_string(),
            description: description.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        self.personas.insert(id, persona.clone());
        Ok(persona)
    }

    pub fn get_personas(&self) -> Vec<Persona> {
        self.personas.values().cloned().collect()
    }

    pub fn get_identities(&self, persona_id: &str) -> Vec<VaultIdentity> {
        self.identities
            .values()
            .filter(|i| i.persona_id == persona_id)
            .cloned()
            .collect()
    }

    pub fn upgrade_persona_to_pq(&mut self, persona_id: &str) -> Result<PqIdentity, String> {
        if self.is_locked {
            return Err("Vault is locked.".into());
        }
        if !self.personas.contains_key(persona_id) {
            return Err(format!("Persona {} not found", persona_id));
        }

        let pq_identity = self.pq_core.generate_identity();
        self.pq_identities
            .insert(persona_id.to_string(), pq_identity.clone());
        Ok(pq_identity)
    }

    pub fn get_pq_identity(&self, persona_id: &str) -> Option<PqIdentity> {
        self.pq_identities.get(persona_id).cloned()
    }

    pub fn sign_with_pq(&self, persona_id: &str, message: &[u8]) -> Result<String, String> {
        if self.is_locked {
            return Err("Vault is locked.".into());
        }
        if let Some(pq_identity) = self.pq_identities.get(persona_id) {
            Ok(self.pq_core.sign_pq(pq_identity, message))
        } else {
            Err("No PQ identity found for this persona.".into())
        }
    }

    /// Derives a new identity from the master seed using a derivation path.
    /// Path format: m/purpose'/coin_type'/account'/change/address_index
    pub fn derive_identity(
        &mut self,
        persona_id: &str,
        label: &str,
        key_type: KeyType,
        path: &str,
    ) -> Result<VaultIdentity, String> {
        if self.is_locked || self.master_seed.is_none() {
            return Err("Vault is locked or seed not initialized.".into());
        }

        if !self.personas.contains_key(persona_id) {
            return Err(format!("Persona {} not found", persona_id));
        }

        let seed_bytes = self.master_seed.as_ref().unwrap();

        // Persona-aware derivation
        // We use the persona_id as a salt to ensure identities are isolated between personas
        let mut mac = <HmacSha512 as Mac>::new_from_slice(seed_bytes).map_err(|e| e.to_string())?;
        mac.update(persona_id.as_bytes());
        mac.update(b"/");
        mac.update(path.as_bytes());
        let derived_bytes = mac.finalize().into_bytes();

        let id = uuid::Uuid::new_v4().to_string();
        let (secret_key, public_key, did) = match key_type {
            KeyType::Ed25519 => {
                let signing_key = EdSigningKey::from_bytes(derived_bytes[..32].try_into().unwrap());
                let verifying_key = EdVerifyingKey::from(&signing_key);
                let pk_bytes = verifying_key.to_bytes().to_vec();

                // Multicodec prefix for Ed25519 is 0xed 0x01
                let mut multicodec = vec![0xed, 0x01];
                multicodec.extend_from_slice(&pk_bytes);
                let encoded = bs58::encode(multicodec)
                    .with_alphabet(bs58::Alphabet::BITCOIN)
                    .into_string();
                let did = format!("did:key:z{}", encoded);

                (signing_key.to_bytes().to_vec(), pk_bytes, did)
            }
            KeyType::EcdsaP256 => {
                let signing_key = EcSigningKey::from_bytes((&derived_bytes[..32]).into())
                    .map_err(|e| e.to_string())?;
                let verifying_key = EcVerifyingKey::from(&signing_key);
                let pk_bytes = verifying_key.to_encoded_point(true).as_bytes().to_vec();

                // Multicodec prefix for P-256 is 0x1200
                let mut multicodec = vec![0x12, 0x00];
                multicodec.extend_from_slice(&pk_bytes);
                let encoded = bs58::encode(multicodec)
                    .with_alphabet(bs58::Alphabet::BITCOIN)
                    .into_string();
                let did = format!("did:key:z{}", encoded);

                (signing_key.to_bytes().to_vec(), pk_bytes, did)
            }
        };

        let identity = VaultIdentity {
            id: id.clone(),
            persona_id: persona_id.to_string(),
            label: label.to_string(),
            did,
            key_type,
            secret_key,
            public_key,
            derivation_path: path.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        self.identities.insert(id, identity.clone());
        Ok(identity)
    }

    /// Signs a message using a specific identity.
    pub fn sign_with_identity(&self, id: &str, message: &[u8]) -> Result<Vec<u8>, String> {
        if self.is_locked {
            return Err("Vault is locked.".into());
        }

        let identity = self
            .identities
            .get(id)
            .ok_or_else(|| format!("Identity {} not found", id))?;

        match identity.key_type {
            KeyType::Ed25519 => {
                let signing_key = EdSigningKey::from_bytes(
                    identity
                        .secret_key
                        .as_slice()
                        .try_into()
                        .map_err(|_| "Invalid key length")?,
                );
                let signature: EdSignature = signing_key.sign(message);
                Ok(signature.to_bytes().to_vec())
            }
            KeyType::EcdsaP256 => {
                let signing_key = EcSigningKey::from_bytes(identity.secret_key.as_slice().into())
                    .map_err(|e| e.to_string())?;
                let signature: EcSignature = signing_key.sign(message);
                Ok(signature.to_der().as_bytes().to_vec())
            }
        }
    }

    pub fn encrypt_data(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let key_bytes = self.master_key.ok_or("Vault not unlocked")?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;

        let mut nonce_bytes = [0u8; 12];
        rand::RngCore::fill_bytes(&mut OsRng, &mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| e.to_string())?;

        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);
        Ok(result)
    }

    pub fn decrypt_data(&self, encrypted_data: &[u8]) -> Result<Vec<u8>, String> {
        let key_bytes = self.master_key.ok_or("Vault not unlocked")?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;

        if encrypted_data.len() < 12 {
            return Err("Invalid encrypted data format".into());
        }

        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        cipher.decrypt(nonce, ciphertext).map_err(|e| e.to_string())
    }

    pub fn store_secret(&mut self, key: &str, value: &str) -> Result<(), String> {
        if self.is_locked {
            return Err("Vault is locked".into());
        }
        let encrypted = self.encrypt_data(value.as_bytes())?;
        self.secrets.insert(key.to_string(), encrypted);
        Ok(())
    }

    pub fn get_secret(&self, key: &str) -> Result<String, String> {
        if self.is_locked {
            return Err("Vault is locked".into());
        }
        let encrypted = self
            .secrets
            .get(key)
            .ok_or_else(|| format!("Secret '{}' not found", key))?;
        let decrypted = self.decrypt_data(encrypted)?;
        String::from_utf8(decrypted).map_err(|e| e.to_string())
    }

    /// Persists the encrypted vault state to a JSON string.
    /// Note: Master key and seed are NEVER persisted.
    pub fn export_encrypted(&self) -> Result<String, String> {
        #[derive(Serialize)]
        struct Export {
            personas: HashMap<String, Persona>,
            identities: HashMap<String, VaultIdentity>,
            // Identity private keys, encrypted with the master key (the same way
            // `secrets` are). `VaultIdentity::secret_key` is `skip_serializing`, so
            // it is never written in plaintext; persist it here encrypted so an
            // exported/synced vault can still sign after import instead of silently
            // restoring empty, unusable keys.
            secret_keys: HashMap<String, Vec<u8>>,
            secrets: HashMap<String, Vec<u8>>,
            salt: Option<String>,
        }

        let mut secret_keys = HashMap::new();
        for (id, identity) in &self.identities {
            if !identity.secret_key.is_empty() {
                secret_keys.insert(id.clone(), self.encrypt_data(&identity.secret_key)?);
            }
        }

        let export = Export {
            personas: self.personas.clone(),
            identities: self.identities.clone(),
            secret_keys,
            secrets: self.secrets.clone(),
            salt: self.salt.clone(),
        };

        serde_json::to_string(&export).map_err(|e| e.to_string())
    }

    pub fn import_encrypted(&mut self, data: &str) -> Result<(), String> {
        #[derive(Deserialize)]
        struct Import {
            personas: HashMap<String, Persona>,
            identities: HashMap<String, VaultIdentity>,
            #[serde(default)]
            secret_keys: HashMap<String, Vec<u8>>,
            secrets: HashMap<String, Vec<u8>>,
            salt: Option<String>,
        }

        let import: Import = serde_json::from_str(data).map_err(|e| e.to_string())?;
        let mut identities = import.identities;
        // Restore the encrypted identity private keys (skipped on serialize) so
        // imported identities can sign. Requires the vault be unlocked with the
        // master key the keys were encrypted under.
        for (id, encrypted) in &import.secret_keys {
            if let Some(identity) = identities.get_mut(id) {
                identity.secret_key = self.decrypt_data(encrypted)?;
            }
        }
        self.personas = import.personas;
        self.identities = identities;
        self.secrets = import.secrets;
        self.salt = import.salt;
        Ok(())
    }

    /// Produces a local, tamper-evident consent token ("Captain's Key"): an
    /// HMAC-SHA512 over the message keyed by a dedicated, domain-separated key
    /// derived from the master seed. Requires the vault to be unlocked.
    ///
    /// Note: this is a symmetric MAC, not a publicly verifiable signature — only a
    /// holder of the seed can produce or check it. Use `sign_with_identity` /
    /// `sign_with_pq` when third-party verifiability is required.
    pub fn sign_consent(&self, message: &str) -> Result<String, String> {
        if !self.is_locked {
            if let Some(seed) = self.master_seed {
                // Derive a dedicated consent key rather than keying the HMAC
                // directly on the HD derivation root (avoids reusing the seed
                // across unrelated purposes).
                let consent_key = {
                    let mut kdf =
                        <HmacSha512 as Mac>::new_from_slice(&seed).map_err(|e| e.to_string())?;
                    kdf.update(b"sextant/consent-key/v1");
                    kdf.finalize().into_bytes()
                };
                let mut hmac =
                    <HmacSha512 as Mac>::new_from_slice(&consent_key).map_err(|e| e.to_string())?;
                hmac.update(message.as_bytes());
                hmac.update(b"CONSENT_DOMAIN_SEPARATOR");
                let result = hmac.finalize().into_bytes();
                Ok(hex::encode(result))
            } else {
                Err("Vault identity not initialized with a seed.".into())
            }
        } else {
            Err("Vault is locked. Captain's Key unavailable.".into())
        }
    }
}

impl Drop for CitadelVault {
    /// Wipe the master key and seed from memory when the vault is dropped, not
    /// only on an explicit `lock()`. Without this, an early return, panic unwind,
    /// or a short-lived vault leaves raw key material in freed memory.
    fn drop(&mut self) {
        if let Some(mut key) = self.master_key.take() {
            key.zeroize();
        }
        if let Some(mut seed) = self.master_seed.take() {
            seed.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier as EdVerifierTrait;

    #[test]
    fn test_multi_curve_vault() {
        let mut vault = CitadelVault::new();
        vault.initialize_new("password123").unwrap();

        let persona = vault.create_persona("Default", "Primary persona").unwrap();

        // Test Ed25519
        let ed_identity = vault
            .derive_identity(&persona.id, "Ed Pilot", KeyType::Ed25519, "m/44'/0'/0'/0/0")
            .unwrap();
        let msg = b"Sextant Ed25519 Test";
        let sig = vault.sign_with_identity(&ed_identity.id, msg).unwrap();
        let pk = EdVerifyingKey::from_bytes(ed_identity.public_key.as_slice().try_into().unwrap())
            .unwrap();
        assert!(pk
            .verify(
                msg,
                &EdSignature::from_bytes(sig.as_slice().try_into().unwrap())
            )
            .is_ok());

        // Test ECDSA P-256
        let ec_identity = vault
            .derive_identity(
                &persona.id,
                "Ec Pilot",
                KeyType::EcdsaP256,
                "m/44'/0'/0'/0/1",
            )
            .unwrap();
        let sig_ec = vault.sign_with_identity(&ec_identity.id, msg).unwrap();
        let pk_ec = EcVerifyingKey::from_encoded_point(
            &p256::EncodedPoint::from_bytes(&ec_identity.public_key).unwrap(),
        )
        .unwrap();
        let signature_ec = EcSignature::from_der(&sig_ec).unwrap();
        assert!(pk_ec.verify(msg, &signature_ec).is_ok());
    }

    #[test]
    fn export_import_round_trip_preserves_identity_signing() {
        let mut vault = CitadelVault::new();
        vault.initialize_new("password123").unwrap();
        let persona = vault.create_persona("Default", "Primary").unwrap();
        let identity = vault
            .derive_identity(&persona.id, "Main", KeyType::Ed25519, "m/44'/0'/0'/0/0")
            .unwrap();

        let msg = b"Sextant export round-trip";
        let sig_before = vault.sign_with_identity(&identity.id, msg).unwrap();

        // Export and re-import into the same (still unlocked) vault. Without the
        // encrypted secret-key round-trip, the imported identity loses its private
        // key and `sign_with_identity` fails with "Invalid key length".
        let exported = vault.export_encrypted().unwrap();
        vault.import_encrypted(&exported).unwrap();

        let sig_after = vault
            .sign_with_identity(&identity.id, msg)
            .expect("imported identity should still sign");
        // Ed25519 is deterministic, so a preserved key reproduces the signature.
        assert_eq!(sig_before, sig_after);
    }
}
