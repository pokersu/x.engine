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
    let outputs = state
        .storage
        .get_all_outputs(instance_id)
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;
    Ok(Json(outputs))
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
