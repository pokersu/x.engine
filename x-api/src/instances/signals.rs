use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use x_types::ids::InstanceId;
use x_types::instance::InstanceState;
use x_types::signal::Signal;

use super::types::SendSignalRequest;
use crate::error::ApiError;
use crate::AppState;

pub async fn send_signal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SendSignalRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let instance_id = InstanceId::from_uuid(id);

    let signal = Signal {
        id: Uuid::now_v7(),
        instance_id,
        signal_type: req.signal_type,
        payload: req.payload,
        created_at: Utc::now(),
        delivered: false,
        delivered_at: None,
    };

    state
        .storage
        .enqueue_signal(&signal)
        .await
        .map_err(|e| ApiError::from_storage(e, "signal"))?;

    let _ = state
        .storage
        .update_instance_state(instance_id, InstanceState::Scheduled, Some(Utc::now()))
        .await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "signal_id": signal.id })),
    ))
}
