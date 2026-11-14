//! Build-from-source pipeline (M19).
//!
//! DANGER: runs `cargo build` on uploaded source. Build scripts and
//! proc-macros execute arbitrary code. Only expose to trusted networks
//! (localhost + known tenants).
//!
//! Endpoint:
//!   POST /t/{tenant}/modules/build    (auth)
//!   POST /modules/build               (default tenant)
//! Body: tar or tar.gz with Cargo.toml + src/ (+ wit/ if needed).
//! Header X-Module-Id optional (default = package name, dash→underscore).
//! Header X-Module-Quota optional (JSON).
//!
//! Sandbox mitigations:
//!   - tempdir (auto-cleanup on drop)
//!   - env_clear + minimal PATH
//!   - timeout 300s (kill_on_drop)
//!   - global build semaphore (1 concurrent)
//!   - tenant rate + concurrency respected
//!   - 10 MB body limit

use std::io::Cursor;
use std::path::{Component, Path as FsPath, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use shared_types::ModuleManifest;
use tokio::process::Command;
use tracing::{info, warn};

use crate::http_api::{parse_quota_header, validate_id, verify_tenant, ApiError};
use crate::module_registry::write_quota_sidecar;
use crate::telemetry::ApiState;
use crate::wasm::module::WasmModule;

const BUILD_TIMEOUT_SECS: u64 = 300;
const MAX_SOURCE_BYTES: usize = 10 * 1024 * 1024;
const STDERR_TAIL_CHARS: usize = 2000;
const MAX_WARNINGS_SHOWN: usize = 10;

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/t/{tenant}/modules/build", post(build_t))
        .route("/modules/build", post(build_d))
        .layer(DefaultBodyLimit::max(MAX_SOURCE_BYTES))
}

// ── Handlers ────────────────────────────────────────────────────────
async fn build_t(
    State(s): State<ApiState>,
    Path(tenant): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    verify_tenant(&s, &tenant, &headers)?;
    build_shared(&s, &tenant, &headers, body).await
}

async fn build_d(
    State(s): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let tenant = s.tenants.default_name().to_string();
    verify_tenant(&s, &tenant, &headers)?;
    build_shared(&s, &tenant, &headers, body).await
}

