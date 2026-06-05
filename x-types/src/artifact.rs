use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMeta {
    pub key: String,
    pub size: u64,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
    pub id: String,
    pub instance_id: String,
    pub uri: String,
}

pub type ArtifactRef = ArtifactMeta;

impl ArtifactMeta {
    pub fn new(key: String, size: u64) -> Self {
        Self {
            id: String::new(),
            key,
            instance_id: String::new(),
            uri: String::new(),
            size,
            content_type: String::new(),
            created_at: Utc::now(),
        }
    }

    pub fn into_output(self) -> serde_json::Value {
        serde_json::json!({
            "key": self.key,
            "size": self.size,
        })
    }
}
