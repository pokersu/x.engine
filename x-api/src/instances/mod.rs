pub mod lifecycle;
pub mod outputs;
pub mod signals;
pub mod types;

use axum::Router;

pub fn routes() -> Router<crate::AppState> {
    Router::new()
        .route("/instances", axum::routing::post(lifecycle::create_instance))
        .route("/instances/batch", axum::routing::post(lifecycle::create_instances_batch))
        .route("/instances", axum::routing::get(lifecycle::list_instances))
        .route("/instances/{id}", axum::routing::get(lifecycle::get_instance))
        .route("/instances/{id}/state", axum::routing::patch(lifecycle::update_state))
        .route("/instances/{id}/context", axum::routing::patch(lifecycle::update_context))
        .route("/instances/{id}/retry", axum::routing::post(lifecycle::retry_instance))
        .route("/instances/{id}/signals", axum::routing::post(signals::send_signal))
        .route("/instances/{id}/outputs", axum::routing::get(outputs::get_outputs))
        .route("/instances/{id}/tree", axum::routing::get(outputs::get_execution_tree))
}
