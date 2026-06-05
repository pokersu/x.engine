//! Shared integration-test bootstrap for `x-api`.
//!
//! Spins up the full axum router backed by an in-memory SQLite storage on an
//! ephemeral port. Integration tests in `x-api/tests/*.rs` consume
//! [`spawn_test_server`] directly so that routing + response shapes are
//! exercised end-to-end.

use std::sync::Arc;

use axum::Router;
use x_storage::sqlite::SqliteStorage;
use tokio_util::sync::CancellationToken;

use crate::{build_router, AppState};

/// A running test server. The listener is bound to `127.0.0.1:0` so tests
/// can run concurrently without port collisions.
pub struct TestServer {
    /// Root URL of the server (e.g. `http://127.0.0.1:12345`).
    pub base_url: String,
    pub shutdown: CancellationToken,
    pub storage: Arc<x_storage::sqlite::SqliteStorage>,
}

impl TestServer {
    /// Returns the canonical versioned base URL (`{base_url}/api/v1`).
    pub fn v1_url(&self) -> String {
        format!("{}{}", self.base_url, crate::API_V1_PREFIX)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

/// Spawn a fully-wired API server on an ephemeral loopback port.
///
/// # Panics
/// Panics if the in-memory storage cannot be initialised or the TCP listener
/// fails to bind.
pub async fn spawn_test_server() -> TestServer {
    let storage = Arc::new(
        SqliteStorage::in_memory()
            .await
            .expect("in-memory sqlite storage must initialise for tests"),
    );
    let shutdown = CancellationToken::new();
    let state = AppState::new(storage.clone(), shutdown.clone(), None);

    let app: Router = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port for test server");
    let addr = listener.local_addr().expect("test listener has local addr");
    let base_url = format!("http://{addr}");

    let cancel_child = shutdown.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = ready_tx.send(());
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move { cancel_child.cancelled().await })
            .await;
    });
    ready_rx
        .await
        .expect("test server task failed to start before first request");

    TestServer {
        base_url,
        shutdown,
        storage,
    }
}
