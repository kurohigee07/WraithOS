//! Module lifecycle HTTP API: upload, replace, delete, list, execute.
//!
//! Routes (localhost-only via metrics_bind = 127.0.0.1:9090):
//!   GET    /modules              — list modules
//!   POST   /modules              — upload (409 if ID exists)
//!                                 headers: X-Module-Id, X-Module-Signature (if keys set)
//!                                 body: raw .wasm bytes
//!   PUT    /modules/{id}         — replace (hot-swap)
//!   DELETE /modules/{id}         — unload + remove file
//!   POST   /modules/{id}/execute — invoke with JSON payload
//!
//! Auth: none (localhost binding is the boundary). M17 multi-tenant akan
//! tambahkan bearer token per tenant.
//!
//! M17 seed: Modul disimpan sebagai `{modules_dir}/{id}.wasm`. Field
//! `tenant` di manifest sudah ada (default "default"), belum di-route.

use std::fs::File;
use std::io::Write;
use std::path::Path as FsPath;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use shared_types::ModuleManifest;
use tracing::{info, warn};

use crate::module::OrchestratorModule;
use crate::telemetry::ApiState;
use crate::wasm::module::WasmModule;

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/modules", get(list_handler).post(upload_handler))
        .route("/modules/{id}", put(replace_handler).delete(delete_handler))
        .route("/modules/{id}/execute", post(execute_handler))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
}

// ─────────────────────────────────────────────────────────────────────
#[derive(Debug)]
enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

// ─────────────────────────────────────────────────────────────────────
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
            })
        })
        .collect();
    Json(serde_json::json!({ "count": modules.len(), "modules": modules }))
}

// ─────────────────────────────────────────────────────────────────────
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
    let module = load_module(&s, &id, &body)?;

    let path = s.modules_dir.join(format!("{id}.wasm"));
    atomic_write(&path, &body).map_err(|e| ApiError::Internal(format!("disk: {e}")))?;

    if s.registry.insert_if_absent(id.clone(), Arc::new(module)).is_err() {
        let _ = std::fs::remove_file(&path);
        return Err(ApiError::Conflict(format!(
            "module '{id}' already exists (race)"
        )));
    }

    info!(id = %id, bytes = body.len(), "module uploaded");
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "status": "created" })),
    ))
}

// ─────────────────────────────────────────────────────────────────────
async fn replace_handler(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    verify_if_needed(&s, &headers, &body)?;
    let module = load_module(&s, &id, &body)?;

    let path = s.modules_dir.join(format!("{id}.wasm"));
    atomic_write(&path, &body).map_err(|e| ApiError::Internal(format!("disk: {e}")))?;

    let previous = s.registry.insert_replace(id.clone(), Arc::new(module));
    let status = if previous.is_some() { "replaced" } else { "created" };

    info!(id = %id, bytes = body.len(), status, "module put");
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "id": id, "status": status })),
    ))
}

// ─────────────────────────────────────────────────────────────────────
async fn delete_handler(
    State(s): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;

    let removed = s
        .registry
        .remove(&id)
        .ok_or_else(|| ApiError::NotFound(format!("module '{id}' not found")))?;
    drop(removed);

    let path = s.modules_dir.join(format!("{id}.wasm"));
    if let Err(e) = std::fs::remove_file(&path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!(id = %id, err = %e, "delete: file removal failed (registry already cleared)");
        }
    }

    info!(id = %id, "module deleted");
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "id": id, "status": "deleted" })),
    ))
}

// ─────────────────────────────────────────────────────────────────────
async fn execute_handler(
    State(s): State<ApiState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let module = s
        .registry
        .get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("module '{id}' not found")))?;
    let result = module
        .execute(s.ctx.clone(), payload)
        .await
        .map_err(|e| ApiError::Internal(format!("execute: {e}")))?;
    Ok(Json(result))
}

// ─────────────────────────────────────────────────────────────────────
fn extract_id(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("x-module-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::BadRequest("X-Module-Id header required".into()))
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

fn load_module(s: &ApiState, id: &str, body: &[u8]) -> Result<WasmModule, ApiError> {
    let manifest = ModuleManifest {
        id: id.to_string(),
        version: "0.0.0".to_string(),
        capabilities: vec![],
        tenant: "default".to_string(),
    };
    WasmModule::load(s.registry.engine(), manifest, body)
        .map_err(|e| ApiError::BadRequest(format!("invalid wasm: {e}")))
}

// ─────────────────────────────────────────────────────────────────────
/// Atomic write: temp file + fsync + rename + dir fsync (durability).
fn atomic_write(path: &FsPath, data: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir"))?;
    std::fs::create_dir_all(parent)?;

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(".upload-{}-{}.tmp", std::process::id(), n));

    let result = (|| -> std::io::Result<()> {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)?;
        if let Ok(d) = File::open(parent) {
            let _ = d.sync_all();
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
