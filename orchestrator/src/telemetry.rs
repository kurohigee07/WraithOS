//! Prometheus metrics + health endpoints. Bind 127.0.0.1 default;
//! override via `metrics_bind` di config.

use std::net::SocketAddr;

use axum::extract::State;
use axum::routing::get;
use axum::Router;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tokio::sync::oneshot;
use tracing::error;

pub fn install() -> anyhow::Result<PrometheusHandle> {
    PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("install metrics recorder: {e}"))
}

pub async fn spawn(
    bind: SocketAddr,
    state: PrometheusHandle,
    shutdown: oneshot::Receiver<()>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| anyhow::anyhow!("metrics bind {bind}: {e}"))?;

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/healthz", get(health_handler))
        .route("/readyz", get(health_handler))
        .with_state(state);

    let handle = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown.await;
        });
        if let Err(e) = server.await {
            error!(err = %e, "metrics server error");
        }
    });

    Ok(handle)
}

async fn metrics_handler(State(handle): State<PrometheusHandle>) -> String {
    handle.render()
}

async fn health_handler() -> &'static str {
    "ok"
}
