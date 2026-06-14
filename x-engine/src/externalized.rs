//! Externalization marker helpers.
//!
//! Supports two marker shapes:
//! - `_externalized` marker: `{"_externalized": true, "_ref": "<key>"}` —
//!   for values stored in the `externalized_state` table (legacy).
//! - `$ref` marker: `{"$ref": "<artifact_key>"}` —
//!   for values offloaded to the artifact store (object storage).

use std::sync::Arc;
use std::sync::OnceLock;

use x_storage::StorageBackend;
use x_types::artifact::{extract_offload_key, is_offload_marker};
use x_types::context::ExecutionContext;
use x_types::error::StorageError;
use serde_json::Value;

use crate::context_cache::ContextCache;

/// Global singleton context cache shared across the engine. Content-addressed
/// by artifact key so the same offloaded value is loaded at most once.
fn global_cache() -> &'static ContextCache {
    static CACHE: OnceLock<ContextCache> = OnceLock::new();
    CACHE.get_or_init(ContextCache::new)
}

pub const EXTERNALIZED_FLAG: &str = "_externalized";
pub const REF_KEY: &str = "_ref";

/// Return `true` iff `v` is exactly the externalization envelope — an object
/// with precisely two keys (`_externalized: true`, `_ref: "<string>"`).
#[must_use]
pub fn is_ref_marker(v: &Value) -> bool {
    let Some(obj) = v.as_object() else {
        return false;
    };
    if obj.len() != 2 {
        return false;
    }
    matches!(obj.get(EXTERNALIZED_FLAG), Some(Value::Bool(true)))
        && matches!(obj.get(REF_KEY), Some(Value::String(_)))
}

/// Return the `_ref` string if `v` is a marker, else `None`.
#[must_use]
pub fn extract_ref_key(v: &Value) -> Option<&str> {
    if !is_ref_marker(v) {
        return None;
    }
    v.as_object()?.get(REF_KEY)?.as_str()
}

/// Walk the top-level fields of `ctx.data` and replace any externalization
/// markers with their resolved payloads.
///
/// Non-object `data` and non-marker values are left untouched; broken refs
/// (payload missing in storage) are also left in place so downstream code
/// can detect them.
///
/// This is the shared inflator used by every engine site that needs to see
/// user context values: step dispatch (`step_block.rs`), router condition
/// evaluation (`router.rs`), and any future site that reads `instance.context`
/// before it is handed to user code.
///
/// Only top-level keys are scanned — nested objects are not recursed into.
/// This mirrors the externalizer's write side: `externalize_fields` only
/// promotes top-level keys, so the resolver only needs to look there.
pub async fn resolve_context_markers(
    storage: &dyn StorageBackend,
    mut ctx: ExecutionContext,
) -> Result<ExecutionContext, StorageError> {
    let Some(obj) = ctx.data.as_object_mut() else {
        return Ok(ctx);
    };
    for (_key, value) in obj.iter_mut() {
        if let Some(ref_key) = extract_ref_key(value) {
            if let Some(resolved) = storage.get_externalized_state(ref_key).await? {
                *value = resolved;
            }
        }
    }
    Ok(ctx)
}

/// Resolve `$ref` offload markers in an instance's `context.data` in-place.
///
/// Scans the top-level keys of `data`. For each `{"$ref": "<key>"}` marker:
/// 1. Checks the in-memory [`ContextCache`] first.
/// 2. Cache miss → fetches from artifact store via `storage.get_artifact()`.
/// 3. Replaces the marker with the resolved JSON value.
/// 4. Stores the resolved value in the cache for subsequent accesses.
///
/// This is called **lazily**: right before a step is about to execute, so
/// only the instance being processed has its markers resolved — not every
/// instance in the current tick batch.
pub async fn resolve_offload_markers(
    storage: &dyn StorageBackend,
    cache: &ContextCache,
    context: &mut ExecutionContext,
) {
    let Some(obj) = context.data.as_object_mut() else {
        return;
    };

    // Phase 1: collect all $ref keys not yet cached.
    let mut keys_to_fetch: Vec<String> = Vec::new();
    for value in obj.values() {
        if let Some(key) = extract_offload_key(value) {
            if cache.get(key).is_none() {
                keys_to_fetch.push(key.to_string());
            }
        }
    }

    // Phase 2: fetch missing keys from artifact store.
    if !keys_to_fetch.is_empty() {
        for key in &keys_to_fetch {
            match storage.get_artifact(key).await {
                Ok(Some(bytes)) => {
                    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
                        Ok(v) => v,
                        Err(_) => serde_json::Value::String(
                            String::from_utf8_lossy(&bytes).into_owned(),
                        ),
                    };
                    cache.set(key.clone(), value);
                }
                Ok(None) => {
                    tracing::warn!(artifact_key = %key, "offload marker references missing artifact");
                }
                Err(e) => {
                    tracing::warn!(artifact_key = %key, error = %e, "failed to fetch offloaded artifact");
                }
            }
        }
    }

    // Phase 3: hydrate all markers from cache.
    for value in obj.values_mut() {
        if let Some(key) = extract_offload_key(value) {
            if let Some(cached) = cache.get(key) {
                *value = cached.clone();
            }
        }
    }
}

