//! Konfigurasi runtime. Dimuat dari TOML; semua field punya default.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context as _;
use clap::ValueEnum;
use serde::Deserialize;
use shared_types::Capability;

use crate::context::AppContextConfig;

#[derive(Debug, Clone, Copy, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ScriptConfig {
    pub inline: Option<String>,
    pub path: Option<PathBuf>,
}

impl Default for ScriptConfig {
    fn default() -> Self {
        Self {
            inline: Some(
                r#"log_info("rhai received: " + event_kind);
emit_event("rhai.processed", "{\"source\":\"rhai\"}");"#
                    .to_string(),
            ),
            path: None,
        }
    }
}

impl ScriptConfig {
    pub fn load(&self) -> anyhow::Result<String> {
        match (&self.inline, &self.path) {
            (Some(s), None) => Ok(s.clone()),
            (None, Some(p)) => std::fs::read_to_string(p)
                .with_context(|| format!("read script {}", p.display())),
            (None, None) => Err(anyhow::anyhow!(
                "script config: at least one of `inline` or `path` required"
            )),
            (Some(_), Some(_)) => Err(anyhow::anyhow!(
                "script config: specify either `inline` or `path`, not both"
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub db_path: PathBuf,
    pub modules_dir: PathBuf,
    pub http_timeout_secs: u64,
    pub max_sockets: usize,
    pub max_concurrent_wasm: usize,
    pub wasm_acquire_timeout_secs: u64,
    pub wasm_memory_limit_mb: u64,
    pub max_db_connections: u32,
    pub log_level: String,
    pub log_format: LogFormat,
    /// Bind address untuk /metrics. Empty string disables.
    pub metrics_bind: Option<String>,
    pub script: ScriptConfig,
    pub capabilities: Vec<Capability>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("orchestrator.db"),
            modules_dir: PathBuf::from("./modules"),
            http_timeout_secs: 30,
            max_sockets: 256,
            max_concurrent_wasm: 2,
            wasm_acquire_timeout_secs: 10,
            wasm_memory_limit_mb: 256,
            max_db_connections: 8,
            log_level: "info".to_string(),
            log_format: LogFormat::Pretty,
            metrics_bind: Some("127.0.0.1:9090".to_string()),
            script: ScriptConfig::default(),
            capabilities: vec![
                Capability::Log,
                Capability::HttpGet {
                    allowed_hosts: vec!["example.com".to_string()],
                },
                Capability::TcpConnect {
                    allowed_ports: vec![443],
                },
            ],
        }
    }
}

impl Config {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("read config {}", path.display()))?;
        let cfg: Config = toml::from_str(&content)
            .with_context(|| format!("parse config {}", path.display()))?;
        Ok(cfg)
    }

    pub fn to_app_context(&self) -> AppContextConfig {
        AppContextConfig {
            db_path: self.db_path.clone(),
            http_timeout: Duration::from_secs(self.http_timeout_secs),
            max_db_connections: self.max_db_connections,
            max_sockets: self.max_sockets,
            max_concurrent_wasm: self.max_concurrent_wasm,
            wasm_acquire_timeout: Duration::from_secs(self.wasm_acquire_timeout_secs),
            wasm_memory_limit_mb: self.wasm_memory_limit_mb,
        }
    }
}
