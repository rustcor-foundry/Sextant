#![allow(dead_code)]

use crate::state::SextantState;
use sextant_bio::BioMethod;
use sextant_mesh::{MeshPayload, SextantMesh};
use sextant_pq::PqIdentity;
use sextant_sync::{SextantSync, SyncPayload};
use sextant_vault::{Persona, VaultIdentity};

pub struct DeferredState {
    pub active_pq_identity: Option<PqIdentity>,
    pub identities: Vec<VaultIdentity>,
    pub mesh: SextantMesh,
    pub mesh_input: String,
    pub available_personas: Vec<Persona>,
    pub sync_manager: SextantSync,
}

impl DeferredState {
    pub fn new(
        active_did: String,
        identities: Vec<VaultIdentity>,
        available_personas: Vec<Persona>,
    ) -> Self {
        Self {
            active_pq_identity: None,
            identities,
            mesh: SextantMesh::new(active_did),
            mesh_input: String::new(),
            available_personas,
            sync_manager: SextantSync::new(),
        }
    }
}

impl SextantState {
    pub async fn export_sync_payload(&mut self) {
        if let Some(persona_id) = self.active_persona.as_ref().map(|p| p.id.clone()) {
            let sync_result = {
                let vault = self.vault.lock().await;
                self.deferred
                    .sync_manager
                    .prepare_sync_payload(&vault, &persona_id)
                    .map(|p| serde_json::to_string_pretty(&p).unwrap_or_default())
            };
            match sync_result {
                Ok(json) => {
                    self.add_log("Sync payload generated. Ready for cross-device transfer.");
                    println!("SYNC PAYLOAD:\n{}", json);
                }
                Err(e) => self.add_log(&format!("Sync export failed: {:?}", e)),
            }
        }
    }

    pub async fn import_sync_payload(&mut self, payload_json: &str) {
        match serde_json::from_str::<SyncPayload>(payload_json) {
            Ok(payload) => {
                let (result, personas) = {
                    let mut vault = self.vault.lock().await;
                    let result = self
                        .deferred
                        .sync_manager
                        .apply_sync_payload(&mut vault, payload);
                    let personas = if result.is_ok() {
                        Some(vault.get_personas())
                    } else {
                        None
                    };
                    (result, personas)
                };
                match result {
                    Ok(_) => {
                        if let Some(personas) = personas {
                            self.deferred.available_personas = personas;
                        }
                        self.add_log("Sync payload applied. Persona context synchronized.");
                    }
                    Err(e) => self.add_log(&format!("Sync import failed: {:?}", e)),
                }
            }
            Err(_) => self.add_log("Invalid sync payload format."),
        }
    }

    pub async fn unlock_with_bio(&mut self) {
        self.add_log("Initiating biometric authentication...");
        let bio_auth = sextant_bio::SextantBioAuth::new();
        if let Ok(proof) = bio_auth.authenticate(BioMethod::TouchID) {
            let result = {
                let mut vault = self.vault.lock().await;
                vault.unlock_with_bio(&proof)
            };
            match result {
                Ok(_) => {
                    self.is_vault_unlocked = true;
                    self.add_log("Vault unlocked via Bio-Auth (TouchID).");
                }
                Err(e) => self.add_log(&format!("Bio-Auth failed: {}", e)),
            }
        }
    }

    pub async fn switch_persona(&mut self, persona: Persona) {
        self.add_log(&format!("Switching to persona: {}", persona.label));
        self.active_persona = Some(persona.clone());
        {
            let mut pilot = self.pilot.lock().await;
            pilot.set_persona(Some(persona.id.clone()));
        }

        let (active_did, pq_identity) = {
            let vault = self.vault.lock().await;
            let did = vault
                .get_identities(&persona.id)
                .first()
                .map(|i| i.did.clone())
                .unwrap_or_else(|| "No Identity".to_string());
            let pq = vault.get_pq_identity(&persona.id);
            (did, pq)
        };
        self.active_did = active_did;
        self.deferred.active_pq_identity = pq_identity;
        self.refresh_memory().await;
        self.refresh_pilot_snapshot().await;
    }

    pub async fn upgrade_to_pq(&mut self) {
        if let Some(persona_id) = self.active_persona.as_ref().map(|p| p.id.clone()) {
            let result = {
                let mut vault = self.vault.lock().await;
                vault.upgrade_persona_to_pq(&persona_id)
            };
            match result {
                Ok(pq_id) => {
                    self.deferred.active_pq_identity = Some(pq_id);
                    self.add_log("Identity Upgraded: Post-Quantum Protection Active (ML-DSA).");
                }
                Err(e) => self.add_log(&format!("Upgrade Failed: {}", e)),
            }
        }
    }

    pub async fn send_mesh_chat(&mut self) {
        let text = self.deferred.mesh_input.clone();
        if text.is_empty() {
            return;
        }

        let pq_sig = if let Some(persona) = &self.active_persona {
            let vault = self.vault.lock().await;
            vault.sign_with_pq(&persona.id, text.as_bytes()).ok()
        } else {
            None
        };

        self.deferred
            .mesh
            .broadcast(MeshPayload::ChatMessage(text), pq_sig);
        self.deferred.mesh_input.clear();
    }

    pub async fn share_active_tab(&mut self) {
        if let Some(tab) = self.tabs.first() {
            if let Some(url) = &tab.url {
                let payload = MeshPayload::TabShare {
                    url: url.to_string(),
                    title: "Shared Tab".into(),
                };

                let pq_sig = if let Some(persona) = &self.active_persona {
                    let vault = self.vault.lock().await;
                    vault
                        .sign_with_pq(&persona.id, url.as_str().as_bytes())
                        .ok()
                } else {
                    None
                };

                self.deferred.mesh.broadcast(payload, pq_sig);
                self.add_log(&format!("Shared tab: {}", url));
            }
        }
    }

    pub async fn share_file(&mut self, name: &str) {
        let payload = MeshPayload::FileShare {
            name: name.to_string(),
            size: 1024,
            type_id: "text/plain".into(),
            content_base64: "U2V4dGFudCBNZXNoIEZpbGUgU2hhcmUgVGVzdA==".into(),
        };

        let pq_sig = if let Some(persona) = &self.active_persona {
            let vault = self.vault.lock().await;
            vault.sign_with_pq(&persona.id, name.as_bytes()).ok()
        } else {
            None
        };

        self.deferred.mesh.broadcast(payload, pq_sig);
        self.add_log(&format!("Shared file via mesh: {}", name));
    }

    pub async fn prune_memory(&mut self, entry_id: uuid::Uuid) {
        let wake = self.wake.lock().await;
        if wake.prune(&entry_id).is_ok() {
            drop(wake);
            self.add_log(&format!("Memory pruned: {}", entry_id));
            self.refresh_memory().await;
        }
    }
}
