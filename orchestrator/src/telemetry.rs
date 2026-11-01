//! Prometheus metrics + health endpoints + debug state.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tokio::sync::oneshot;
use tracing::error;

use crate::context::AppContext;
use crate::module::OrchestratorModule;
use crate::module_registry::ModuleRegistry;

#[derive(Clone)]
pub struct ApiState {
    pub prom: PrometheusHandle,
    pub ctx: Arc<AppContext>,
    pub registry: Arc<ModuleRegistry>,
    pub started_at: Instant,
    pub modules_dir: std::path::PathBuf,
    pub tenants: std::sync::Arc<crate::tenant::TenantRegistry>,
}

pub fn install() -> anyhow::Result<PrometheusHandle> {
    PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("install metrics recorder: {e}"))
}

pub async fn spawn(
    bind: SocketAddr,
    state: ApiState,
    shutdown: oneshot::Receiver<()>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| anyhow::anyhow!("metrics bind {bind}: {e}"))?;

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/healthz", get(health_handler))
        .route("/readyz", get(health_handler))
        .route("/debug/state", get(debug_state_handler))
        .merge(crate::http_api::router())
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

async fn metrics_handler(State(s): State<ApiState>) -> String {
    s.prom.render()
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn debug_state_handler(State(s): State<ApiState>) -> Json<serde_json::Value> {
    let modules: Vec<serde_json::Value> = s
        .registry
        .all()
        .iter()
        .map(|(tenant, id, m)| {
            serde_json::json!({
                "tenant": tenant,
                "id": id,
                "version": m.manifest().version,
                "capabilities": m.manifest().capabilities,
            })
        })
        .collect();

    Json(serde_json::json!({
        "uptime_secs": s.started_at.elapsed().as_secs(),
        "modules": modules,
        "wasm_pool": {
            "total": s.ctx.wasm_pool.total(),
            "available": s.ctx.wasm_pool.available(),
        },
        "sockets": {
            "active": s.ctx.sockets.len(),
            "max": s.ctx.sockets.max(),
            "available": s.ctx.sockets.available(),
        },
    }))
}
