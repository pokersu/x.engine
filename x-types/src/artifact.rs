use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Key used in `context.data` to mark a value that has been offloaded
/// to the artifact store. The value is `{"$ref": "<artifact_key>"}`.
pub const REF_MARKER_KEY: &str = "$ref";

/// Return `true` iff `v` is a `$ref` offload marker (a single-key object
/// `{"$ref": "<string>"}`).
#[must_use]
pub fn is_offload_marker(v: &Value) -> bool {
    let Some(obj) = v.as_object() else {
        return false;
    };
    if obj.len() != 1 {
        return false;
    }
    matches!(obj.get(REF_MARKER_KEY), Some(Value::String(s)) if !s.is_empty())
}

/// Extract the artifact key from a `$ref` offload marker. Returns `None` if
/// `v` is not a valid marker.
#[must_use]
pub fn extract_offload_key(v: &Value) -> Option<&str> {
    v.as_object()?.get(REF_MARKER_KEY)?.as_str().filter(|s| !s.is_empty())
}

/// Build a `$ref` offload marker for the given artifact key.
#[must_use]
pub fn offload_marker(key: &str) -> Value {
    let mut obj = serde_json::Map::with_capacity(1);
    obj.insert(REF_MARKER_KEY.to_string(), Value::String(key.to_string()));
    Value::Object(obj)
}

/// Build the artifact key for a context.data top-level field.
/// Format: `{instance_uuid}/ctx/{field_name}`
#[must_use]
pub fn context_artifact_key(instance_id: &str, field: &str) -> String {
    format!("{instance_id}/ctx/{field}")
}

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
