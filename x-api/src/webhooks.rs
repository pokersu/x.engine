use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use x_types::ids::InstanceId;

use crate::error::ApiError;
use crate::AppState;

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/webhooks/events/{slug}", post(receive_webhook_event))
}

#[derive(Deserialize)]
pub struct WebhookPayload {
    pub body: serde_json::Value,
    pub headers: Option<serde_json::Value>,
}

pub async fn receive_webhook_event(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .storage
        .create_instance(&x_types::instance::TaskInstance {
            id: InstanceId::new(),
            sequence_id: x_types::ids::SequenceId::new(),
            tenant_id: x_types::ids::TenantId::unchecked("default"),
            namespace: x_types::ids::Namespace::new("default"),
            state: x_types::instance::InstanceState::Scheduled,
            next_fire_at: Some(chrono::Utc::now()),
            priority: x_types::instance::Priority::default(),
            timezone: "UTC".into(),
            metadata: serde_json::json!({"trigger": "webhook", "slug": slug, "payload": payload}),
            context: x_types::context::ExecutionContext::default(),
            concurrency_key: None,
            max_concurrency: None,
            idempotency_key: None,
            session_id: None,
            parent_instance_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .await
        .map_err(|e| ApiError::from_storage(e, "instance"))?;

    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"status": "accepted"}))))
}
