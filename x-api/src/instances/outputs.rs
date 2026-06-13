use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use x_types::ids::InstanceId;

use crate::error::ApiError;
use crate::AppState;

pub async fn get_outputs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let instance_id = InstanceId::from_uuid(id);
    let mut outputs = state
        .storage
        .get_all_outputs(instance_id)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    // Strip internal bookkeeping rows that are not user-facing outputs.
    strip_internal_outputs(&mut outputs);

    Ok(Json(outputs))
}

/// Drop internal bookkeeping rows that are not user-facing outputs.
///
/// Two kinds exist, neither a real step output:
///   - `__in_progress__`: a crash-recovery sentinel written before handler
///     execution.
///   - `__retry__`: a retry marker written after a retryable failure to
///     advance the attempt counter (see `scheduler/step_exec.rs`).
///
/// Once a real output exists for the block these are noise and are removed —
/// otherwise a caller doing `outputs.find(block_id)` could pick up the marker
/// instead of the answer. On a terminal/mid-retry path where the marker or
/// sentinel is the ONLY row for the block it is kept, so callers can still see
/// the step ran.
fn strip_internal_outputs(outputs: &mut Vec<x_types::output::BlockOutput>) {
    fn is_internal(o: &x_types::output::BlockOutput) -> bool {
        matches!(
            o.output_ref.as_deref(),
            Some("__in_progress__" | "__retry__")
        )
    }
    let blocks_with_real_output: std::collections::HashSet<String> = outputs
        .iter()
        .filter(|o| !is_internal(o))
        .map(|o| o.block_id.as_str().to_owned())
        .collect();
    outputs.retain(|o| !is_internal(o) || !blocks_with_real_output.contains(o.block_id.as_str()));
}

#[cfg(test)]
mod tests {
    use super::strip_internal_outputs;
    use x_types::ids::{BlockId, InstanceId};
    use x_types::output::BlockOutput;

    fn out(block: &str, output_ref: Option<&str>, value: &str) -> BlockOutput {
        BlockOutput {
            id: uuid::Uuid::now_v7(),
            instance_id: InstanceId::new(),
            block_id: BlockId::new(block),
            output: serde_json::json!({ "v": value }),
            output_ref: output_ref.map(str::to_owned),
            output_size: 0,
            attempt: 0,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn strips_retry_marker_and_sentinel_when_real_output_exists() {
        // Order mimics storage (created_at ASC): marker, sentinel, then the
        // real output. A caller's `find(block)` must land on the real output.
        let mut outputs = vec![
            out("s1", Some("__retry__"), "retry-bookkeeping"),
            out("s1", Some("__in_progress__"), "sentinel"),
            out("s1", None, "ok after retry"),
        ];
        strip_internal_outputs(&mut outputs);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].output["v"], "ok after retry");
    }

    #[test]
    fn keeps_marker_when_it_is_the_only_row() {
        // Mid-retry / permanent-failure with no real output: keep the row so
        // callers can see the step ran rather than seeing nothing.
        let mut outputs = vec![out("s1", Some("__retry__"), "still-retrying")];
        strip_internal_outputs(&mut outputs);
        assert_eq!(outputs.len(), 1);
    }
}

pub async fn get_execution_tree(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let instance_id = InstanceId::from_uuid(id);
    let tree = state
        .storage
        .get_execution_tree(instance_id)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;
    Ok(Json(tree))
}
