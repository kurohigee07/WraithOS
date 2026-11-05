//! First-boot wizard: hostname, admin password, network config.
//! State in-memory during steps 1-2, persisted to data_dir on finish.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::Argon2;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::telemetry::ApiState;

const WIZARD_HTML: &str = include_str!("wizard.html");
const COMPLETE_MARKER: &str = "wizard.complete";
const LUKS_MARKER: &str = "wizard.luks-active";
const DATA_DEVICE: &str = "/dev/vda4";
const MAPPER_NAME: &str = "wraith-data";

#[derive(Debug, Default, Clone)]
struct WizardData {
    hostname: Option<String>,
    password_hash: Option<String>,
    network_mode: Option<String>,
    static_ip: Option<String>,
    gateway: Option<String>,
    dns: Option<String>,
    completed: bool,
    encryption_password: Option<String>,
    luks_enabled: bool,
}

pub struct WizardState {
    data: Mutex<WizardData>,
}

impl WizardState {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        let completed = data_dir.join(COMPLETE_MARKER).exists();
        let luks_enabled = data_dir.join(LUKS_MARKER).exists();
        if completed {
            info!(data_dir = %data_dir.display(), "wizard: setup already complete");
        } else {
            info!(data_dir = %data_dir.display(), "wizard: first-boot setup required");
        }
        Arc::new(Self {
            data: Mutex::new(WizardData {
                completed,
                luks_enabled,
                ..Default::default()
            }),
        })
    }

    pub async fn is_completed(&self) -> bool {
        self.data.lock().await.completed
    }
}

#[derive(Deserialize)]
struct Step1Payload {
    hostname: String,
    password: String,
    #[serde(default)]
    encryption_password: Option<String>,
}

#[derive(Deserialize)]
struct Step2Payload {
    network_mode: String,
    #[serde(default)]
    static_ip: Option<String>,
    #[serde(default)]
    gateway: Option<String>,
    #[serde(default)]
    dns: Option<String>,
}

#[derive(Serialize)]
struct StatusPayload {
    completed: bool,
    step: u8,
    luks_enabled: bool,
}

#[derive(Serialize)]
struct ErrorPayload {
    error: String,
}

fn err_response(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorPayload { error: msg.into() })).into_response()
}

async fn wizard_page() -> Html<&'static str> {
    Html(WIZARD_HTML)
}

async fn status_handler(State(s): State<ApiState>) -> Json<StatusPayload> {
    let w = s.wizard.data.lock().await;
    let step = if w.completed || w.network_mode.is_some() {
        3
    } else if w.password_hash.is_some() {
        2
    } else {
        1
    };
    Json(StatusPayload {
        completed: w.completed,
        step,
        luks_enabled: w.luks_enabled,
    })
}

async fn step1_handler(State(s): State<ApiState>, Json(p): Json<Step1Payload>) -> Response {
    if p.hostname.is_empty() || p.hostname.len() > 63 {
        return err_response(StatusCode::BAD_REQUEST, "hostname 1-63 chars");
    }
    if !p
        .hostname
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return err_response(StatusCode::BAD_REQUEST, "hostname must be [a-z0-9-]");
    }
    if p.password.len() < 8 {
        return err_response(StatusCode::BAD_REQUEST, "password min 8 chars");
    }
    if p.encryption_password
        .as_deref()
        .is_some_and(|enc| enc.len() < 8)
    {
        return err_response(StatusCode::BAD_REQUEST, "encryption_password min 8 chars");
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = match Argon2::default().hash_password(p.password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(e) => {
            warn!(err = %e, "argon2 hash failed");
            return err_response(StatusCode::INTERNAL_SERVER_ERROR, "hash failed");
        }
    };

    let mut w = s.wizard.data.lock().await;
    if w.completed {
        return err_response(StatusCode::CONFLICT, "setup already completed");
    }
    w.hostname = Some(p.hostname);
    w.password_hash = Some(hash);
    w.encryption_password = p.encryption_password;
    info!("wizard: step1 saved (hostname)");
    (
        StatusCode::OK,
        Json(serde_json::json!({"status":"ok","step":1})),
    )
        .into_response()
}

