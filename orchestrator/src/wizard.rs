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
use std::os::unix::fs::PermissionsExt;

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
    ssh_pubkey: Option<String>,
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
    #[serde(default)]
    ssh_pubkey: Option<String>,
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
    luks_recovery_needed: bool,
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
    let luks_recovery_needed = {
        let probe = luks_cmd(&["isLuks", DATA_DEVICE], None)
            .map(|o| o.status.success())
            .unwrap_or(false);
        let mounted = std::fs::read_to_string("/proc/mounts")
            .map(|m| {
                m.lines()
                    .any(|l| l.split_whitespace().nth(1) == Some("/data"))
            })
            .unwrap_or(false);
        probe && !mounted
    };
    Json(StatusPayload {
        completed: w.completed,
        step,
        luks_enabled: w.luks_enabled,
        luks_recovery_needed,
    })
}

fn validate_ssh_pubkey(input: &str) -> Result<String, String> {
    const ALLOWED: &[&str] = &[
        "ssh-ed25519",
        "ssh-rsa",
        "ecdsa-sha2-nistp256",
        "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp521",
        "sk-ssh-ed25519@openssh.com",
        "sk-ecdsa-sha2-nistp256@openssh.com",
    ];
    let mut out = Vec::new();
    for (i, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let ktype = parts
            .next()
            .ok_or_else(|| format!("line {}: empty", i + 1))?;
        let payload = parts
            .next()
            .ok_or_else(|| format!("line {}: missing base64 payload", i + 1))?;
        if !ALLOWED.contains(&ktype) {
            return Err(format!("line {}: unsupported key type '{}'", i + 1, ktype));
        }
        if !payload.starts_with("AAAA") || payload.len() < 16 {
            return Err(format!("line {}: invalid base64 payload", i + 1));
        }
        if !payload
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
        {
            return Err(format!("line {}: non-base64 chars in payload", i + 1));
        }
        out.push(line.to_string());
    }
    if out.is_empty() {
        return Err("no valid keys found".to_string());
    }
    Ok(out.join("\n"))
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

    match p
        .ssh_pubkey
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => match validate_ssh_pubkey(raw) {
            Ok(keys) => w.ssh_pubkey = Some(keys),
            Err(e) => {
                return err_response(StatusCode::BAD_REQUEST, format!("ssh_pubkey: {e}"));
            }
        },
        None => w.ssh_pubkey = None,
    }
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

/// Item 1: best-effort seal of /boot/luks.key to TPM 2.0.
/// Never fails wizard completion — plaintext keyfile stays as
/// recovery path until operator runs wraith-tpm-finalize.
enum SealOutcome {
    Sealed,
    TpmAbsent,
    Failed(String),
}

async fn tpm_seal_best_effort() -> SealOutcome {
    tpm_seal_best_effort_at("/sbin/wraith-tpm-seal").await
}

async fn tpm_seal_best_effort_at(script: &str) -> SealOutcome {
    let fut = tokio::process::Command::new(script)
        .arg("/boot/luks.key")
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(std::time::Duration::from_secs(60), fut).await {
        Err(_) => SealOutcome::Failed("timeout after 60s".into()),
        Ok(Ok(o)) if o.status.success() => SealOutcome::Sealed,
        Ok(Ok(o)) if o.status.code() == Some(2) => SealOutcome::TpmAbsent,
        Ok(Ok(o)) => SealOutcome::Failed(format!(
            "exit={:?} stderr_tail={}",
            o.status.code(),
            stderr_tail(&o)
        )),
        Ok(Err(e)) => SealOutcome::Failed(format!("spawn failed: {e}")),
    }
}

/// H3: best-effort one-way finalize of the LUKS keyfile.
/// Wraps /sbin/wraith-tpm-finalize, which re-unseals the sealed blob
/// before removing /boot/luks.key. Any failure (TPM absent, unseal
/// mismatch, /boot ro) leaves the plaintext keyfile in place, so the
/// caller never ends up in a bricked boot state.
enum FinalizeOutcome {
    Finalized,
    Skipped,
    Failed(String),
}

