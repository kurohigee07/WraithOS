//! Module lifecycle HTTP API (tenant-aware).
//!
//! Tenant routes: /t/{tenant}/modules...
//! Default routes: /modules...        (backward compat = default tenant)

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use shared_types::{ModuleManifest, ModuleQuota};
use tracing::{info, warn};

use crate::module::OrchestratorModule;
use crate::module_registry::write_quota_sidecar;
use crate::telemetry::ApiState;
use crate::tenant::AuthError;
use crate::wasm::module::{QuotaExecError, WasmModule};

pub fn router() -> Router<ApiState> {
    Router::new()
        // tenant routes
        .route("/t/{tenant}/modules", get(list_t).post(upload_t))
        .route("/t/{tenant}/modules/{id}", put(replace_t).delete(delete_t))
        .route("/t/{tenant}/modules/{id}/execute", post(execute_t))
        .route(
            "/t/{tenant}/modules/{id}/quota",
            get(get_quota_t).put(put_quota_t),
        )
        // default (backward compat)
        .route("/modules", get(list_d).post(upload_d))
        .route("/modules/{id}", put(replace_d).delete(delete_d))
        .route("/modules/{id}/execute", post(execute_d))
        .route("/modules/{id}/quota", get(get_quota_d).put(put_quota_d))
        .route("/audit/logs", get(audit_logs_handler))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
}

// ── ApiError ────────────────────────────────────────────────────────
#[derive(Debug)]
pub(crate) enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Unauthorized(String),
    RateLimited { retry_after_ms: u64 },
    TenantModuleLimit { current: usize, max: usize },
    Unavailable(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::BadRequest(m) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            ApiError::Conflict(m) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            ApiError::NotFound(m) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            ApiError::Unauthorized(m) => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            ApiError::RateLimited { retry_after_ms } => {
                let mut resp = (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(serde_json::json!({
                        "error": "rate limited",
                        "retry_after_ms": retry_after_ms,
                    })),
                )
                    .into_response();
                let secs = (retry_after_ms / 1000).max(1).to_string();
                if let Ok(h) = HeaderValue::from_str(&secs) {
                    resp.headers_mut().insert("retry-after", h);
                }
                resp
            }
            ApiError::TenantModuleLimit { current, max } => (
                StatusCode::INSUFFICIENT_STORAGE,
                Json(serde_json::json!({
                    "error": format!("tenant module limit reached ({current}/{max})"),
                    "current": current,
                    "max": max,
                })),
            )
                .into_response(),
            ApiError::Unavailable(m) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            ApiError::Internal(m) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
        }
    }
}

// ── Shared impls ────────────────────────────────────────────────────
async fn list_shared(s: &ApiState, tenant: &str) -> Json<serde_json::Value> {
    let modules: Vec<serde_json::Value> = s
        .registry
        .all_for_tenant(tenant)
        .iter()
        .map(|(id, m)| {
            let manifest = m.manifest();
            serde_json::json!({
                "tenant": manifest.tenant,
                "id": id,
                "version": manifest.version,
                "capabilities": manifest.capabilities,
                "quota": manifest.quota,
            })
        })
        .collect();
    Json(serde_json::json!({ "count": modules.len(), "modules": modules }))
}

async fn upload_shared(
    s: &ApiState,
    tenant: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let id = extract_id(headers)?;
    validate_id(&id)?;

    if s.registry.has(tenant, &id) {
        return Err(ApiError::Conflict(format!(
            "module '{id}' already exists in tenant '{tenant}'; use PUT to replace"
        )));
    }

    // M18: tenant module capacity
    let current_count = s.registry.all_for_tenant(tenant).len();
    if let Err((cur, max)) = s.tenants.check_capacity(tenant, current_count) {
        metrics::counter!(
            "tenant_quota_rejected_total",
            "tenant" => tenant.to_string(),
            "reason" => "module_limit",
        )
        .increment(1);
        return Err(ApiError::TenantModuleLimit { current: cur, max });
    }

    verify_if_needed(s, headers, &body)?;
    let quota = parse_quota_header(headers)?.unwrap_or_else(|| s.registry.default_quota());
    let quota_state = s
        .registry
        .build_quota_state(&quota)
        .map_err(|e| ApiError::BadRequest(format!("quota: {e}")))?;
    let module = load_module(s, tenant, &id, &body, &quota, quota_state)?;

    let tenant_dir = s.modules_dir.join(tenant);
    let path = tenant_dir.join(format!("{id}.wasm"));
    crate::fs_util::atomic_write(&path, &body)
        .map_err(|e| ApiError::Internal(format!("disk: {e}")))?;
    write_quota_sidecar(&tenant_dir, &id, &quota)
        .map_err(|e| ApiError::Internal(format!("sidecar: {e}")))?;

    if s.registry
        .insert_if_absent(tenant, &id, Arc::new(module))
        .is_err()
    {
        let _ = std::fs::remove_file(&path);
        return Err(ApiError::Conflict(format!(
            "module '{id}' already exists (race)"
        )));
    }

    info!(tenant = %tenant, id = %id, bytes = body.len(), "module uploaded");
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "tenant": tenant, "id": id, "status": "created", "quota": quota,
        })),
    )
        .into_response())
}

