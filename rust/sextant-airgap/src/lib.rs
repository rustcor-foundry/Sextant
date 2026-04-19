use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum AirGapStatus {
    Online,
    Isolated, // Network blocked, local inference only
    Hardened, // Network blocked, encrypted local storage only
}

pub struct SextantAirGap {
    status: AirGapStatus,
    local_model_path: String,
    is_offline_ready: bool,
}

impl SextantAirGap {
    pub fn new() -> Self {
        Self {
            status: AirGapStatus::Online,
            local_model_path: "/models/llama-3-8b-q4.gguf".to_string(),
            is_offline_ready: true,
        }
    }

    pub fn set_status(&mut self, status: AirGapStatus) {
        self.status = status;
    }

    pub fn get_status(&self) -> AirGapStatus {
        self.status.clone()
    }

    pub fn check_network_allowed(&self) -> bool {
        matches!(self.status, AirGapStatus::Online)
    }

    pub fn get_local_model(&self) -> Result<String, String> {
        if self.is_offline_ready {
            Ok(self.local_model_path.clone())
        } else {
            Err("Local model not downloaded or corrupted.".into())
        }
    }
}
