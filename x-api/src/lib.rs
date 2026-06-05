pub mod error;
pub mod health;
pub mod instances;
pub mod metrics;
pub mod request_id;
pub mod sequences;
pub mod streaming;
pub mod test_harness;
pub mod webhooks;

use std::sync::Arc;

use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
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
    /// Prometheus metrics handle. `None` if metrics are not enabled.
    pub prometheus_handle: Option<PrometheusHandle>,
}

impl AppState {
    pub fn new(
        storage: Arc<dyn StorageBackend>,
        cancel: CancellationToken,
        prometheus_handle: Option<PrometheusHandle>,
    ) -> Self {
        Self {
            storage,
            shutdown: cancel,
            max_context_bytes: 256 * 1024,
            stream_limiter: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_STREAMS)),
            prometheus_handle,
        }
    }
}

/// Assemble the versioned API sub-router (without state applied).
fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(sequences::routes())
        .merge(instances::routes())
        .merge(streaming::routes())
}

/// Build the axum router with all routes.
pub fn build_router(state: AppState) -> Router {
    let api = api_routes();

    let mut root = Router::new()
        .nest(API_V1_PREFIX, api.clone())
        .merge(api)
        .merge(health::routes())
        .merge(webhooks::public_routes());

    // Only mount /metrics when Prometheus is configured.
    if state.prometheus_handle.is_some() {
        root = root.merge(metrics::routes());
    }

    root.with_state(state)
}
