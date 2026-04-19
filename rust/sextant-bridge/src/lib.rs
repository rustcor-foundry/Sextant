use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use sextant_privacy::{PrivacyMasker, PrivacyLevel};

use std::sync::Mutex;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MediaType {
    VideoFrame,
    AudioChunk,
    ScreenCapture,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MediaPacket {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub media_type: MediaType,
    pub data_base64: String, // Base64 encoded raw data or thumbnail
    pub metadata: String,    // e.g., "Frame 450", "Audio 2s"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MultiModalPerception {
    pub summary: String,
    pub detected_entities: Vec<String>,
    pub transcription: Option<String>,
    pub visual_description: Option<String>,
}

pub struct NeuralBridge {
    active_streams: Vec<Uuid>,
    privacy_masker: PrivacyMasker,
    privacy_level: Mutex<PrivacyLevel>,
}

impl NeuralBridge {
    pub fn new() -> Self {
        Self {
            active_streams: Vec::new(),
            privacy_masker: PrivacyMasker::new(),
            privacy_level: Mutex::new(PrivacyLevel::Standard),
        }
    }

    pub fn set_privacy_level(&self, level: PrivacyLevel) {
        if let Ok(mut l) = self.privacy_level.lock() {
            *l = level;
        }
    }

    pub fn active_stream_count(&self) -> usize {
        self.active_streams.len()
    }

    pub fn capture_frame(&self, tab_id: &Uuid) -> MediaPacket {
        // In a real app, this would use wgpu or OS-level screen capture
        let metadata = format!("Tab {} active viewport capture", tab_id);
        let redacted_metadata = self.privacy_masker.redact_image_simulated(&metadata);

        MediaPacket {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            media_type: MediaType::VideoFrame,
            data_base64: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BfAAAACQEBXfeTKwAAAABJRU5ErkJggg==".to_string(),
            metadata: redacted_metadata,
        }
    }

    pub fn capture_audio(&self, tab_id: &Uuid) -> MediaPacket {
        // In a real app, this would capture the system/tab audio loopback
        MediaPacket {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            media_type: MediaType::AudioChunk,
            data_base64: "UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQAAAAA=".to_string(),
            metadata: format!("Tab {} audio stream chunk", tab_id),
        }
    }

    pub fn bridge_to_neural_engine(&self, packets: Vec<MediaPacket>) -> MultiModalPerception {
        // This simulates the "Neural Bridge" processing the raw packets
        // into a format the PilotBrain can understand.
        
        let has_video = packets.iter().any(|p| matches!(p.media_type, MediaType::VideoFrame));
        let has_audio = packets.iter().any(|p| matches!(p.media_type, MediaType::AudioChunk));

        let visual_desc: Option<String> = if has_video { Some("A technical dashboard with real-time graphs and a code editor.".to_string()) } else { None };
        let transcription: Option<String> = if has_audio { Some("The speaker is discussing the benefits of Rust for systems programming.".to_string()) } else { None };

        let level = self.privacy_level.lock().unwrap();

        MultiModalPerception {
            summary: format!("Perceived {} media packets via Neural Bridge.", packets.len()),
            detected_entities: vec!["User Interface".into(), "Video Player".into()],
            transcription: transcription.map(|t| self.privacy_masker.redact_text(&t, &level)),
            visual_description: visual_desc.map(|v| self.privacy_masker.redact_text(&v, &level)),
        }
    }
}