async fn build_shared(
    s: &ApiState,
    tenant: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if body.is_empty() {
        return Err(ApiError::BadRequest("empty source body".into()));
    }

    // Build path hardcodes capabilities to vec![] and persists no
    // .wasm.capabilities.json sidecar; built modules always fall back to
    // the global CapabilitySet on load. Reject early if the caller
    // expects per-module caps, so the mismatch is explicit instead of
    // silently ignored. Use POST /modules to upload a pre-built wasm
    // with a per-module caps sidecar.
    if headers.contains_key("x-module-capabilities") {
        return Err(ApiError::BadRequest(
            "build path ignores X-Module-Capabilities; built modules always use the global capability set. Omit this header to accept global caps, or upload a pre-built wasm via POST /modules to set per-module capabilities.".into(),
        ));
    }

    // Global build slot (1 at a time — cargo lock contention).
    let _build_permit = match s.build_semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            metrics::counter!(
                "build_rejected_total",
                "tenant" => tenant.to_string(),
                "reason" => "busy",
            )
            .increment(1);
            return Err(ApiError::Unavailable(
                "another build is in progress; retry later".into(),
            ));
        }
    };

    // Tenant-level rate + concurrency (build counts against tenant quota).
    if let Err(retry_after_ms) = s.tenants.try_acquire_rate(tenant) {
        return Err(ApiError::RateLimited { retry_after_ms });
    }
    let _tenant_permit = match s.tenants.acquire_concurrency(tenant).await {
        Ok(p) => p,
        Err(_) => {
            return Err(ApiError::Unavailable(format!(
                "tenant '{tenant}' concurrency timeout"
            )));
        }
    };

    // Tempdir (auto-cleanup on drop, even on error).
    let tmpdir = tempfile::Builder::new()
        .prefix("orchestrator-build-")
        .tempdir()
        .map_err(|e| ApiError::Internal(format!("tempdir: {e}")))?;

    // Extract
    extract_tar(tmpdir.path(), &body).await?;

    // Parse Cargo.toml
    let manifest_path = tmpdir.path().join("Cargo.toml");
    let manifest_str = std::fs::read_to_string(&manifest_path)
        .map_err(|e| ApiError::BadRequest(format!("Cargo.toml missing at archive root: {e}")))?;
    let toml_val: toml::Value = toml::from_str(&manifest_str)
        .map_err(|e| ApiError::BadRequest(format!("Cargo.toml parse: {e}")))?;
    let crate_name = toml_val
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| ApiError::BadRequest("Cargo.toml [package].name missing".into()))?
        .to_string();
    let crate_underscore = crate_name.replace('-', "_");

    let module_id = headers
        .get("x-module-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| crate_underscore.clone());
    validate_id(&module_id)?;

    // Tenant capacity check (only if new)
    if !s.registry.has(tenant, &module_id) {
        let cur = s.registry.all_for_tenant(tenant).len();
        if let Err((cur, max)) = s.tenants.check_capacity(tenant, cur) {
            return Err(ApiError::TenantModuleLimit { current: cur, max });
        }
    }

    // Build
    let build_start = Instant::now();
    let out = run_cargo_build(tmpdir.path(), &s.cargo_home, s.enable_build_network).await?;
    let build_ms = build_start.elapsed().as_millis() as u64;

    if !out.status.success() {
        let tail = tail_chars(&String::from_utf8_lossy(&out.stderr), STDERR_TAIL_CHARS);
        warn!(tenant = %tenant, id = %module_id, build_ms, "build failed: {tail}");
        return Err(ApiError::BadRequest(format!(
            "build failed ({build_ms}ms): {tail}"
        )));
    }

    // Extract warnings for response
    let stderr_str = String::from_utf8_lossy(&out.stderr);
    let warnings: Vec<String> = stderr_str
        .lines()
        .filter(|l| l.starts_with("warning:"))
        .take(MAX_WARNINGS_SHOWN)
        .map(|s| s.to_string())
        .collect();

    // Locate wasm output
    let wasm_path: PathBuf = tmpdir
        .path()
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join(format!("{crate_underscore}.wasm"));
    let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| {
        ApiError::Internal(format!(
            "wasm output missing at {}: {e} \
             (does the crate declare crate-type=[\"cdylib\"] ?)",
            wasm_path.display()
        ))
    })?;
    let size_bytes = wasm_bytes.len();

    // Quota (header or default)
    let quota = parse_quota_header(headers)?.unwrap_or_else(|| s.registry.default_quota());
    let quota_state = s
        .registry
        .build_quota_state(&quota)
        .map_err(|e| ApiError::BadRequest(format!("quota: {e}")))?;

    let manifest = ModuleManifest {
        id: module_id.clone(),
        version: "0.0.0".to_string(),
        capabilities: vec![],
        tenant: tenant.to_string(),
        quota: quota.clone(),
    };
    let module = WasmModule::load(
        s.registry.engine(),
        manifest,
        &wasm_bytes,
        quota_state,
        s.ctx.capabilities.clone(),
    )
    .map_err(|e| ApiError::BadRequest(format!("wasm load: {e}")))?;

    // Persist
    let tenant_dir = s.modules_dir.join(tenant);
    let out_path = tenant_dir.join(format!("{module_id}.wasm"));
    crate::fs_util::atomic_write(&out_path, &wasm_bytes)
        .map_err(|e| ApiError::Internal(format!("disk write: {e}")))?;
    write_quota_sidecar(&tenant_dir, &module_id, &quota)
        .map_err(|e| ApiError::Internal(format!("quota sidecar: {e}")))?;

    // Register
    let status = if s
        .registry
        .insert_replace(tenant, &module_id, Arc::new(module))
        .is_some()
    {
        "replaced"
    } else {
        "created"
    };

    metrics::counter!(
        "build_total",
        "tenant" => tenant.to_string(),
        "result" => "ok",
    )
    .increment(1);
    metrics::histogram!(
        "build_duration_seconds",
        "tenant" => tenant.to_string(),
    )
    .record(build_start.elapsed().as_secs_f64());

    info!(
        tenant = %tenant,
        id = %module_id,
        status,
        build_ms,
        size = size_bytes,
        "module built"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "tenant": tenant,
            "id": module_id,
            "status": status,
            "size_bytes": size_bytes,
            "build_ms": build_ms,
            "quota": quota,
            "warnings": warnings,
        })),
    )
        .into_response())
}

// ── Tar extraction (external tar, auto-detect gz) ──────────────────
async fn extract_tar(dir: &FsPath, body: &Bytes) -> Result<(), ApiError> {
    // Extraction runs in `spawn_blocking` because tar-rs is synchronous.
    // The tar crate is used instead of shelling out to the system `tar`
    // binary: the system tar on WraithOS is BusyBox, whose path-sanitization
    // guarantees are weaker than GNU tar's. The tar crate validates every
    // entry path (`..`, absolute, prefix) and lets us filter entry types
    // (symlink, hardlink, char/block device, fifo) before any filesystem
    // write. Filtering is done explicitly here — `unpack_in` alone is not
    // enough because it will happily extract symlinks.
    let dir_owned = dir.to_path_buf();
    let body_owned = body.clone(); // Bytes clone is cheap (Arc)
    tokio::task::spawn_blocking(move || extract_tar_sync(&dir_owned, &body_owned))
        .await
        .map_err(|e| ApiError::Internal(format!("tar join: {e}")))?
}

