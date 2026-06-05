//! Built-in `blob_put` / `blob_get` handlers — durable binary I/O.
//!
//! These are the kernel primitive for moving binary artifacts *between* steps:
//! a workflow generates bytes (an image, a rendered document, a fetched asset),
//! stores them with `blob_put`, passes the returned ref forward, and reads them
//! back with `blob_get`. Bytes never travel inline in the execution context —
//! only the small [`ArtifactRef`](x_types::artifact::ArtifactRef) does.
//!
//! Backed by the storage layer's artifact store (local FS / S3-compatible), so
//! durability, encryption, and externalization come for free.
//!
//! ## `blob_put`
//!
//! | Field | Type | Default | Description |
//! |-------|------|---------|-------------|
//! | `text` | string | — | UTF-8 content to store (one of `text`/`data` required) |
//! | `data` | string | — | base64-encoded bytes to store |
//! | `content_type` | string | inferred | MIME type stored with the blob |
//! | `max_size_bytes` | u64 | 25 MiB | Reject payloads larger than this |
//!
//! Output: `{ "artifact": { id, key, content_type, size, uri, … } }`.
//!
//! ## `blob_get`
//!
//! | Field | Type | Default | Description |
//! |-------|------|---------|-------------|
//! | `ref` | string \| object | **required** | Artifact key, `{key}`, or `{artifact:{key}}` |
//! | `encoding` | string | `"base64"` | `"base64"` or `"utf8"` |
//! | `max_size_bytes` | u64 | 25 MiB | Reject artifacts larger than this |
//!
//! Output (base64): `{ "data": "<b64>", "encoding": "base64", "size": N }`.
//! Output (utf8): `{ "text": "<string>", "encoding": "utf8", "size": N }`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use bytes::Bytes;
use serde_json::{json, Value};
use tracing::debug;

use x_types::error::StepError;

use super::StepContext;

/// Default ceiling on a single blob, in bytes (25 MiB). Generous enough for
/// images/documents, small enough to protect engine memory.
const DEFAULT_MAX_BYTES: u64 = 25 * 1024 * 1024;

/// Map an artifact-backend storage error to a step error. A `Unsupported`
/// backend (not configured) is a permanent misconfiguration — retrying would
/// only burn the budget. Transient backend failures stay retryable.
fn artifact_step_err(context: &str, e: &x_types::error::StorageError) -> StepError {
    match e {
        x_types::error::StorageError::Unsupported(_) => StepError::Permanent {
            message: format!("{context}: {e}"),
            details: None,
        },
        _ => StepError::Retryable {
            message: format!("{context}: {e}"),
            details: None,
        },
    }
}

/// Resolve an artifact key from a `ref` param. Accepts a bare key string, a
/// `{ "key": "…" }` object, or the canonical `{ "artifact": { "key": "…" } }`
/// shape that `blob_put` (and `tool_call`) emit — so callers can wire
/// `{{outputs.put.artifact}}` straight through.
fn extract_key(refval: &Value) -> Option<String> {
    match refval {
        Value::String(s) => Some(s.clone()),
        Value::Object(_) => {
            let inner = refval.get("artifact").unwrap_or(refval);
            inner.get("key").and_then(Value::as_str).map(str::to_string)
        }
        _ => None,
    }
}