async fn tpm_finalize_best_effort() -> FinalizeOutcome {
    tpm_finalize_best_effort_at("/sbin/wraith-tpm-finalize").await
}

async fn tpm_finalize_best_effort_at(script: &str) -> FinalizeOutcome {
    let fut = tokio::process::Command::new(script)
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(std::time::Duration::from_secs(120), fut).await {
        Err(_) => FinalizeOutcome::Failed("timeout after 120s".into()),
        Ok(Ok(o)) if o.status.success() => FinalizeOutcome::Finalized,
        Ok(Ok(o)) if o.status.code() == Some(1) => FinalizeOutcome::Skipped,
        Ok(Ok(o)) => FinalizeOutcome::Failed(format!(
            "exit={:?} stderr_tail={}",
            o.status.code(),
            stderr_tail(&o)
        )),
        Ok(Err(e)) => FinalizeOutcome::Failed(format!("spawn failed: {e}")),
    }
}

fn derive_keyfile(pwd: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    Argon2::default()
        .hash_password_into(pwd.as_bytes(), b"wraith-luks-keyfile-v1", &mut out)
        .expect("argon2 keyfile derivation");
    out
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

    let keyfile = "/tmp/luks.key";
    let key = derive_keyfile(pwd);
    std::fs::write(keyfile, key).map_err(|e| internal(format!("keyfile write: {e}")))?;
    std::fs::set_permissions(keyfile, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| internal(format!("keyfile chmod: {e}")))?;

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
        keyfile,
        DATA_DEVICE,
    ];
    let out = luks_cmd(&format_args, None).map_err(internal)?;
    succeed("luksFormat", &out)?;

    let open_args = ["luksOpen", "--key-file", keyfile, DATA_DEVICE, MAPPER_NAME];
    let out = luks_cmd(&open_args, None).map_err(internal)?;
    succeed("luksOpen", &out)?;

    let mapper = format!("/dev/mapper/{MAPPER_NAME}");

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(pwd.as_bytes(), &salt)
        .map_err(|e| internal(format!("argon2 unlock hash: {e}")))?
        .to_string();

    let stage = std::path::Path::new("/tmp/wraith-luks-stage");
    let _ = std::fs::remove_dir_all(stage);
    std::fs::create_dir_all(stage).map_err(|e| internal(format!("stage mkdir: {e}")))?;
    let orch = stage.join("orchestrator");
    std::fs::create_dir_all(orch.join("wraith"))
        .map_err(|e| internal(format!("stage mkdir: {e}")))?;
    std::fs::create_dir_all(stage.join("modules"))
        .map_err(|e| internal(format!("stage mkdir: {e}")))?;
    std::fs::create_dir_all(stage.join("wraith"))
        .map_err(|e| internal(format!("stage mkdir: {e}")))?;
    for name in ["hostname.txt", "admin.password.argon2", "network.toml"] {
        let src = data_dir.join("wraith").join(name);
        if src.exists() {
            std::fs::copy(&src, orch.join("wraith").join(name))
                .map_err(|e| internal(format!("stage copy {name}: {e}")))?;
        }
    }
    std::fs::write(
        orch.join("wraith").join("unlock.argon2"),
        format!("{hash}\n"),
    )
    .map_err(|e| internal(format!("stage unlock.argon2: {e}")))?;
    std::fs::write(orch.join(LUKS_MARKER), "luks2\n")
        .map_err(|e| internal(format!("stage marker: {e}")))?;
    std::fs::write(orch.join(COMPLETE_MARKER), "completed\n")
        .map_err(|e| internal(format!("stage complete: {e}")))?;

    let mkfs = Command::new("mkfs.ext4")
        .arg("-d")
        .arg(stage)
        .arg("-F")
        .arg(&mapper)
        .output()
        .map_err(|e| internal(format!("mkfs.ext4 spawn: {e}")))?;
    if !mkfs.status.success() {
        let plain = Command::new("mkfs.ext4")
            .arg("-F")
            .arg(&mapper)
            .output()
            .map_err(|e| internal(format!("mkfs.ext4 spawn: {e}")))?;
        succeed("mkfs.ext4", &plain)?;
        let mnt = std::path::Path::new("/tmp/wraith-luks-mnt");
        let _ = std::fs::remove_dir_all(mnt);
        std::fs::create_dir_all(mnt).map_err(|e| internal(format!("mnt mkdir: {e}")))?;
        let m = Command::new("mount")
            .arg(&mapper)
            .arg(mnt)
            .output()
            .map_err(|e| internal(format!("mount spawn: {e}")))?;
        if !m.status.success() {
            let _ = std::fs::remove_file(keyfile);
            return Err(internal("mount mapper for cp failed".to_string()));
        }
        for sub in ["orchestrator", "modules", "wraith"] {
            let d = mnt.join(sub);
            std::fs::create_dir_all(&d).map_err(|e| internal(format!("mnt sub mkdir: {e}")))?;
        }
        let cp_status = Command::new("cp")
            .arg("-a")
            .arg(format!("{}/.", stage.display()))
            .arg(mnt)
            .status();
        let _ = Command::new("umount").arg(mnt).status();
        match cp_status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                return Err(internal(format!(
                    "cp -a stage to /data failed: exit={:?}",
                    s.code()
                )))
            }
            Err(e) => return Err(internal(format!("cp -a spawn: {e}"))),
        }
    }

    let close = luks_cmd(&["luksClose", MAPPER_NAME], None).map_err(internal)?;
    succeed("luksClose", &close)?;

    std::fs::copy(keyfile, "/boot/luks.key")
        .map_err(|e| internal(format!("keyfile -> /boot: {e}")))?;
    let _ = std::fs::remove_file(keyfile);

    Ok(())
}

