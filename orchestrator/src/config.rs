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
            (None, Some(p)) => {
                std::fs::read_to_string(p).with_context(|| format!("read script {}", p.display()))
            }
            (None, None) => Err(anyhow::anyhow!(
                "script config: at least one of `inline` or `path` required"
            )),
            (Some(_), Some(_)) => Err(anyhow::anyhow!(
                "script config: specify either `inline` or `path`, not both"
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TenantConfig {
    /// Argon2 PHC string (e.g. "$argon2id$v=19$m=..."). Production recommended.
    #[serde(default)]
    pub token_hash: Option<String>,
    /// Plaintext token (dev/testing only; logs warning). If both set, hash wins.
    #[serde(default)]
    pub token_plain: Option<String>,
    /// Max # of modules per tenant. None = unlimited.
    #[serde(default)]
    pub max_modules: Option<usize>,
    /// Max concurrent wasm executions per tenant. None = only global pool.
    #[serde(default)]
    pub max_concurrent_wasm: Option<u32>,
    /// Aggregate rate limit (requests/sec) across all tenant modules. None = unlimited.
    #[serde(default)]
    pub rate_limit_per_sec: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(default = "default_tenant_name")]
    pub default_tenant: String,
    /// If true, ALL endpoints except /healthz and /readyz require
    /// Authorization: Bearer <token>. Default tenant must have token_hash
    /// or token_plain configured, else startup fails.
    #[serde(default)]
    pub require_auth: bool,
    /// If false (default), POST /modules/build returns 404. Requires explicit
    /// opt-in due to arbitrary code execution risk (build.rs runs as service user).
    #[serde(default)]
    pub enable_build: bool,
    /// If true, build sandbox keeps network access (crates.io fetch works).
    /// If false (default), network unshared — requires pre-fetched registry.
    /// Trade-off: network=true allows build.rs exfil, but allows dynamic deps.
    /// Use pre-fetched mode (false) for production.
    #[serde(default)]
    pub enable_build_network: bool,
    #[serde(default)]
    pub tenants: std::collections::HashMap<String, TenantConfig>,
    pub db_path: PathBuf,
    /// Persistent data dir. WraithOS = /data, VPS host = ./data
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
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
    /// Hex-encoded ed25519 pubkeys (64 hex chars each).
    /// If non-empty, every `*.wasm` requires valid sibling `*.wasm.sig`.
    /// Cargo home used by the build pipeline. None = $HOME/.cargo.
    #[serde(default)]
    pub cargo_home: Option<PathBuf>,
    pub trusted_signer_keys: Vec<String>,
}

fn default_tenant_name() -> String {
    "default".to_string()
}
fn default_data_dir() -> PathBuf {
    // Absolute, writable path. A relative default resolves against the
    // process CWD which is / for init-spawned processes, i.e. /data on
    // the read-only squashfs root. That would fail the first wizard
    // write before /data is mounted. /var/lib is a tmpfs from inittab
    // and is bind-replaced by /data/orchestrator once LUKS is up.
    PathBuf::from("/var/lib/orchestrator")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("orchestrator.db"),
            data_dir: default_data_dir(),
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
            default_tenant: default_tenant_name(),
            require_auth: false,
            enable_build: false,
            enable_build_network: false,
            tenants: std::collections::HashMap::new(),
            cargo_home: None,
            trusted_signer_keys: vec![],
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
        let cfg: Config =
            toml::from_str(&content).with_context(|| format!("parse config {}", path.display()))?;
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