async fn replace_shared(
    s: &ApiState,
    tenant: &str,
    id: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    validate_id(id)?;
    verify_if_needed(s, headers, &body)?;

    let quota = match parse_quota_header(headers)? {
        Some(q) => q,
        None => s
            .registry
            .get(tenant, id)
            .map(|m| m.manifest().quota.clone())
            .unwrap_or_else(|| s.registry.default_quota()),
    };
    let quota_state = s
        .registry
        .build_quota_state(&quota)
        .map_err(|e| ApiError::BadRequest(format!("quota: {e}")))?;
    let module = load_module(s, tenant, id, &body, &quota, quota_state)?;

    let tenant_dir = s.modules_dir.join(tenant);
    let path = tenant_dir.join(format!("{id}.wasm"));
    crate::fs_util::atomic_write(&path, &body)
        .map_err(|e| ApiError::Internal(format!("disk: {e}")))?;
    write_quota_sidecar(&tenant_dir, id, &quota)
        .map_err(|e| ApiError::Internal(format!("sidecar: {e}")))?;

    let previous = s.registry.insert_replace(tenant, id, Arc::new(module));
    let status = if previous.is_some() {
        "replaced"
    } else {
        "created"
    };
    info!(tenant = %tenant, id = %id, bytes = body.len(), status, "module put");
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "tenant": tenant, "id": id, "status": status, "quota": quota,
        })),
    )
        .into_response())
}

async fn delete_shared(s: &ApiState, tenant: &str, id: &str) -> Result<Response, ApiError> {
    validate_id(id)?;
    let removed = s.registry.remove(tenant, id).ok_or_else(|| {
        ApiError::NotFound(format!("module '{id}' not found in tenant '{tenant}'"))
    })?;
    drop(removed);

    let tenant_dir = s.modules_dir.join(tenant);
    let wasm_path = tenant_dir.join(format!("{id}.wasm"));
    if let Err(e) = std::fs::remove_file(&wasm_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!(id = %id, err = %e, "delete: wasm file removal failed");
        }
    }
    let quota_path = tenant_dir.join(format!("{id}.wasm.quota.json"));
    if let Err(e) = std::fs::remove_file(&quota_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!(id = %id, err = %e, "delete: quota file removal failed");
        }
    }

    info!(tenant = %tenant, id = %id, "module deleted");
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "tenant": tenant, "id": id, "status": "deleted" })),
    )
        .into_response())
}

async fn execute_shared(
    s: &ApiState,
    tenant: &str,
    id: &str,
    payload: serde_json::Value,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_id(id)?;
    let module = s.registry.get(tenant, id).ok_or_else(|| {
        ApiError::NotFound(format!("module '{id}' not found in tenant '{tenant}'"))
    })?;

    // M18: tenant-level rate (before module quota — cheaper to reject here)
    if let Err(retry_after_ms) = s.tenants.try_acquire_rate(tenant) {
        metrics::counter!(
            "tenant_quota_rejected_total",
            "tenant" => tenant.to_string(),
            "reason" => "rate",
        )
        .increment(1);
        return Err(ApiError::RateLimited { retry_after_ms });
    }

    // M18: tenant-level concurrency (permit held via RAII until handler returns)
    let _tenant_permit = match s.tenants.acquire_concurrency(tenant).await {
        Ok(p) => p,
        Err(_) => {
            metrics::counter!(
                "tenant_quota_rejected_total",
                "tenant" => tenant.to_string(),
                "reason" => "concurrency",
            )
            .increment(1);
            return Err(ApiError::Unavailable(format!(
                "tenant '{tenant}' concurrency timeout"
            )));
        }
    };

    match module.execute_quota(s.ctx.clone(), payload).await {
        Ok(v) => Ok(Json(v)),
        Err(QuotaExecError::RateLimited { retry_after_ms }) => {
            Err(ApiError::RateLimited { retry_after_ms })
        }
        Err(QuotaExecError::ConcurrencyTimeout) => {
            Err(ApiError::Unavailable("module concurrency timeout".into()))
        }
        Err(QuotaExecError::Internal(e)) => Err(ApiError::Internal(format!("execute: {e}"))),
    }
}

