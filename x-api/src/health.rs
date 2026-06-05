use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
}

/// Liveness probe: returns 200 if the process is running.

pub(crate) async fn liveness() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness probe: returns 200 if the database is reachable.

pub(crate) async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    match state.storage.ping().await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}