fn write_sync(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    f.write_all(data)?;
    f.sync_all()?;
    Ok(())
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

    if let Err(e) = write_sync(
        &wraith_dir.join("hostname.txt"),
        format!("{hostname}\n").as_bytes(),
    ) {
        warn!(err = %e, "wizard: write hostname failed");
        return err_response(StatusCode::INTERNAL_SERVER_ERROR, "hostname write failed");
    }
    if let Err(e) = write_sync(
        &wraith_dir.join("admin.password.argon2"),
        format!("{password_hash}\n").as_bytes(),
    ) {
        warn!(err = %e, "wizard: write password hash failed");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "password hash write failed",
        );
    }

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
    if let Err(e) = write_sync(&wraith_dir.join("network.toml"), net.as_bytes()) {
        warn!(err = %e, "wizard: write network.toml failed");
        return err_response(StatusCode::INTERNAL_SERVER_ERROR, "network write failed");
    }
    let luks_pwd = match w.encryption_password.as_ref() {
        Some(p) => p.clone(),
        None => {
            return err_response(
                StatusCode::BAD_REQUEST,
                "encryption_password required for /data persistence",
            )
        }
    };
    // D1-completion: setup_luks blocks on luksFormat / mkfs.ext4 /
    // cp -a. A wedged disk or hung cryptsetup would hold the
    // wizard.data mutex forever, blocking every subsequent wizard
    // request. Run it on the blocking pool behind a hard timeout.
    // 300 s is generous: LUKS format + mkfs on the 256 MiB /data
    // partition finishes in <30 s even on slow SD cards.
    let pwd_for_blocking = luks_pwd.clone();
    let data_dir_for_blocking = s.data_dir.clone();
    let setup_result = match tokio::time::timeout(
        std::time::Duration::from_secs(300),
        tokio::task::spawn_blocking(move || setup_luks(&pwd_for_blocking, &data_dir_for_blocking)),
    )
    .await
    {
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "LUKS setup timeout after 300 s (disk wedged?). Reboot to recover.".to_string(),
        )),
        Ok(Err(join_err)) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("LUKS setup task panic: {join_err}"),
        )),
        Ok(Ok(r)) => r,
    };
    match setup_result {
        Ok(()) => {
            w.luks_enabled = true;
            w.encryption_password = None;
            info!("wizard: LUKS data partition ready");

            // Item 1: best-effort TPM seal. Non-fatal.
            let seal_outcome = tpm_seal_best_effort().await;
            let seal_ok = matches!(&seal_outcome, SealOutcome::Sealed);
            match seal_outcome {
                SealOutcome::Sealed => info!("wizard: TPM seal succeeded"),
                SealOutcome::TpmAbsent => info!("wizard: TPM absent, plaintext keyfile only"),
                SealOutcome::Failed(err) => {
                    warn!(err = %err, "wizard: TPM seal failed (non-fatal, plaintext fallback)")
                }
            }
            // Re-mount /data from the new LUKS volume immediately so
            // RAUC status writes and orchestrator state see a
            // writable /data without waiting for a reboot. S30data
            // restart runs stop (unmount everything) + start (unlock
            // LUKS with the keyfile we just wrote to /boot).
            let remount_fut = tokio::process::Command::new("/etc/init.d/S30data")
                .arg("restart")
                .kill_on_drop(true)
                .status();
            let remount_ok =
                match tokio::time::timeout(std::time::Duration::from_secs(60), remount_fut).await {
                    Err(_) => {
                        warn!("wizard: /data remount timeout after 60s");
                        false
                    }
                    Ok(Ok(st)) if st.success() => {
                        info!("wizard: /data remounted from LUKS");
                        true
                    }
                    Ok(Ok(st)) => {
                        warn!(code = ?st.code(), "wizard: /data remount exit non-zero");
                        false
                    }
                    Ok(Err(e)) => {
                        warn!(err = %e, "wizard: /data remount spawn failed");
                        false
                    }
                };
            // H3: auto-finalize only when the TPM path is verified.
            // wraith-tpm-finalize re-unseals before removing the
            // plaintext keyfile, so any transient failure leaves the
            // keyfile in place and S30data falls back normally.
            if seal_ok && remount_ok {
                match tpm_finalize_best_effort().await {
                    FinalizeOutcome::Finalized => {
                        info!("wizard: TPM finalized, plaintext keyfile removed")
                    }
                    FinalizeOutcome::Skipped => {
                        info!("wizard: TPM finalize skipped (no sealed blob or already finalized)")
                    }
                    FinalizeOutcome::Failed(err) => {
                        warn!(err = %err, "wizard: TPM finalize failed (plaintext retained)")
                    }
                }
            } else {
                info!(
                    seal_ok,
                    remount_ok, "wizard: TPM finalize skipped (preconditions not met)"
                );
            }
        }
        Err((code, msg)) => {
            warn!(err = %msg, "wizard: LUKS setup failed");
            return err_response(code, msg);
        }
    }

    if let Some(pk) = w.ssh_pubkey.as_ref() {
        if let Err(e) = write_sync(&wraith_dir.join("authorized_keys"), pk.as_bytes()) {
            warn!(err = %e, "wizard: write authorized_keys failed");
        } else {
            match write_sync(&wraith_dir.join("ssh-pubkey-only"), b"enabled\n") {
                Ok(()) => info!("wizard: SSH pubkey-only mode enabled"),
                Err(e) => warn!(err = %e, "wizard: write ssh-pubkey-only marker failed"),
            }
        }
    }

    let marker = s.data_dir.join(COMPLETE_MARKER);
    if let Err(e) = write_sync(&marker, b"completed\n") {
        warn!(err = %e, "wizard: write marker failed");
        return err_response(StatusCode::INTERNAL_SERVER_ERROR, "marker write failed");
    }

    w.completed = true;

    // v0.51: restart dropbear now that wizard.complete is on disk.
    // S50dropbear refuses to start pre-wizard (the default root
    // password would be exposed on port 22 before provisioning).
    // Serial console remains the provisioning interface pre-wizard.
    let dropbear_fut = tokio::process::Command::new("/etc/init.d/S50dropbear")
        .arg("restart")
        .kill_on_drop(true)
        .status();
    match tokio::time::timeout(std::time::Duration::from_secs(30), dropbear_fut).await {
        Err(_) => warn!("wizard: dropbear restart timeout after 30s"),
        Ok(Ok(st)) if st.success() => info!("wizard: dropbear restarted"),
        Ok(Ok(st)) => warn!(code = ?st.code(), "wizard: dropbear restart exit non-zero"),
        Ok(Err(e)) => warn!(err = %e, "wizard: dropbear restart spawn failed"),
    }

    info!("wizard: setup complete (hostname={})", hostname);
    (
        StatusCode::OK,
        Json(serde_json::json!({"status":"complete","reboot_required":true})),
    )
        .into_response()
}