async fn step2_handler(State(s): State<ApiState>, Json(p): Json<Step2Payload>) -> Response {
    if p.network_mode != "dhcp" && p.network_mode != "static" {
        return err_response(StatusCode::BAD_REQUEST, "network_mode dhcp|static");
    }
    if p.network_mode == "static" {
        if p.static_ip.as_deref().unwrap_or("").is_empty() {
            return err_response(StatusCode::BAD_REQUEST, "static_ip required");
        }
        if p.gateway.as_deref().unwrap_or("").is_empty() {
            return err_response(StatusCode::BAD_REQUEST, "gateway required");
        }
    }

    let mut w = s.wizard.data.lock().await;
    if w.completed {
        return err_response(StatusCode::CONFLICT, "setup already completed");
    }
    w.network_mode = Some(p.network_mode);
    w.static_ip = p.static_ip;
    w.gateway = p.gateway;
    w.dns = p.dns;
    info!("wizard: step2 saved (network)");
    (
        StatusCode::OK,
        Json(serde_json::json!({"status":"ok","step":2})),
    )
        .into_response()
}

type LuksResult<T> = Result<T, (StatusCode, String)>;

fn internal(e: String) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e)
}

fn stderr_tail(out: &std::process::Output) -> String {
    let text = String::from_utf8_lossy(&out.stderr);
    let s: &str = &text;
    let start = s.len().saturating_sub(200);
    let tail = s.get(start..).unwrap_or(s).trim().to_string();
    if tail.is_empty() {
        let status = out.status;
        format!("exit status {status}")
    } else {
        tail
    }
}

fn luks_cmd(args: &[&str], pwd: Option<&str>) -> Result<std::process::Output, String> {
    let mut cmd = Command::new("cryptsetup");
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if pwd.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd.spawn().map_err(|e| format!("cryptsetup spawn: {e}"))?;
    if let Some(p) = pwd {
        let mut si = child
            .stdin
            .take()
            .ok_or_else(|| "cryptsetup stdin unavailable".to_string())?;
        si.write_all(p.as_bytes())
            .map_err(|e| format!("key write: {e}"))?;
    }
    child
        .wait_with_output()
        .map_err(|e| format!("cryptsetup wait: {e}"))
}

fn succeed(step: &str, out: &std::process::Output) -> LuksResult<()> {
    if out.status.success() {
        Ok(())
    } else {
        let detail = stderr_tail(out);
        Err(internal(format!("{step} failed: {detail}")))
    }
}

