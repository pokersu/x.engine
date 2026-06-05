use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::error::ApiError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/metrics", get(prometheus_metrics))
}

async fn prometheus_metrics(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let handle = state
        .prometheus_handle
        .ok_or_else(|| ApiError::Unavailable("metrics not enabled".into()))?;
    let output = handle.render();
    Ok((
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        output,
    ))
}
