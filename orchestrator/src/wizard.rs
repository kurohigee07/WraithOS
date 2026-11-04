//! First-boot wizard: hostname, admin password, network config.
//! State in-memory during steps 1-2, persisted to data_dir on finish.

use std::path::PathBuf;
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

#[derive(Debug, Default, Clone)]
struct WizardData {
    hostname: Option<String>,
    password_hash: Option<String>,
    network_mode: Option<String>,
    static_ip: Option<String>,
    gateway: Option<String>,
    dns: Option<String>,
    completed: bool,
}

pub struct WizardState {
    data: Mutex<WizardData>,
}

impl WizardState {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        let completed = data_dir.join(COMPLETE_MARKER).exists();
        if completed {
            info!(data_dir = %data_dir.display(), "wizard: setup already complete");
        } else {
            info!(data_dir = %data_dir.display(), "wizard: first-boot setup required");
        }
        Arc::new(Self {
            data: Mutex::new(WizardData { completed, ..Default::default() }),
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
    Json(StatusPayload { completed: w.completed, step })
}

async fn step1_handler(State(s): State<ApiState>, Json(p): Json<Step1Payload>) -> Response {
    if p.hostname.is_empty() || p.hostname.len() > 63 {
        return err_response(StatusCode::BAD_REQUEST, "hostname 1-63 chars");
    }
    if !p.hostname.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return err_response(StatusCode::BAD_REQUEST, "hostname must be [a-z0-9-]");
    }
    if p.password.len() < 8 {
        return err_response(StatusCode::BAD_REQUEST, "password min 8 chars");
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
    info!("wizard: step1 saved (hostname)");
    (StatusCode::OK, Json(serde_json::json!({"status":"ok","step":1}))).into_response()
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
    (StatusCode::OK, Json(serde_json::json!({"status":"ok","step":2}))).into_response()
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
    let _ = std::fs::write(wraith_dir.join("admin.password.argon2"), format!("{password_hash}\n"));

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

    let marker = s.data_dir.join(COMPLETE_MARKER);
    if let Err(e) = std::fs::write(&marker, "completed\n") {
        warn!(err = %e, "wizard: write marker failed");
        return err_response(StatusCode::INTERNAL_SERVER_ERROR, "marker write failed");
    }

    w.completed = true;
    info!("wizard: setup complete (hostname={})", hostname);
    (StatusCode::OK, Json(serde_json::json!({"status":"complete","reboot_required":true}))).into_response()
}

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/wizard", get(wizard_page))
        .route("/wizard/status", get(status_handler))
        .route("/wizard/step1", post(step1_handler))
        .route("/wizard/step2", post(step2_handler))
        .route("/wizard/finish", post(finish_handler))
}
