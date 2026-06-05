use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;

use x_types::filter::{InstanceFilter, Pagination};
use x_types::ids::{InstanceId, Namespace, SequenceId, TenantId};
use x_types::instance::{InstanceState, TaskInstance};

use super::types::{
    parse_states, BatchCreateRequest, CountResponse, CreateInstanceRequest, ListQuery,
    UpdateContextRequest, UpdateStateRequest,
};
use crate::error::ApiError;
use crate::AppState;

pub(crate) async fn create_instance(
    State(state): State<AppState>,
    Json(req): Json<CreateInstanceRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let now = Utc::now();
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
        context: req.context,
        concurrency_key: None,
        max_concurrency: None,
        idempotency_key: None,
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

    Ok((StatusCode::CREATED, Json(instance)))
}

pub(crate) async fn create_instances_batch(
    State(state): State<AppState>,
    Json(req): Json<BatchCreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let now = Utc::now();
    let instances: Vec<TaskInstance> = req
        .instances
        .into_iter()
        .map(|template| TaskInstance {
            id: InstanceId::new(),
            sequence_id: template.sequence_id,
            tenant_id: TenantId::unchecked("default"),
            namespace: template.namespace,
            state: InstanceState::Scheduled,
            next_fire_at: Some(now),
            priority: template.priority,
            timezone: template.timezone,
            metadata: template.metadata,
            context: template.context,
            concurrency_key: None,
            max_concurrency: None,
            idempotency_key: None,
            session_id: None,
            parent_instance_id: None,
            created_at: now,
            updated_at: now,
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
