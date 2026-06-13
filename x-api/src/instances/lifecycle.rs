use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;

use x_types::filter::{InstanceFilter, Pagination};
use x_types::ids::{InstanceId, Namespace, SequenceId, TenantId};
use x_types::instance::{InstanceState, TaskInstance};

use super::types::{
    parse_states, BatchCreateRequest, CreateInstanceRequest, ListQuery, UpdateContextRequest,
    UpdateStateRequest,
};
use crate::error::ApiError;
use crate::AppState;

/// Scope an idempotency key by execution mode. Non-empty keys for dry-runs are
/// prefixed so a real run reusing the same key isn't deduplicated to a prior
/// simulation (see E1). Empty / absent keys are preserved verbatim so the
/// existing empty-key unique-constraint semantics are unchanged.
///
/// Edge case (accepted): a *real* run whose key is literally `dryrun:<x>` shares
/// the dedup namespace of a dry-run with key `<x>`. Harmless in practice — caller
/// keys are opaque and this collision only affects at-most-once dedup, never
/// correctness of execution.
fn mode_scoped_idempotency_key(key: Option<&str>, dry_run: bool) -> Option<String> {
    key.map(|k| {
        if !k.is_empty() && dry_run {
            format!("dryrun:{k}")
        } else {
            k.to_string()
        }
    })
}

pub(crate) async fn create_instance(
    State(state): State<AppState>,
    Json(req): Json<CreateInstanceRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let now = Utc::now();

    // Dry-run is requested per run; stamp it onto the instance's runtime
    // context so every handler sees it (and it survives checkpoint/replay).
    let mut context = req.context;
    context.runtime.dry_run = req.dry_run || context.runtime.dry_run;
    context.runtime.dry_run_auto_approve =
        req.dry_run_auto_approve || context.runtime.dry_run_auto_approve;
    let dry_run = context.runtime.dry_run;

    // Idempotency identity must include the execution mode: otherwise a real
    // run reusing a dry-run's key would be silently deduplicated to the
    // simulation (the real workflow never executes). Namespace non-empty keys
    // by mode; an empty key is preserved verbatim so the existing empty-key
    // unique-constraint behavior is unchanged.
    let effective_idem = mode_scoped_idempotency_key(req.idempotency_key.as_deref(), dry_run);

    // Idempotency check: only for non-empty keys (empty keys are not a dedup
    // lookup — they rely on the DB unique constraint).
    if let Some(ref idem_key) = effective_idem {
        if !idem_key.is_empty() {
            if let Some(existing) = state
                .storage
                .find_by_idempotency_key(&TenantId::unchecked("default"), idem_key)
                .await
                .map_err(|e| ApiError::from_storage(e, "instance"))?
            {
                return Ok((StatusCode::OK, Json(serde_json::json!({"id": existing.id}))));
            }
        }
    }

    let instance = TaskInstance {
        id: InstanceId::new(),
        sequence_id: req.sequence_id,
        tenant_id: TenantId::unchecked("default"),
        namespace: req.namespace,
        state: InstanceState::Scheduled,
        next_fire_at: Some(now),
        priority: req.priority,
        timezone: req.timezone,
        metadata: req.metadata,
        context,
        concurrency_key: req.concurrency_key,
        max_concurrency: req.max_concurrency,
        idempotency_key: effective_idem,
        session_id: None,
        parent_instance_id: None,
        created_at: now,
        updated_at: now,
    };

    state
        .storage
        .create_instance(&instance)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    // Surface dry-run mode in telemetry so simulated runs are distinguishable
    // from real ones (the flag is also queryable on the instance's
    // `context.runtime.dry_run`).
    if instance.context.runtime.dry_run {
        tracing::info!(instance_id = %instance.id, dry_run = true, "instance created (dry-run)");
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": instance.id })),
    ))
}

pub(crate) async fn create_instances_batch(
    State(state): State<AppState>,
    Json(req): Json<BatchCreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let now = Utc::now();
    let instances: Vec<TaskInstance> = req
        .instances
        .into_iter()
        .map(|template| {
            let mut context = template.context;
            context.runtime.dry_run = template.dry_run || context.runtime.dry_run;
            context.runtime.dry_run_auto_approve =
                template.dry_run_auto_approve || context.runtime.dry_run_auto_approve;
            // Same mode-namespacing as the single-create path (see E1).
            let idempotency_key = mode_scoped_idempotency_key(
                template.idempotency_key.as_deref(),
                context.runtime.dry_run,
            );
            TaskInstance {
                id: InstanceId::new(),
                sequence_id: template.sequence_id,
                tenant_id: TenantId::unchecked("default"),
                namespace: template.namespace,
                state: InstanceState::Scheduled,
                next_fire_at: Some(now),
                priority: template.priority,
                timezone: template.timezone,
                metadata: template.metadata,
                context,
                concurrency_key: template.concurrency_key,
                max_concurrency: template.max_concurrency,
                idempotency_key,
                session_id: None,
                parent_instance_id: None,
                created_at: now,
                updated_at: now,
            }
        })
        .collect();

    let count = state
        .storage
        .create_instances_batch(&instances)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    tracing::info!(count = count, "batch created instances");
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "created": count }))))
}

pub(crate) async fn get_instance(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let instance_id = InstanceId::from_uuid(id);
    let instance = state
        .storage
        .get_instance(instance_id)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?
        .ok_or_else(|| ApiError::NotFound(format!("instance {id}")))?;
    Ok(Json(instance))
}

pub(crate) async fn list_instances(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let filter = InstanceFilter {
        tenant_id: None,
        namespace: q.namespace.map(Namespace::new),
        sequence_id: q.sequence_id.map(SequenceId::from_uuid),
        states: None,
        metadata_filter: None,
        priority: None,
    };
    let pagination = Pagination {
        offset: q.offset,
        limit: q.limit.min(1000),
        sort_ascending: false,
    };

    let instances = state
        .storage
        .list_instances(&filter, &pagination)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    let count = state
        .storage
        .count_instances(&filter)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    Ok(Json(serde_json::json!({
        "items": instances,
        "total": count,
        "has_more": instances.len() as u64 >= pagination.limit as u64
    })))
}

pub(crate) async fn update_state(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<UpdateStateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let instance_id = InstanceId::from_uuid(id);

    state
        .storage
        .update_instance_state(instance_id, req.state, req.next_fire_at)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    Ok(StatusCode::OK)
}

pub(crate) async fn update_context(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<UpdateContextRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let instance_id = InstanceId::from_uuid(id);
    let mut instance = state
        .storage
        .get_instance(instance_id)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?
        .ok_or_else(|| ApiError::NotFound(format!("instance {id}")))?;

    instance.context.data = req.context.data;
    instance.context.config = req.context.config;

    state
        .storage
        .update_instance_context(instance_id, &instance.context)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    Ok(StatusCode::OK)
}

pub(crate) async fn retry_instance(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let instance_id = InstanceId::from_uuid(id);
    let instance = state
        .storage
        .get_instance(instance_id)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?
        .ok_or_else(|| ApiError::NotFound(format!("instance {id}")))?;

    if !instance.state.is_terminal() {
        return Err(ApiError::Conflict(format!(
            "instance {id} is in state {:?}, not terminal",
            instance.state
        )));
    }

    state
        .storage
        .update_instance_state(instance_id, InstanceState::Scheduled, Some(Utc::now()))
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    Ok(StatusCode::OK)
}
