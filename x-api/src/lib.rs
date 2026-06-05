pub mod error;
pub mod health;
pub mod instances;
pub mod metrics;
pub mod request_id;
pub mod sequences;
pub mod streaming;
pub mod webhooks;

use std::sync::Arc;

use axum::Router;
use x_storage::StorageBackend;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

/// Default ceiling on concurrent SSE streams per process.
pub const DEFAULT_MAX_CONCURRENT_STREAMS: usize = 256;

/// Canonical API version prefix.
pub const API_V1_PREFIX: &str = "/api/v1";

/// Generic paginated response envelope for list endpoints.
#[derive(Debug, serde::Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}

impl<T: serde::Serialize> PaginatedResponse<T> {
    pub fn from_vec(items: Vec<T>, limit: u32) -> Self {
        let has_more = u32::try_from(items.len()).unwrap_or(u32::MAX) >= limit && limit > 0;
        Self { items, has_more }
    }
}

/// Shared application state injected into all handlers.
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn StorageBackend>,
    pub shutdown: CancellationToken,
    pub max_context_bytes: u32,
    pub stream_limiter: Arc<Semaphore>,
}

impl AppState {
    pub fn new(storage: Arc<dyn StorageBackend>, cancel: CancellationToken) -> Self {
        Self {
            storage,
            shutdown: cancel,
            max_context_bytes: 256 * 1024,
            stream_limiter: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_STREAMS)),
        }
    }
}

/// Assemble the versioned API sub-router (without state applied).
fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(sequences::routes())
        .merge(instances::routes())
}

/// Build the axum router with all routes.
pub fn build_router(state: AppState) -> Router {
    let mut api = api_routes();

    Router::new()
        .nest(API_V1_PREFIX, api.clone())
        .merge(api)
        .with_state(state)
}
