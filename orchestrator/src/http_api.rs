//! Module lifecycle HTTP API + per-module quota management.

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
use crate::wasm::module::{QuotaExecError, WasmModule};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/modules", get(list_handler).post(upload_handler))
        .route("/modules/{id}", put(replace_handler).delete(delete_handler))
        .route("/modules/{id}/execute", post(execute_handler))
        .route(
            "/modules/{id}/quota",
            get(get_quota_handler).put(put_quota_handler),
        )
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
}

// ── ApiError ────────────────────────────────────────────────────────
#[derive(Debug)]
enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    RateLimited { retry_after_ms: u64 },
    Unavailable(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::BadRequest(m) => {
                (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": m }))).into_response()
            }
            ApiError::Conflict(m) => {
                (StatusCode::CONFLICT, Json(serde_json::json!({ "error": m }))).into_response()
            }
            ApiError::NotFound(m) => {
                (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": m }))).into_response()
            }
            ApiError::RateLimited { retry_after_ms } => {
                let mut resp = (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(serde_json::json!({
                        "error": "rate limited",
                        "retry_after_ms": retry_after_ms,
                    })),
                ).into_response();
                let secs = (retry_after_ms / 1000).max(1).to_string();
                if let Ok(h) = HeaderValue::from_str(&secs) {
                    resp.headers_mut().insert("retry-after", h);
                }
                resp
            }
            ApiError::Unavailable(m) => {
                (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({ "error": m })))
                    .into_response()
            }
            ApiError::Internal(m) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": m })),
            ).into_response(),
        }
    }
}

// ── Handlers ────────────────────────────────────────────────────────
async fn list_handler(State(s): State<ApiState>) -> Json<serde_json::Value> {
    let modules: Vec<serde_json::Value> = s
        .registry
        .all()
        .iter()
        .map(|(id, m)| {
            let manifest = m.manifest();
            serde_json::json!({
                "id": id,
                "version": manifest.version,
                "tenant": manifest.tenant,
                "capabilities": manifest.capabilities,
                "quota": manifest.quota,
            })
        })
        .collect();
    Json(serde_json::json!({ "count": modules.len(), "modules": modules }))
}

async fn upload_handler(
    State(s): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let id = extract_id(&headers)?;
    validate_id(&id)?;

    if s.registry.has(&id) {
        return Err(ApiError::Conflict(format!(
            "module '{id}' already exists; use PUT /modules/{id} to replace"
        )));
    }

    verify_if_needed(&s, &headers, &body)?;
    let quota = parse_quota_header(&headers)?.unwrap_or_else(|| s.registry.default_quota());
    let quota_state = s
        .registry
        .build_quota_state(&quota)
        .map_err(|e| ApiError::BadRequest(format!("quota: {e}")))?;
    let module = load_module(&s, &id, &body, &quota, quota_state)?;

    let path = s.modules_dir.join(format!("{id}.wasm"));
    crate::fs_util::atomic_write(&path, &body)
        .map_err(|e| ApiError::Internal(format!("disk: {e}")))?;
    write_quota_sidecar(&s.modules_dir, &id, &quota)
        .map_err(|e| ApiError::Internal(format!("quota sidecar: {e}")))?;

    if s.registry.insert_if_absent(id.clone(), Arc::new(module)).is_err() {
        let _ = std::fs::remove_file(&path);
        return Err(ApiError::Conflict(format!("module '{id}' already exists (race)")));
    }

    info!(id = %id, bytes = body.len(), "module uploaded");
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "status": "created", "quota": quota })),
    ))
}

async fn replace_handler(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    verify_if_needed(&s, &headers, &body)?;

    let quota = match parse_quota_header(&headers)? {
        Some(q) => q,
        None => {
            // Preserve existing quota if PUT body doesn't specify one
            s.registry
                .get(&id)
                .map(|m| m.manifest().quota.clone())
                .unwrap_or_else(|| s.registry.default_quota())
        }
    };
    let quota_state = s
        .registry
        .build_quota_state(&quota)
        .map_err(|e| ApiError::BadRequest(format!("quota: {e}")))?;
    let module = load_module(&s, &id, &body, &quota, quota_state)?;

    let path = s.modules_dir.join(format!("{id}.wasm"));
    crate::fs_util::atomic_write(&path, &body)
        .map_err(|e| ApiError::Internal(format!("disk: {e}")))?;
    write_quota_sidecar(&s.modules_dir, &id, &quota)
        .map_err(|e| ApiError::Internal(format!("quota sidecar: {e}")))?;

    let previous = s.registry.insert_replace(id.clone(), Arc::new(module));
    let status = if previous.is_some() { "replaced" } else { "created" };
    info!(id = %id, bytes = body.len(), status, "module put");
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "id": id, "status": status, "quota": quota })),
    ))
}