async fn get_quota_shared(
    s: &ApiState,
    tenant: &str,
    id: &str,
) -> Result<Json<ModuleQuota>, ApiError> {
    validate_id(id)?;
    let module = s.registry.get(tenant, id).ok_or_else(|| {
        ApiError::NotFound(format!("module '{id}' not found in tenant '{tenant}'"))
    })?;
    Ok(Json(module.manifest().quota.clone()))
}

async fn put_quota_shared(
    s: &ApiState,
    tenant: &str,
    id: &str,
    new_quota: ModuleQuota,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_id(id)?;
    let old = s.registry.get(tenant, id).ok_or_else(|| {
        ApiError::NotFound(format!("module '{id}' not found in tenant '{tenant}'"))
    })?;

    s.registry
        .build_quota_state(&new_quota)
        .map_err(|e| ApiError::BadRequest(format!("quota: {e}")))?;

    let tenant_dir = s.modules_dir.join(tenant);
    let wasm_path = tenant_dir.join(format!("{id}.wasm"));
    let bytes = std::fs::read(&wasm_path)
        .map_err(|e| ApiError::Internal(format!("read {}: {e}", wasm_path.display())))?;

    let new_manifest = ModuleManifest {
        id: id.to_string(),
        version: old.manifest().version.clone(),
        capabilities: old.manifest().capabilities.clone(),
        tenant: tenant.to_string(),
        quota: new_quota.clone(),
    };
    let qs = s
        .registry
        .build_quota_state(&new_quota)
        .map_err(|e| ApiError::Internal(format!("build quota state: {e}")))?;
    let new_module = WasmModule::load(s.registry.engine(), new_manifest, &bytes, qs)
        .map_err(|e| ApiError::Internal(format!("reload: {e}")))?;

    write_quota_sidecar(&tenant_dir, id, &new_quota)
        .map_err(|e| ApiError::Internal(format!("write sidecar: {e}")))?;

    s.registry.insert_replace(tenant, id, Arc::new(new_module));
    info!(tenant = %tenant, id = %id, ?new_quota, "quota updated");
    Ok(Json(
        serde_json::json!({ "tenant": tenant, "id": id, "quota": new_quota }),
    ))
}

// ── Helpers ─────────────────────────────────────────────────────────
pub(crate) fn verify_tenant(
    s: &ApiState,
    tenant: &str,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    match s.tenants.verify(tenant, headers.get("authorization")) {
        Ok(()) => Ok(()),
        Err(AuthError::UnknownTenant) => {
            Err(ApiError::NotFound(format!("tenant '{tenant}' not found")))
        }
        Err(AuthError::MissingToken) => Err(ApiError::Unauthorized(
            "missing Authorization: Bearer".into(),
        )),
        Err(AuthError::BadToken) => Err(ApiError::Unauthorized("invalid token".into())),
        Err(AuthError::Internal) => Err(ApiError::Internal("auth internal".into())),
    }
}

fn extract_id(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("x-module-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::BadRequest("X-Module-Id header required".into()))
}

pub(crate) fn parse_quota_header(headers: &HeaderMap) -> Result<Option<ModuleQuota>, ApiError> {
    let raw = match headers.get("x-module-quota") {
        Some(v) => v,
        None => return Ok(None),
    };
    let s = raw
        .to_str()
        .map_err(|e| ApiError::BadRequest(format!("X-Module-Quota not utf8: {e}")))?;
    let q: ModuleQuota = serde_json::from_str(s)
        .map_err(|e| ApiError::BadRequest(format!("X-Module-Quota not json: {e}")))?;
    Ok(Some(q))
}

pub(crate) fn validate_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty() || id.len() > 64 {
        return Err(ApiError::BadRequest(
            "module id must be 1..=64 chars".into(),
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::BadRequest(
            "module id must be [a-zA-Z0-9_-]".into(),
        ));
    }
    Ok(())
}

fn verify_if_needed(s: &ApiState, headers: &HeaderMap, body: &[u8]) -> Result<(), ApiError> {
    if !s.registry.has_trusted_keys() {
        return Ok(());
    }
    let sig_hex = headers
        .get("x-module-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::BadRequest(
                "X-Module-Signature required (trusted_signer_keys configured)".into(),
            )
        })?;
    let sig_bytes = hex::decode(sig_hex)
        .map_err(|e| ApiError::BadRequest(format!("invalid signature hex: {e}")))?;
    s.registry
        .verify_signature(body, &sig_bytes)
        .map_err(|e| ApiError::BadRequest(format!("signature: {e}")))
}