/// Store bytes as a durable artifact, returning its ref.
pub async fn handle_blob_put(ctx: StepContext) -> Result<Value, StepError> {
    let max_size = ctx
        .params
        .get("max_size_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_BYTES);

    // Resolve bytes + a sensible default content type from whichever input
    // is present. `text` is checked first so a UTF-8 default is applied.
    let (bytes, default_ct): (Vec<u8>, &str) = if let Some(text) =
        ctx.params.get("text").and_then(Value::as_str)
    {
        (text.as_bytes().to_vec(), "text/plain; charset=utf-8")
    } else if let Some(data) = ctx.params.get("data").and_then(Value::as_str) {
        let decoded = STANDARD.decode(data).map_err(|e| StepError::Permanent {
            message: format!("blob_put: `data` is not valid base64: {e}"),
            details: None,
        })?;
        (decoded, "application/octet-stream")
    } else {
        return Err(StepError::Permanent {
            message: "blob_put: one of `text` (utf-8 string) or `data` (base64 string) is required"
                .into(),
            details: None,
        });
    };

    if bytes.len() as u64 > max_size {
        return Err(StepError::Permanent {
            message: format!(
                "blob_put: payload {} bytes exceeds max_size_bytes {}",
                bytes.len(),
                max_size
            ),
            details: None,
        });
    }

    let content_type = ctx
        .params
        .get("content_type")
        .and_then(Value::as_str)
        .unwrap_or(default_ct)
        .to_string();

    // Dry-run: report what *would* be stored without touching the backend.
    if ctx.is_dry_run() {
        return Ok(json!({
            "dry_run": true,
            "size": bytes.len(),
            "content_type": content_type,
        }));
    }

    let aref = ctx
        .storage
        .put_artifact(ctx.instance_id, &content_type, Bytes::from(bytes))
        .await
        .map_err(|e| artifact_step_err("blob_put store error", &e))?;

    debug!(
        instance_id = %ctx.instance_id,
        block_id = %ctx.block_id,
        key = %aref.key,
        size = aref.size,
        "blob_put stored artifact"
    );
    Ok(aref.into_output())
}

/// Read a stored artifact back as base64 or UTF-8 text.
pub async fn handle_blob_get(ctx: StepContext) -> Result<Value, StepError> {
    let key = ctx
        .params
        .get("ref")
        .and_then(extract_key)
        .ok_or_else(|| StepError::Permanent {
            message: "blob_get: `ref` is required (artifact ref object or key string)".into(),
            details: None,
        })?;

    // IDOR guard: artifact object keys are `<instance_id>/<artifact_id>`. A
    // workflow may only read back artifacts produced by *its own* instance,
    // never a sibling/other-tenant instance's — otherwise a crafted `ref` like
    // `<victim-instance-uuid>/<artifact-id>` would exfiltrate cross-tenant data.
    // Mirrors the same check `tool_call` applies to fetched artifacts.
    if !super::tool_call::artifact_key_owned_by(&key, ctx.instance_id) {
        return Err(StepError::Permanent {
            message: format!("blob_get: ref '{key}' does not belong to this instance"),
            details: None,
        });
    }

    let encoding = ctx
        .params
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("base64");

    let max_size = ctx
        .params
        .get("max_size_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MAX_BYTES);

    let bytes = ctx
        .storage
        .get_artifact(&key)
        .await
        .map_err(|e| artifact_step_err("blob_get fetch error", &e))?
        .ok_or_else(|| StepError::Permanent {
            message: format!("blob_get: artifact not found: {key}"),
            details: None,
        })?;

    if bytes.len() as u64 > max_size {
        return Err(StepError::Permanent {
            message: format!(
                "blob_get: artifact {} bytes exceeds max_size_bytes {}",
                bytes.len(),
                max_size
            ),
            details: None,
        });
    }
    let size = bytes.len();

    match encoding {
        "base64" => Ok(json!({
            "data": STANDARD.encode(&bytes),
            "encoding": "base64",
            "size": size,
        })),
        "utf8" | "text" => {
            let text = String::from_utf8(bytes).map_err(|e| StepError::Permanent {
                message: format!(
                    "blob_get: artifact is not valid UTF-8 (use encoding=base64): {e}"
                ),
                details: None,
            })?;
            Ok(json!({ "text": text, "encoding": "utf8", "size": size }))
        }
        other => Err(StepError::Permanent {
            message: format!("blob_get: unknown encoding `{other}` (use `base64` or `utf8`)"),
            details: None,
        }),
    }
}