async fn delete_handler(
    State(s): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let removed = s.registry.remove(&id).ok_or_else(|| {
        ApiError::NotFound(format!("module '{id}' not found"))
    })?;
    drop(removed);

    let wasm_path = s.modules_dir.join(format!("{id}.wasm"));
    if let Err(e) = std::fs::remove_file(&wasm_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!(id = %id, err = %e, "delete: wasm file removal failed");
        }
    }
    let quota_path = s.modules_dir.join(format!("{id}.wasm.quota.json"));
    if let Err(e) = std::fs::remove_file(&quota_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!(id = %id, err = %e, "delete: quota file removal failed");
        }
    }

    info!(id = %id, "module deleted");
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "id": id, "status": "deleted" })),
    ))
}

async fn execute_handler(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let module = s.registry.get(&id).ok_or_else(|| {
        ApiError::NotFound(format!("module '{id}' not found"))
    })?;
    match module.execute_quota(s.ctx.clone(), payload).await {
        Ok(v) => Ok(Json(v)),
        Err(QuotaExecError::RateLimited { retry_after_ms }) => {
            Err(ApiError::RateLimited { retry_after_ms })
        }
        Err(QuotaExecError::ConcurrencyTimeout) => {
            Err(ApiError::Unavailable("module concurrency timeout".into()))
        }
        Err(QuotaExecError::Internal(e)) => {
            Err(ApiError::Internal(format!("execute: {e}")))
        }
    }
}

async fn get_quota_handler(
    State(s): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let module = s.registry.get(&id).ok_or_else(|| {
        ApiError::NotFound(format!("module '{id}' not found"))
    })?;
    Ok(Json(module.manifest().quota.clone()))
}

async fn put_quota_handler(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    Json(new_quota): Json<ModuleQuota>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;

    let old_module = s.registry.get(&id).ok_or_else(|| {
        ApiError::NotFound(format!("module '{id}' not found"))
    })?;

    // Validate quota against global ceiling
    s.registry
        .build_quota_state(&new_quota)
        .map_err(|e| ApiError::BadRequest(format!("quota: {e}")))?;

    // Read .wasm from disk and reload with new quota state
    let wasm_path = s.modules_dir.join(format!("{id}.wasm"));
    let wasm_bytes = std::fs::read(&wasm_path)
        .map_err(|e| ApiError::Internal(format!("read {}: {e}", wasm_path.display())))?;

    let new_manifest = ModuleManifest {
        id: id.clone(),
        version: old_module.manifest().version.clone(),
        capabilities: old_module.manifest().capabilities.clone(),
        tenant: old_module.manifest().tenant.clone(),
        quota: new_quota.clone(),
    };
    let quota_state = s
        .registry
        .build_quota_state(&new_quota)
        .map_err(|e| ApiError::Internal(format!("build quota state: {e}")))?;
    let new_module = WasmModule::load(
        s.registry.engine(),
        new_manifest,
        &wasm_bytes,
        quota_state,
    )
    .map_err(|e| ApiError::Internal(format!("reload: {e}")))?;

    // Persist sidecar BEFORE swapping registry (rollback if write fails)
    write_quota_sidecar(&s.modules_dir, &id, &new_quota)
        .map_err(|e| ApiError::Internal(format!("write sidecar: {e}")))?;

    s.registry.insert_replace(id.clone(), Arc::new(new_module));

    info!(id = %id, ?new_quota, "quota updated");
    Ok(Json(serde_json::json!({ "id": id, "quota": new_quota })))
}

// ── Helpers ─────────────────────────────────────────────────────────
fn extract_id(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("x-module-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::BadRequest("X-Module-Id header required".into()))
}

fn parse_quota_header(headers: &HeaderMap) -> Result<Option<ModuleQuota>, ApiError> {
    let raw = match headers.get("x-module-quota") {
        Some(v) => v,
        None => return Ok(None),
    };
    let s = raw.to_str().map_err(|e| {
        ApiError::BadRequest(format!("X-Module-Quota not utf8: {e}"))
    })?;
    let q: ModuleQuota = serde_json::from_str(s).map_err(|e| {
        ApiError::BadRequest(format!("X-Module-Quota not json: {e}"))
    })?;
    Ok(Some(q))
}

fn validate_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty() || id.len() > 64 {
        return Err(ApiError::BadRequest("module id must be 1..=64 chars".into()));
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ApiError::BadRequest("module id must be [a-zA-Z0-9_-]".into()));
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
        .ok_or_else(|| ApiError::BadRequest(
            "X-Module-Signature required (trusted_signer_keys configured)".into()
        ))?;
    let sig_bytes = hex::decode(sig_hex)
        .map_err(|e| ApiError::BadRequest(format!("invalid signature hex: {e}")))?;
    s.registry
        .verify_signature(body, &sig_bytes)
        .map_err(|e| ApiError::BadRequest(format!("signature: {e}")))
}

fn load_module(
    s: &ApiState,
    id: &str,
    body: &[u8],
    quota: &ModuleQuota,
    quota_state: Arc<crate::quota::ModuleQuotaState>,
) -> Result<WasmModule, ApiError> {
    let manifest = ModuleManifest {
        id: id.to_string(),
        version: "0.0.0".to_string(),
        capabilities: vec![],
        tenant: "default".to_string(),
        quota: quota.clone(),
    };
    WasmModule::load(s.registry.engine(), manifest, body, quota_state)
        .map_err(|e| ApiError::BadRequest(format!("invalid wasm: {e}")))
}
