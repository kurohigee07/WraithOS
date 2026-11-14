//! Prometheus metrics + health endpoints + debug state.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::Request;
use axum::extract::State;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{
    extract::DefaultBodyLimit,Json, Router};
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
    pub require_auth: bool,
    pub enable_build: bool,
    pub enable_build_network: bool,
    pub data_dir: std::path::PathBuf,
    pub wizard: std::sync::Arc<crate::wizard::WizardState>,
}

pub fn install() -> anyhow::Result<PrometheusHandle> {
    PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("install metrics recorder: {e}"))
}

pub async fn auth_middleware(
    State(state): State<ApiState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Public endpoints: process liveness only
    if path == "/healthz" || path == "/readyz" {
        return next.run(request).await;
    }

    // Defensive fail-closed: if require_auth but no token configured,
    // do not silently allow. Startup should have caught this.
    if !state.tenants.default_has_auth() {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": "auth required but no token configured for default tenant"
            })),
        )
            .into_response();
    }

    // Tenant routes: /t/{tenant}/... must be verified against that
    // tenant's token, not the default tenant's. Otherwise legitimate
    // /t/alice requests with alice-token would be rejected here before
    // the handler's per-tenant verify_tenant even runs.
    let tenant_for_auth = match extract_tenant_from_path(path) {
        Some(t) => t,
        None => state.tenants.default_name(),
    };

    let auth = request.headers().get("authorization").cloned();
    match state.tenants.verify(tenant_for_auth, auth.as_ref()) {
        Ok(()) => next.run(request).await,
        Err(_) => (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({"error": "unauthorized"})),
        )
            .into_response(),
    }
}

/// Extract the tenant segment from /t/{tenant}/... routes.
/// Returns None for non-tenant routes.
fn extract_tenant_from_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/t/")?;
    let end = rest.find('/').unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

pub fn build_router(state: ApiState) -> Router {
    let require_auth = state.require_auth;
    let enable_build = state.enable_build;

    let mut router = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/healthz", get(health_handler))
        .route("/readyz", get(readiness_handler))
        .route("/debug/state", get(debug_state_handler))
        .merge(crate::http_api::router())
        .merge(crate::ui::router())
        .merge(crate::ws::router())
        .merge(crate::wizard::router());

    // Build pipeline off by default (arbitrary code execution risk)
    if enable_build {
        router = router.merge(crate::build_api::router());
    }

    // Auth middleware: conditional on require_auth
    if require_auth {
        router = router.layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));
    }

    router
        // 1 MiB default body limit for all routes that do not set
        // their own (wizard, ui, ws). http_api/build_api set their
        // own 10 MiB inner limit which takes precedence because
        // innermost DefaultBodyLimit wins at body-read time.
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            // Outer HTTP timeout must be strictly larger than the Wasm
            // epoch deadline (60 s = 600 ticks x 100 ms) so a spinning
            // guest hits the epoch trap and returns a clean 500 with
            // trap body, rather than racing the outer layer and
            // returning a bodyless 408.
            Duration::from_secs(65),
        ))
        .with_state(state)
}

pub async fn spawn(
    bind: SocketAddr,
    state: ApiState,
    shutdown: oneshot::Receiver<()>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| anyhow::anyhow!("metrics bind {bind}: {e}"))?;

    let app = build_router(state);

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