fn setup_luks(pwd: &str, data_dir: &std::path::Path) -> LuksResult<()> {
    if !std::path::Path::new(DATA_DEVICE).exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            "data partition /dev/vda4 not found".to_string(),
        ));
    }
    let probe = luks_cmd(&["isLuks", DATA_DEVICE], None).map_err(internal)?;
    if probe.status.success() {
        return Err((
            StatusCode::CONFLICT,
            "data partition already encrypted".to_string(),
        ));
    }

    let format_args = [
        "luksFormat",
        "--type",
        "luks2",
        "--cipher",
        "aes-xts-plain64",
        "--key-size",
        "512",
        "--hash",
        "sha256",
        "--batch-mode",
        "--key-file",
        "-",
        DATA_DEVICE,
    ];
    let out = luks_cmd(&format_args, Some(pwd)).map_err(internal)?;
    succeed("luksFormat", &out)?;

    let open_args = ["luksOpen", "--key-file", "-", DATA_DEVICE, MAPPER_NAME];
    let out = luks_cmd(&open_args, Some(pwd)).map_err(internal)?;
    succeed("luksOpen", &out)?;

    let mapper = format!("/dev/mapper/{MAPPER_NAME}");

    // State yang finish_handler tulis ke data_dir hidup di tmpfs. Sesudah
    // reboot S30data bind-mount <volume>/orchestrator -> /var/lib/orchestrator,
    // jadi volume kosong bikin wizard minta setup ulang. Bangun tree identik
    // di staging, lalu salin ke filesystem baru lewat mkfs.ext4 -d.
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(pwd.as_bytes(), &salt)
        .map_err(|e| internal(format!("argon2 unlock hash: {e}")))?
        .to_string();

    let stage = std::path::Path::new("/tmp/wraith-luks-stage");
    let _ = std::fs::remove_dir_all(stage);
    std::fs::create_dir_all(stage).map_err(|e| internal(format!("luks stage mkdir: {e}")))?;
    let orch = stage.join("orchestrator");
    std::fs::create_dir_all(orch.join("wraith"))
        .map_err(|e| internal(format!("luks stage mkdir: {e}")))?;
    std::fs::create_dir_all(stage.join("modules"))
        .map_err(|e| internal(format!("luks stage mkdir: {e}")))?;
    std::fs::create_dir_all(stage.join("wraith"))
        .map_err(|e| internal(format!("luks stage mkdir: {e}")))?;
    for name in ["hostname.txt", "admin.password.argon2", "network.toml"] {
        let src = data_dir.join("wraith").join(name);
        if src.exists() {
            std::fs::copy(&src, orch.join("wraith").join(name))
                .map_err(|e| internal(format!("luks stage copy {name}: {e}")))?;
        }
    }
    std::fs::write(
        orch.join("wraith").join("unlock.argon2"),
        format!("{hash}\n"),
    )
    .map_err(|e| internal(format!("luks stage unlock.argon2: {e}")))?;
    std::fs::write(orch.join(LUKS_MARKER), "luks2\n")
        .map_err(|e| internal(format!("luks stage marker: {e}")))?;
    std::fs::write(orch.join(COMPLETE_MARKER), "completed\n")
        .map_err(|e| internal(format!("luks stage complete: {e}")))?;

    let mkfs = Command::new("mkfs.ext4")
        .arg("-d")
        .arg(stage)
        .arg("-F")
        .arg(&mapper)
        .output()
        .map_err(|e| internal(format!("mkfs.ext4 spawn: {e}")))?;
    if !mkfs.status.success() {
        // Fallback kalau e2fsprogs-nya nggak punya populate_fs: format polos,
        // mount sementara, cp, umount. Tetap di volume, bukan tmpfs.
        let plain = Command::new("mkfs.ext4")
            .arg("-F")
            .arg(&mapper)
            .output()
            .map_err(|e| internal(format!("mkfs.ext4 spawn: {e}")))?;
        succeed("mkfs.ext4", &plain)?;
        let mnt = std::path::Path::new("/tmp/wraith-luks-mnt");
        let _ = std::fs::remove_dir_all(mnt);
        std::fs::create_dir_all(mnt).map_err(|e| internal(format!("luks mnt mkdir: {e}")))?;
        let m = Command::new("mount")
            .args(["-t", "ext4", mapper.as_str(), "/tmp/wraith-luks-mnt"])
            .output()
            .map_err(|e| internal(format!("luks mnt mount spawn: {e}")))?;
        succeed("luks mnt mount", &m)?;
        let cp = Command::new("cp")
            .args(["-a", "/tmp/wraith-luks-stage/.", "/tmp/wraith-luks-mnt/"])
            .output()
            .map_err(|e| internal(format!("luks mnt cp spawn: {e}")))?;
        let u = Command::new("umount")
            .arg("/tmp/wraith-luks-mnt")
            .output()
            .map_err(|e| internal(format!("luks mnt umount spawn: {e}")))?;
        succeed("luks mnt cp", &cp)?;
        succeed("luks mnt umount", &u)?;
    }

    let close = luks_cmd(&["luksClose", MAPPER_NAME], None).map_err(internal)?;
    succeed("luksClose", &close)?;

    // State identik untuk sesi berjalan ini (tmpfs).
    let wraith_dir = data_dir.join("wraith");
    std::fs::create_dir_all(&wraith_dir).map_err(|e| internal(format!("mkdir wraith dir: {e}")))?;
    std::fs::write(wraith_dir.join("unlock.argon2"), format!("{hash}\n"))
        .map_err(|e| internal(format!("write unlock.argon2: {e}")))?;
    std::fs::write(data_dir.join(LUKS_MARKER), "luks2\n")
        .map_err(|e| internal(format!("write luks marker: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn markers_survive_restart() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(COMPLETE_MARKER), "completed\n").unwrap();
        std::fs::write(tmp.path().join(LUKS_MARKER), "luks2\n").unwrap();
        let s = WizardState::new(tmp.path().to_path_buf());
        assert!(s.is_completed().await);
        assert!(s.data.lock().await.luks_enabled);
    }

    #[tokio::test]
    async fn fresh_state_is_incomplete() {
        let tmp = tempfile::tempdir().unwrap();
        let s = WizardState::new(tmp.path().to_path_buf());
        assert!(!s.is_completed().await);
        assert!(!s.data.lock().await.luks_enabled);
    }
}

async fn finish_handler(State(s): State<ApiState>) -> Response {
    let mut w = s.wizard.data.lock().await;
    if w.completed {
        return err_response(StatusCode::CONFLICT, "setup already completed");
    }
    let hostname = match w.hostname.as_ref() {
        Some(h) => h.clone(),
        None => return err_response(StatusCode::BAD_REQUEST, "step1 not done"),
    };
    let password_hash = match w.password_hash.as_ref() {
        Some(h) => h.clone(),
        None => return err_response(StatusCode::BAD_REQUEST, "step1 not done"),
    };
    let network_mode = match w.network_mode.as_ref() {
        Some(n) => n.clone(),
        None => return err_response(StatusCode::BAD_REQUEST, "step2 not done"),
    };

    let wraith_dir = s.data_dir.join("wraith");
    if let Err(e) = std::fs::create_dir_all(&wraith_dir) {
        warn!(err = %e, "wizard: mkdir wraith dir failed");
        return err_response(StatusCode::INTERNAL_SERVER_ERROR, "mkdir failed");
    }

    let _ = std::fs::write(wraith_dir.join("hostname.txt"), format!("{hostname}\n"));
    let _ = std::fs::write(
        wraith_dir.join("admin.password.argon2"),
        format!("{password_hash}\n"),
    );

    let net = format!(
        "mode = \"{}\"\n{}",
        network_mode,
        match network_mode.as_str() {
            "static" => format!(
                "static_ip = \"{}\"\ngateway = \"{}\"\ndns = \"{}\"\n",
                w.static_ip.as_deref().unwrap_or(""),
                w.gateway.as_deref().unwrap_or(""),
                w.dns.as_deref().unwrap_or("")
            ),
            _ => String::new(),
        }
    );
    let _ = std::fs::write(wraith_dir.join("network.toml"), net);
    if let Some(luks_pwd) = w.encryption_password.clone() {
        match setup_luks(&luks_pwd, &s.data_dir) {
            Ok(()) => {
                w.luks_enabled = true;
                w.encryption_password = None;
                info!("wizard: LUKS data partition ready");
            }
            Err((code, msg)) => {
                warn!(err = %msg, "wizard: LUKS setup failed");
                return err_response(code, msg);
            }
        }
    }

    let marker = s.data_dir.join(COMPLETE_MARKER);
    if let Err(e) = std::fs::write(&marker, "completed\n") {
        warn!(err = %e, "wizard: write marker failed");
        return err_response(StatusCode::INTERNAL_SERVER_ERROR, "marker write failed");
    }

    w.completed = true;
    info!("wizard: setup complete (hostname={})", hostname);
    (
        StatusCode::OK,
        Json(serde_json::json!({"status":"complete","reboot_required":true})),
    )
        .into_response()
}

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/wizard", get(wizard_page))
        .route("/wizard/status", get(status_handler))
        .route("/wizard/step1", post(step1_handler))
        .route("/wizard/step2", post(step2_handler))
        .route("/wizard/finish", post(finish_handler))
}
