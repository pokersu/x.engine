use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
// removed utoipa

use crate::ids::TenantId;

/// The kind of event source that fires a trigger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TriggerType {
    #[default]
    Webhook,
    Nats,
    FileWatch,
    /// In-process event bus — fired by `POST /triggers/{slug}/fire` or
    /// internally by workflows via the `emit_event` built-in handler.
    /// Unlike webhooks, event triggers carry no HMAC validation — they're
    /// intended for trusted server-to-server or in-cluster integration.
    Event,
}

impl fmt::Display for TriggerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Webhook => f.write_str("webhook"),
            Self::Nats => f.write_str("nats"),
            Self::FileWatch => f.write_str("file_watch"),
            Self::Event => f.write_str("event"),
        }
    }
}

impl TriggerType {
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s {
            "webhook" => Some(Self::Webhook),
            "nats" => Some(Self::Nats),
            "file_watch" => Some(Self::FileWatch),
            "event" => Some(Self::Event),
            _ => None,
        }
    }
}

/// A persisted trigger definition that maps an event source to a sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDef {
    /// Unique slug used in the trigger URL / identifier.
    pub slug: String,
    /// Sequence name to instantiate when fired.
    pub sequence_name: String,
    /// Optional specific version. If omitted, uses latest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
    /// Tenant ID for created instances.
    pub tenant_id: TenantId,
    /// Namespace for created instances.
    #[serde(default = "crate::serde_defaults::default_namespace")]
    pub namespace: String,
    /// Whether this trigger is active.
    #[serde(default = "crate::serde_defaults::yes")]
    pub enabled: bool,
    /// Optional secret for HMAC validation of incoming requests.
    /// Redacted in Debug/Serialize output — use `.secret.as_ref().map(|s| s.expose())` to access.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<crate::config::SecretString>,
    /// Trigger type: `webhook`, `nats`, `file_watch`.
    #[serde(default)]
    pub trigger_type: TriggerType,
    /// Type-specific configuration (JSON).
    #[serde(default)]
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