/// Convenience wrapper around [`resolve_offload_markers`] that uses the
/// engine-global context cache.
pub async fn resolve_offload_markers_global(
    storage: &dyn StorageBackend,
    context: &mut ExecutionContext,
) {
    resolve_offload_markers(storage, global_cache(), context).await;
}

/// Invalidate a specific artifact key in the global context cache.
/// Called after context is updated so stale cached values don't persist.
pub fn invalidate_cache_key(key: &str) {
    global_cache().invalidate(key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_ref_marker_true_for_envelope() {
        let v = json!({"_externalized": true, "_ref": "k"});
        assert!(is_ref_marker(&v));
    }

    #[test]
    fn is_ref_marker_false_for_extra_keys() {
        let v = json!({"_externalized": true, "_ref": "k", "other": 1});
        assert!(!is_ref_marker(&v));
    }

    #[test]
    fn is_ref_marker_false_for_missing_flag() {
        let v = json!({"_ref": "k", "other": 1});
        assert!(!is_ref_marker(&v));
    }

    #[test]
    fn is_ref_marker_false_for_flag_not_true() {
        let v = json!({"_externalized": false, "_ref": "k"});
        assert!(!is_ref_marker(&v));
    }

    #[test]
    fn is_ref_marker_false_for_non_object() {
        assert!(!is_ref_marker(&json!("string")));
        assert!(!is_ref_marker(&json!(42)));
        assert!(!is_ref_marker(&json!([1, 2])));
        assert!(!is_ref_marker(&json!(null)));
    }

    #[test]
    fn extract_ref_key_returns_key() {
        let v = json!({"_externalized": true, "_ref": "inst:block"});
        assert_eq!(extract_ref_key(&v), Some("inst:block"));
    }

    #[test]
    fn extract_ref_key_none_for_non_marker() {
        assert_eq!(extract_ref_key(&json!({"other": 1})), None);
        assert_eq!(extract_ref_key(&json!("scalar")), None);
    }

    #[test]
    fn is_ref_marker_false_for_non_string_ref() {
        assert!(!is_ref_marker(&json!({"_externalized": true, "_ref": 42})));
        assert!(!is_ref_marker(&json!({"_externalized": true, "_ref": null})));
        assert!(!is_ref_marker(&json!({"_externalized": true, "_ref": ["a"]})));
        assert!(!is_ref_marker(&json!({"_externalized": true, "_ref": {"nested": "k"}})));
    }

    #[test]
    fn is_ref_marker_false_for_flag_not_bool() {
        assert!(!is_ref_marker(&json!({"_externalized": "true", "_ref": "k"})));
        assert!(!is_ref_marker(&json!({"_externalized": 1, "_ref": "k"})));
    }

    #[test]
    fn is_ref_marker_false_for_empty_object() {
        assert!(!is_ref_marker(&json!({})));
    }

    #[test]
    fn extract_ref_key_returns_empty_string_when_ref_is_empty() {
        let v = json!({"_externalized": true, "_ref": ""});
        assert_eq!(extract_ref_key(&v), Some(""));
    }

    #[test]
    fn extract_ref_key_unicode_key_preserved() {
        let v = json!({"_externalized": true, "_ref": "inst:ctx:data:日本語"});
        assert_eq!(extract_ref_key(&v), Some("inst:ctx:data:日本語"));
    }

    // -- $ref marker tests --

    #[test]
    fn is_offload_marker_recognises_ref_object() {
        let v = json!({"$ref": "inst_xxx/ctx/ocr_result"});
        assert!(is_offload_marker(&v));
        assert_eq!(extract_offload_key(&v), Some("inst_xxx/ctx/ocr_result"));
    }

    #[test]
    fn is_offload_marker_rejects_extra_keys() {
        let v = json!({"$ref": "k", "other": 1});
        assert!(!is_offload_marker(&v));
    }

    #[test]
    fn is_offload_marker_rejects_empty_key() {
        let v = json!({"$ref": ""});
        assert!(!is_offload_marker(&v));
    }

    #[test]
    fn is_offload_marker_rejects_non_object() {
        assert!(!is_offload_marker(&json!("string")));
        assert!(!is_offload_marker(&json!(42)));
        assert!(!is_offload_marker(&json!(null)));
    }

    #[test]
    fn offload_marker_and_is_offload_marker_roundtrip() {
        let key = "inst_xxx/ctx/data_field";
        let m = x_types::artifact::offload_marker(key);
        assert!(is_offload_marker(&m));
        assert_eq!(extract_offload_key(&m), Some(key));
    }
}