async fn reset_luks_handler(State(s): State<ApiState>, headers: axum::http::HeaderMap) -> Response {
    // Confirmation gate: LUKS erase is destructive and irreversible
    // (removes the LUKS header on /dev/vda4, including the keyfile
    // slot). require_auth is off by default, so anyone with loopback
    // (or network, if metrics_bind was set to 0.0.0.0) could trigger
    // this. A header requirement blocks accidental calls and naive
    // scanners; a determined attacker with code access to the guest
    // can trivially add the header, but at that point the attacker
    // already has local code execution and this endpoint is not the
    // escalation. See docs/wraithos.md -> "Wizard reset-luks" for
    // the documented recovery path.
    let confirmed = headers
        .get("x-wizard-confirm")
        .and_then(|v| v.to_str().ok())
        == Some("I-understand-data-loss");
    if !confirmed {
        return err_response(
            StatusCode::BAD_REQUEST,
            "add header 'X-Wizard-Confirm: I-understand-data-loss' to confirm",
        );
    }

    let mounted = std::fs::read_to_string("/proc/mounts")
        .map(|m| {
            m.lines()
                .any(|l| l.split_whitespace().nth(1) == Some("/data"))
        })
        .unwrap_or(false);
    if mounted {
        return err_response(StatusCode::CONFLICT, "/data is mounted; refusing to erase");
    }
    let probe = match luks_cmd(&["isLuks", DATA_DEVICE], None) {
        Ok(o) => o,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, format!("isLuks: {e}")),
    };
    if !probe.status.success() {
        return err_response(StatusCode::BAD_REQUEST, "/dev/vda4 is not a LUKS device");
    }
    let out = match luks_cmd(&["luksErase", "--batch-mode", DATA_DEVICE], None) {
        Ok(o) => o,
        Err(e) => {
            return err_response(StatusCode::INTERNAL_SERVER_ERROR, format!("luksErase: {e}"))
        }
    };
    if !out.status.success() {
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("luksErase failed: {}", stderr_tail(&out)),
        );
    }
    let _ = std::fs::remove_file("/boot/luks.key");
    info!(
        target: "audit",
        event = "wizard.luks_erase",
        device = DATA_DEVICE,
        "wizard: LUKS header erased via /wizard/reset-luks"
    );
    let mut w = s.wizard.data.lock().await;
    w.luks_enabled = false;
    w.completed = false;
    w.encryption_password = None;
    w.password_hash = None;
    w.hostname = None;
    w.network_mode = None;
    w.static_ip = None;
    w.gateway = None;
    w.dns = None;
    info!("wizard: LUKS reset; re-run setup");
    (StatusCode::OK, Json(serde_json::json!({"status":"reset"}))).into_response()
}

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/wizard", get(wizard_page))
        .route("/wizard/status", get(status_handler))
        .route("/wizard/step1", post(step1_handler))
        .route("/wizard/step2", post(step2_handler))
        .route("/wizard/finish", post(finish_handler))
        .route("/wizard/reset-luks", post(reset_luks_handler))
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

