//! Credentials registry: named, tenant-scoped secrets referenced by step params
//! via the `credentials://<id>` URI scheme.
//!
//! The sequence JSON stored in Postgres/SQLite never contains raw secrets —
//! authors write `"auth": { "access_token": "credentials://stripe-prod" }` and
//! the step handler resolves the reference at dispatch time through the
//! `StorageBackend::get_credential` call. This lets operators rotate secrets
//! without touching any sequence, and keeps a single encrypted blob per secret
//! regardless of how many workflows consume it.
//!
//! `OAuth2` credentials carry a `refresh_url` and `refresh_token`; a background
//! loop in `orch8-engine` refreshes them a few minutes before expiry.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
// removed utoipa

/// The kind of secret held in a credential.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// A single opaque token (Bearer, API key, PAT).
    #[default]
    ApiKey,
    /// `OAuth2` access/refresh token pair with optional expiry.
    Oauth2,
    /// Basic auth username + password.
    Basic,
}

impl fmt::Display for CredentialKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApiKey => f.write_str("api_key"),
            Self::Oauth2 => f.write_str("oauth2"),
            Self::Basic => f.write_str("basic"),
        }
    }
}

impl CredentialKind {
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s {
            "api_key" => Some(Self::ApiKey),
            "oauth2" => Some(Self::Oauth2),
            "basic" => Some(Self::Basic),
            _ => None,
        }
    }
}

/// A persisted credential — the secret value is redacted in Debug/Serialize
/// output (via [`crate::config::SecretString`]) so it cannot leak through API
/// responses or logs.
///
/// The `value` field's shape depends on `kind`:
/// - `api_key`: `{"token": "..."}`
/// - `oauth2`: `{"access_token": "...", "refresh_token": "..."}` (`refresh_token`
///   is duplicated in the top-level [`Self::refresh_token`] column for indexed
///   lookup by the refresh loop)
/// - `basic`:  `{"username": "...", "password": "..."}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialDef {
    /// Unique credential id (used in `credentials://<id>` references).
    pub id: String,
    /// Tenant that owns this credential (empty = global).
    #[serde(default)]
    pub tenant_id: String,
    /// Human-readable display name.
    pub name: String,
    /// What kind of secret this is.
    #[serde(default)]
    pub kind: CredentialKind,
    /// The secret material as a JSON-encoded string. Redacted in Debug/Serialize.
    /// Use `.value.expose()` to obtain the raw value; pass to `serde_json::from_str`
    /// to parse into a typed struct.
    pub value: crate::config::SecretString,
    /// When the current `value` becomes unusable. Used by the refresh loop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// `OAuth2` token endpoint for refresh. None for static credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    /// `OAuth2` refresh token. Redacted in Debug/Serialize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<crate::config::SecretString>,
    /// Whether this credential can be resolved. Disabled credentials cause
    /// `credentials://<id>` refs to fail-fast with a permanent error.
    #[serde(default = "crate::serde_defaults::yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

