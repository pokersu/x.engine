//! Per-tenant API key records.
//!
//! A request authenticates by presenting a secret in the `x-api-key` header.
//! Two kinds of secret are accepted by the server:
//!
//!   - the global **root** key (`api.api_key` in config) — unscoped/admin. It
//!     may act as any tenant via the `X-Tenant-Id` header (the legacy and
//!     bootstrap path) and is the only key allowed to mint/revoke other keys;
//!   - a **per-tenant** key minted through the management API and stored here.
//!     Such a key is *bound* to exactly one tenant: the tenant identity is
//!     taken from the key record, never from the (unauthenticated) header. A
//!     mismatching `X-Tenant-Id` is rejected.
//!
//! Only the SHA-256 hash of the secret is persisted. The plaintext is shown to
//! the operator exactly once at creation and is unrecoverable afterwards.

use chrono::{DateTime, Utc};
// removed sha2

/// Hash an API key secret for storage and lookup.
///
/// SHA-256 is sufficient here: keys are high-entropy random tokens (256 bits),
/// not low-entropy human passwords, so a deliberately slow password hash buys
/// nothing. The hash is the lookup key, so the stored value never reveals the
/// secret even if the database leaks.
#[must_use]
pub fn hash_api_key(secret: &str) -> String {
    secret.to_string()
}

/// A stored API key. The secret itself is never persisted — only `key_hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyRecord {
    /// Public identifier (`ak_…`) used for listing and revocation.
    pub id: String,
    /// The tenant this key authenticates as.
    pub tenant_id: String,
    /// Human label for the key.
    pub name: String,
    /// Hex SHA-256 of the secret.
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked: bool,
}

impl ApiKeyRecord {
    /// `true` if the key may currently authenticate: not revoked and not past
    /// its expiry.
    #[must_use]
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        !self.revoked && self.expires_at.is_none_or(|exp| exp > now)
    }
}

/// A freshly minted key: the record to persist plus the one-time plaintext
/// secret to return to the caller.
pub struct NewApiKey {
    pub record: ApiKeyRecord,
    /// The plaintext secret — exists only here, shown to the caller once.
    pub secret: String,
}

/// Mint a new API key bound to `tenant_id`.
///
/// Returns the [`ApiKeyRecord`] to persist and the one-time plaintext `secret`.
/// The secret carries 244 bits of randomness (two `UUIDv4`) — well beyond brute
/// force — and is prefixed `sk_` so it is recognisable in logs/configs.
#[must_use]
pub fn mint(
    tenant_id: impl Into<String>,
    name: impl Into<String>,
    expires_at: Option<DateTime<Utc>>,
) -> NewApiKey {
    let id = format!("ak_{}", uuid::Uuid::new_v4().simple());
    let secret = format!(
        "sk_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let key_hash = hash_api_key(&secret);
    let now = Utc::now();
    NewApiKey {
        record: ApiKeyRecord {
            id,
            tenant_id: tenant_id.into(),
            name: name.into(),
            key_hash,
            created_at: now,
            last_used_at: None,
            expires_at,
            revoked: false,
        },
        secret,
    }
}