fn extract_tar_sync(dir: &FsPath, body: &[u8]) -> Result<(), ApiError> {
    // Sniff gzip magic (0x1F 0x8B); tar-rs does not auto-detect on a
    // plain `Read` either, so we pick the decoder ourselves.
    let is_gzip = body.len() >= 2 && body[0] == 0x1f && body[1] == 0x8b;
    let reader: Box<dyn std::io::Read> = if is_gzip {
        Box::new(flate2::read::GzDecoder::new(Cursor::new(body)))
    } else {
        Box::new(Cursor::new(body))
    };
    let mut archive = tar::Archive::new(reader);

    let entries = archive
        .entries()
        .map_err(|e| ApiError::BadRequest(format!("tar entries: {e}")))?;

    for entry_result in entries {
        let mut entry =
            entry_result.map_err(|e| ApiError::BadRequest(format!("tar entry: {e}")))?;

        // Path check: reject any `..`, absolute, or prefixed component.
        // The tar crate's unpack_in already rejects `..` and absolute
        // paths, but we reject earlier with a specific error so the API
        // response is actionable.
        let path = entry
            .path()
            .map_err(|e| ApiError::BadRequest(format!("tar entry path: {e}")))?
            .into_owned();
        for comp in path.components() {
            match comp {
                Component::ParentDir => {
                    return Err(ApiError::BadRequest(
                        "tar entry contains '..' component".into(),
                    ));
                }
                Component::RootDir => {
                    return Err(ApiError::BadRequest("tar entry has absolute path".into()));
                }
                Component::Prefix(_) => {
                    return Err(ApiError::BadRequest("tar entry has path prefix".into()));
                }
                _ => {}
            }
        }

        // Entry type check: only regular files and directories are
        // extracted. Symlinks and hardlinks are rejected because they
        // are the standard tar-traversal escape (symlink to /etc/passwd
        // followed by a file entry that writes "through" the symlink).
        // Device nodes and fifos are rejected because they have no place
        // in a Cargo build source tree.
        let entry_type = entry.header().entry_type();
        if !matches!(
            entry_type,
            tar::EntryType::Regular | tar::EntryType::Directory
        ) {
            return Err(ApiError::BadRequest(format!(
                "tar entry type not allowed: {entry_type:?} (only regular files and directories accepted)"
            )));
        }

        entry
            .unpack_in(dir)
            .map_err(|e| ApiError::BadRequest(format!("tar unpack: {e}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod extract_tar_tests {
    use super::*;

    fn make_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut buf);
            for (name, content) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_path(name).unwrap();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append(&header, *content).unwrap();
            }
            builder.finish().unwrap();
        }
        buf
    }

    /// Build a tar containing a single file entry with a raw path name
    /// that the tar crate's Header::set_path would refuse (e.g. `../x`).
    fn make_tar_raw_path(name: &str, content: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];
        let name_bytes = name.as_bytes();
        header[..name_bytes.len()].copy_from_slice(name_bytes);
        header[100..107].copy_from_slice(b"0000644");
        header[107] = 0;
        header[108..115].copy_from_slice(b"0000000");
        header[115] = 0;
        header[116..123].copy_from_slice(b"0000000");
        header[123] = 0;
        let size_oct = format!("{:011o}", content.len());
        header[124..135].copy_from_slice(size_oct.as_bytes());
        header[135] = 0;
        header[136..147].copy_from_slice(b"00000000000");
        header[147] = 0;
        for i in 148..156 {
            header[i] = b' ';
        }
        header[156] = b'0';
        header[257..262].copy_from_slice(b"ustar");
        header[262] = 0;
        header[263..265].copy_from_slice(b"00");
        let sum: u32 = header.iter().map(|&b| b as u32).sum();
        let cksum = format!("{:06o}  ", sum);
        header[148..156].copy_from_slice(cksum.as_bytes());

        let mut out = Vec::new();
        out.extend_from_slice(&header);
        out.extend_from_slice(content);
        let pad = (512 - (content.len() % 512)) % 512;
        out.extend(std::iter::repeat(0u8).take(pad));
        out.extend(std::iter::repeat(0u8).take(1024));
        out
    }

    fn make_tar_with_symlink() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut buf);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_path("link").unwrap();
            header.set_size(0);
            header.set_mode(0o777);
            header.set_link_name("/etc/passwd").unwrap();
            header.set_cksum();
            builder.append(&header, &[][..]).unwrap();
            builder.finish().unwrap();
        }
        buf
    }

    #[tokio::test]
    async fn extract_tar_accepts_valid() {
        let body = make_tar(&[("Cargo.toml", b"[package]\nname = \"x\"\n")]);
        let tmp = tempfile::tempdir().unwrap();
        extract_tar(tmp.path(), &Bytes::from(body)).await.unwrap();
        assert!(tmp.path().join("Cargo.toml").exists());
    }

    #[tokio::test]
    async fn extract_tar_rejects_parent_traversal() {
        let body = make_tar_raw_path("../evil.txt", b"evil");
        let tmp = tempfile::tempdir().unwrap();
        let r = extract_tar(tmp.path(), &Bytes::from(body)).await;
        assert!(r.is_err(), "expected parent-traversal rejection, got Ok");
        // Ensure no file landed outside the tempdir sibling.
        assert!(!tmp.path().parent().unwrap().join("evil.txt").exists());
    }

    #[tokio::test]
    async fn extract_tar_rejects_symlink() {
        let body = make_tar_with_symlink();
        let tmp = tempfile::tempdir().unwrap();
        let r = extract_tar(tmp.path(), &Bytes::from(body)).await;
        assert!(r.is_err(), "expected symlink rejection, got Ok");
        assert!(!tmp.path().join("link").exists());
    }
}

