use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MeshPayload {
    ChatMessage(String),
    TabShare { url: String, title: String },
    FileShare { name: String, size: u64, type_id: String, content_base64: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MeshMessage {
    pub id: Uuid,
    pub sender_did: String,
    pub timestamp: DateTime<Utc>,
    pub payload: MeshPayload,
    pub pq_signature: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MeshPeer {
    pub did: String,
    pub label: String,
    pub last_seen: DateTime<Utc>,
}

pub struct SextantMesh {
    local_did: String,
    peers: Vec<MeshPeer>,
    message_history: Vec<MeshMessage>,
    wg_interface: String,
    peer_tunnels: HashMap<String, String>, // DID -> WG Public Key
}

impl SextantMesh {
    pub fn new(local_did: String) -> Self {
        Self {
            local_did,
            peers: Vec::new(),
            message_history: Vec::new(),
            wg_interface: "wg0".to_string(),
            peer_tunnels: HashMap::new(),
        }
    }

    pub fn setup_wireguard_tunnel(&mut self, peer_did: &str, public_key: &str) {
        // In a real app, this would use wireguard-uapi to configure the kernel/userspace interface
        self.peer_tunnels.insert(peer_did.to_string(), public_key.to_string());
    }

    pub fn wireguard_interface(&self) -> &str {
        &self.wg_interface
    }

    pub fn add_peer(&mut self, peer: MeshPeer) {
        if !self.peers.iter().any(|p| p.did == peer.did) {
            self.peers.push(peer);
        }
    }

    pub fn broadcast(&mut self, payload: MeshPayload, pq_signature: Option<String>) -> MeshMessage {
        let msg = MeshMessage {
            id: Uuid::new_v4(),
            sender_did: self.local_did.clone(),
            timestamp: Utc::now(),
            payload,
            pq_signature,
        };
        self.message_history.push(msg.clone());
        msg
    }

    pub fn receive_message(&mut self, msg: MeshMessage) {
        self.message_history.push(msg);
    }

    pub fn get_history(&self) -> Vec<MeshMessage> {
        self.message_history.clone()
    }

    pub fn get_peers(&self) -> Vec<MeshPeer> {
        self.peers.clone()
    }
}
