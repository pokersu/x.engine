use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, delete};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use x_types::ids::{InstanceId, Namespace, SequenceId, TenantId};
use x_types::sequence::{SequenceDefinition, SequenceStatus};

use crate::error::ApiError;
use crate::AppState;
use tracing::warn;

fn default_tenant() -> TenantId {
    TenantId::unchecked("default")
}

fn default_namespace() -> Namespace {
    Namespace::new("default")
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sequences", post(create_sequence).get(list_sequences))
        .route("/sequences/{id}", get(get_sequence).delete(delete_sequence))
        .route("/sequences/{id}/deprecate", post(deprecate_sequence))
        .route("/sequences/{id}/status", post(set_sequence_status))
        .route("/sequences/by-name", get(get_sequence_by_name))
        .route("/sequences/versions", get(list_sequence_versions))
}

pub(crate) async fn create_sequence(
    State(state): State<AppState>,
    Json(mut seq): Json<SequenceDefinition>,
) -> Result<impl IntoResponse, ApiError> {
    seq.validate()
        .map_err(|e| ApiError::InvalidArgument(e.to_string()))?;

    let mut warnings = x_engine::template::validate_sequence_templates(&seq);
    for tw in &warnings {
        warn!("sequence template warning: {tw}");
    }

    state
        .storage
        .create_sequence(&seq)
        .await
        .map_err(|e| ApiError::from_storage(e, "sequence"))?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": seq.id }))))
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    namespace: Option<String>,
    limit: Option<u32>,
    offset: Option<u64>,
}

pub(crate) async fn list_sequences(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let namespace = q.namespace.as_deref().map(Namespace::new);
    let limit = q.limit.unwrap_or(100).min(1000);
    let offset = q.offset.unwrap_or(0) as u32;
    let tenant_id = default_tenant();
    let sequences = state
        .storage
        .list_sequences(Some(&tenant_id), namespace.as_ref(), limit, offset)
        .await
        .map_err(|e| ApiError::from_storage(e, "sequence"))?;
    Ok(Json(sequences))
}

pub(crate) async fn get_sequence(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let seq = state
        .storage
        .get_sequence(SequenceId::from_uuid(id))
        .await
        .map_err(|e| ApiError::from_storage(e, "sequence"))?
        .ok_or_else(|| ApiError::NotFound(format!("sequence {id}")))?;
    Ok(Json(seq))
}

pub(crate) async fn delete_sequence(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .storage
        .delete_sequence(SequenceId::from_uuid(id))
        .await
        .map_err(|e| ApiError::from_storage(e, "sequence"))?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn deprecate_sequence(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .storage
        .deprecate_sequence(SequenceId::from_uuid(id))
        .await
        .map_err(|e| ApiError::from_storage(e, "sequence"))?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub(crate) struct StatusRequest {
    status: String,
}

pub(crate) async fn set_sequence_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<StatusRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .storage
        .update_sequence_status(SequenceId::from_uuid(id), &req.status)
        .await
        .map_err(|e| ApiError::from_storage(e, "sequence"))?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub(crate) struct ByNameQuery {
    namespace: Option<String>,
    name: String,
    version: Option<i32>,
}

pub(crate) async fn get_sequence_by_name(
    State(state): State<AppState>,
    Query(q): Query<ByNameQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let namespace = q.namespace.map(Namespace::new).unwrap_or_else(default_namespace);
    let tenant_id = default_tenant();
    let seq = state
        .storage
        .get_sequence_by_name(&tenant_id, &namespace, &q.name, q.version)
        .await
        .map_err(|e| ApiError::from_storage(e, "sequence"))?
        .ok_or_else(|| ApiError::NotFound(format!("sequence {}", q.name)))?;
    Ok(Json(seq))
}

#[derive(Deserialize)]
pub(crate) struct VersionsQuery {
    namespace: Option<String>,
    name: String,
}

pub(crate) async fn list_sequence_versions(
    State(state): State<AppState>,
    Query(q): Query<VersionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let namespace = q.namespace.map(Namespace::new).unwrap_or_else(default_namespace);
    let versions = state
        .storage
        .list_sequence_versions(&default_tenant(), &namespace, &q.name)
        .await
        .map_err(|e| ApiError::from_storage(e, "sequence"))?;
    Ok(Json(versions))
}