fn load_module(
    s: &ApiState,
    tenant: &str,
    id: &str,
    body: &[u8],
    quota: &ModuleQuota,
    quota_state: Arc<crate::quota::ModuleQuotaState>,
) -> Result<WasmModule, ApiError> {
    let manifest = ModuleManifest {
        id: id.to_string(),
        version: "0.0.0".to_string(),
        capabilities: vec![],
        tenant: tenant.to_string(),
        quota: quota.clone(),
    };
    WasmModule::load(s.registry.engine(), manifest, body, quota_state)
        .map_err(|e| ApiError::BadRequest(format!("invalid wasm: {e}")))
}

// ── Tenant handlers ─────────────────────────────────────────────────
async fn list_t(
    State(s): State<ApiState>,
    Path(tenant): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    verify_tenant(&s, &tenant, &headers)?;
    Ok(list_shared(&s, &tenant).await)
}

async fn upload_t(
    State(s): State<ApiState>,
    Path(tenant): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    verify_tenant(&s, &tenant, &headers)?;
    upload_shared(&s, &tenant, &headers, body).await
}

async fn replace_t(
    State(s): State<ApiState>,
    Path((tenant, id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    verify_tenant(&s, &tenant, &headers)?;
    replace_shared(&s, &tenant, &id, &headers, body).await
}

async fn delete_t(
    State(s): State<ApiState>,
    Path((tenant, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    verify_tenant(&s, &tenant, &headers)?;
    delete_shared(&s, &tenant, &id).await
}

async fn execute_t(
    State(s): State<ApiState>,
    Path((tenant, id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    verify_tenant(&s, &tenant, &headers)?;
    execute_shared(&s, &tenant, &id, payload).await
}

async fn get_quota_t(
    State(s): State<ApiState>,
    Path((tenant, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<ModuleQuota>, ApiError> {
    verify_tenant(&s, &tenant, &headers)?;
    get_quota_shared(&s, &tenant, &id).await
}

async fn put_quota_t(
    State(s): State<ApiState>,
    Path((tenant, id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(new_quota): Json<ModuleQuota>,
) -> Result<Json<serde_json::Value>, ApiError> {
    verify_tenant(&s, &tenant, &headers)?;
    put_quota_shared(&s, &tenant, &id, new_quota).await
}

// ── Default handlers (backward compat) ──────────────────────────────
async fn list_d(State(s): State<ApiState>) -> Json<serde_json::Value> {
    let tenant = s.tenants.default_name().to_string();
    list_shared(&s, &tenant).await
}

async fn upload_d(
    State(s): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let tenant = s.tenants.default_name().to_string();
    // Default tenant typically has no auth; still check if configured.
    verify_tenant(&s, &tenant, &headers)?;
    upload_shared(&s, &tenant, &headers, body).await
}

async fn replace_d(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let tenant = s.tenants.default_name().to_string();
    verify_tenant(&s, &tenant, &headers)?;
    replace_shared(&s, &tenant, &id, &headers, body).await
}

async fn delete_d(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let tenant = s.tenants.default_name().to_string();
    verify_tenant(&s, &tenant, &headers)?;
    delete_shared(&s, &tenant, &id).await
}

async fn execute_d(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tenant = s.tenants.default_name().to_string();
    verify_tenant(&s, &tenant, &headers)?;
    execute_shared(&s, &tenant, &id, payload).await
}

async fn get_quota_d(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ModuleQuota>, ApiError> {
    let tenant = s.tenants.default_name().to_string();
    verify_tenant(&s, &tenant, &headers)?;
    get_quota_shared(&s, &tenant, &id).await
}

async fn put_quota_d(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(new_quota): Json<ModuleQuota>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tenant = s.tenants.default_name().to_string();
    verify_tenant(&s, &tenant, &headers)?;
    put_quota_shared(&s, &tenant, &id, new_quota).await
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct ExecutionLog {
    id: i64,
    tenant: String,
    module_id: String,
    status: String,
    duration_ms: i64,
    error_message: Option<String>,
    executed_at: String,
}

async fn audit_logs_handler(
    State(s): State<ApiState>,
) -> Result<Json<Vec<ExecutionLog>>, ApiError> {
    let logs = sqlx::query_as::<_, ExecutionLog>(
        "SELECT id, tenant, module_id, status, duration_ms, error_message, executed_at 
         FROM execution_logs 
         ORDER BY id DESC 
         LIMIT 50",
    )
    .fetch_all(&s.ctx.db)
    .await
    .map_err(|e| ApiError::Internal(format!("db query: {e}")))?;

    Ok(Json(logs))
}