#[cfg(test)]
mod h3_tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // H3 tests spawn child processes; serialize to avoid filesystem
    // contention races that intermittently fail Command::output() under
    // parallel cargo test threads.
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn write_exit_script(dir: &std::path::Path, name: &str, code: i32) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "#!/bin/sh\nexit {}", code).unwrap();
        // sync_all forces the file to disk + inode ready before exec.
        // Parallel tests otherwise occasionally race here and spawn fails.
        f.sync_all().unwrap();
        drop(f);
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).unwrap();
        p
    }

    #[tokio::test]
    async fn seal_maps_exit_zero_to_sealed() {
        let _g = LOCK.lock().await;
        let td = tempfile::tempdir().unwrap();
        let s = write_exit_script(td.path(), "seal", 0);
        assert!(matches!(
            tpm_seal_best_effort_at(s.to_str().unwrap()).await,
            SealOutcome::Sealed
        ));
    }

    #[tokio::test]
    async fn seal_maps_exit_two_to_tpm_absent() {
        let _g = LOCK.lock().await;
        let td = tempfile::tempdir().unwrap();
        let s = write_exit_script(td.path(), "seal", 2);
        assert!(matches!(
            tpm_seal_best_effort_at(s.to_str().unwrap()).await,
            SealOutcome::TpmAbsent
        ));
    }

    #[tokio::test]
    async fn seal_maps_exit_one_to_failed() {
        let _g = LOCK.lock().await;
        let td = tempfile::tempdir().unwrap();
        let s = write_exit_script(td.path(), "seal", 1);
        assert!(matches!(
            tpm_seal_best_effort_at(s.to_str().unwrap()).await,
            SealOutcome::Failed(_)
        ));
    }

    #[tokio::test]
    async fn seal_missing_script_is_failed() {
        let _g = LOCK.lock().await;
        assert!(matches!(
            tpm_seal_best_effort_at("/tmp/does-not-exist-wraith-seal").await,
            SealOutcome::Failed(_)
        ));
    }

    #[tokio::test]
    async fn finalize_maps_exit_zero_to_finalized() {
        let _g = LOCK.lock().await;
        let td = tempfile::tempdir().unwrap();
        let s = write_exit_script(td.path(), "finalize", 0);
        assert!(matches!(
            tpm_finalize_best_effort_at(s.to_str().unwrap()).await,
            FinalizeOutcome::Finalized
        ));
    }

    #[tokio::test]
    async fn finalize_maps_exit_one_to_skipped() {
        let _g = LOCK.lock().await;
        let td = tempfile::tempdir().unwrap();
        let s = write_exit_script(td.path(), "finalize", 1);
        assert!(matches!(
            tpm_finalize_best_effort_at(s.to_str().unwrap()).await,
            FinalizeOutcome::Skipped
        ));
    }

    #[tokio::test]
    async fn finalize_maps_other_exit_to_failed() {
        let _g = LOCK.lock().await;
        let td = tempfile::tempdir().unwrap();
        let s = write_exit_script(td.path(), "finalize", 3);
        assert!(matches!(
            tpm_finalize_best_effort_at(s.to_str().unwrap()).await,
            FinalizeOutcome::Failed(_)
        ));
    }

    #[tokio::test]
    async fn finalize_missing_script_is_failed() {
        let _g = LOCK.lock().await;
        assert!(matches!(
            tpm_finalize_best_effort_at("/tmp/does-not-exist-wraith-finalize").await,
            FinalizeOutcome::Failed(_)
        ));
    }
}