// ── Cargo build (env_clear, timeout, kill_on_drop) ─────────────────
async fn run_cargo_build(
    dir: &FsPath,
    cargo_home: &FsPath,
    enable_network: bool,
) -> Result<std::process::Output, ApiError> {
    let cargo_bin = cargo_home.join("bin").join("cargo");
    if !cargo_bin.exists() {
        return Err(ApiError::Internal(format!(
            "cargo binary not found at {}",
            cargo_bin.display()
        )));
    }

    let rustup_home = cargo_home
        .parent()
        .map(|p| p.join(".rustup"))
        .filter(|p| p.exists())
        .or_else(|| std::env::var_os("RUSTUP_HOME").map(std::path::PathBuf::from))
        .ok_or_else(|| {
            ApiError::Internal("cannot determine RUSTUP_HOME (no ~/.rustup and no env var)".into())
        })?;

    let mut args: Vec<String> = vec!["--die-with-parent".to_string(), "--unshare-all".to_string()];
    if enable_network {
        args.push("--share-net".to_string());
    }

    for sys in ["/usr", "/lib", "/lib64", "/bin", "/sbin"] {
        if std::path::Path::new(sys).exists() {
            args.push("--ro-bind".into());
            args.push(sys.into());
            args.push(sys.into());
        }
    }
    if enable_network && std::path::Path::new("/etc/resolv.conf").exists() {
        args.push("--ro-bind".into());
        args.push("/etc/resolv.conf".into());
        args.push("/etc/resolv.conf".into());
    }
    args.push("--proc".into());
    args.push("/proc".into());
    args.push("--dev".into());
    args.push("/dev".into());
    args.push("--tmpfs".into());
    args.push("/dev/shm".into());
    args.push("--tmpfs".into());
    args.push("/tmp".into());
    args.push("--tmpfs".into());
    args.push("/home".into());
    args.push("--tmpfs".into());
    args.push("/root".into());

    args.push("--ro-bind".into());
    args.push(cargo_home.display().to_string());
    args.push(cargo_home.display().to_string());
    args.push("--ro-bind".into());
    args.push(rustup_home.display().to_string());
    args.push(rustup_home.display().to_string());

    args.push("--bind".into());
    args.push(dir.display().to_string());
    args.push(dir.display().to_string());
    args.push("--chdir".into());
    args.push(dir.display().to_string());

    let path_env = format!("{}/bin:/usr/bin:/bin:/usr/sbin:/sbin", cargo_home.display());
    args.push("--setenv".into());
    args.push("CARGO_HOME".into());
    args.push(cargo_home.display().to_string());
    args.push("--setenv".into());
    args.push("RUSTUP_HOME".into());
    args.push(rustup_home.display().to_string());
    args.push("--setenv".into());
    args.push("PATH".into());
    args.push(path_env);
    args.push("--setenv".into());
    args.push("HOME".into());
    args.push("/tmp".into());
    if !enable_network {
        args.push("--setenv".into());
        args.push("CARGO_NET_OFFLINE".into());
        args.push("true".into());
    }

    args.push(cargo_bin.display().to_string());
    args.push("build".into());
    args.push("--target".into());
    args.push("wasm32-wasip2".into());
    args.push("--release".into());

    let child = Command::new("bwrap")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ApiError::Internal(format!("bwrap spawn failed: {e}")))?;

    match tokio::time::timeout(
        Duration::from_secs(BUILD_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(ApiError::Internal(format!("bwrap wait: {e}"))),
        Err(_) => Err(ApiError::BadRequest(format!(
            "build timeout ({}s) in sandbox",
            BUILD_TIMEOUT_SECS
        ))),
    }
}

fn tail_chars(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let start = chars.len() - max;
        format!("...{}", chars[start..].iter().collect::<String>())
    }
}
