use std::sync::Arc;

use metrics_exporter_prometheus::PrometheusBuilder;
use x_engine::handlers::HandlerRegistry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let storage: Arc<dyn x_storage::StorageBackend> = Arc::new(
        x_storage::sqlite::SqliteStorage::file("./x.engine.db").await?,
    );

    // Set up the Prometheus metrics recorder.
    let prometheus_handle = {
        let recorder = PrometheusBuilder::new()
            .build_recorder();
        let handle = recorder.handle();
        metrics::set_global_recorder(Box::new(recorder))
            .map_err(|e| anyhow::anyhow!("failed to set metrics recorder: {e}"))?;
        handle
    };

    let mut handlers = HandlerRegistry::new();
    x_engine::handlers::builtin::register_builtins(&mut handlers);

    let cancel = tokio_util::sync::CancellationToken::new();
    let engine = Arc::new(x_engine::Engine::new(
        Arc::clone(&storage),
        x_types::config::SchedulerConfig::default(),
        handlers,
        cancel.clone(),
    ));

    // Start engine loop in background
    let engine_cancel = cancel.clone();
    tokio::spawn(async move {
        if let Err(e) = engine.run().await {
            tracing::error!(error = %e, "engine exited with error");
        }
    });

    let app_state = x_api::AppState::new(
        Arc::clone(&storage),
        cancel.clone(),
        Some(prometheus_handle),
    );
    let app = x_api::build_router(app_state);
    let app = app.layer(tower_http::cors::CorsLayer::permissive());
    let app = app.layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    tracing::info!("x.engine server starting on 127.0.0.1:8080");

    let shutdown_token = engine_cancel.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { shutdown_token.cancelled().await })
        .await?;

    Ok(())
}
