//! Prometheus metrics + health endpoints + debug state.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Duration;
use tokio::sync::oneshot;
use tower_http::timeout::TimeoutLayer;
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
    pub build_semaphore: std::sync::Arc<tokio::sync::Semaphore>,
    pub cargo_home: std::path::PathBuf,
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
        .route("/readyz", get(readiness_handler))
        .route("/debug/state", get(debug_state_handler))
        .merge(crate::http_api::router())
        .merge(crate::build_api::router())
        .merge(crate::ui::router())
        .merge(crate::ws::router())
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(60),
        ))
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

async fn readiness_handler(State(s): State<ApiState>) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    // Check 1: SQLite reachable
    if let Err(e) = sqlx::query("SELECT 1").fetch_one(&s.ctx.db).await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "status": "not ready",
                "reason": "db",
                "error": e.to_string(),
            })),
        )
            .into_response();
    }

    // Check 2: Wasm pool has slots
    if s.ctx.wasm_pool.available() == 0 {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "status": "not ready",
                "reason": "wasm_pool_saturated",
                "available": 0,
            })),
        )
            .into_response();
    }

    // Check 3: registry loaded (at least 0 is OK, but no panic)
    let module_count = s.registry.all().len();

    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "status": "ready",
            "modules_loaded": module_count,
            "wasm_pool_available": s.ctx.wasm_pool.available(),
            "uptime_secs": s.started_at.elapsed().as_secs(),
        })),
    )
        .into_response()
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
