//! Shared DTO dan capability manifest antara host dan guest Wasm.

use serde::{Deserialize, Serialize};

fn default_tenant() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub id: String,
    pub version: String,
    pub capabilities: Vec<Capability>,
    #[serde(default = "default_tenant")]
    pub tenant: String,
    #[serde(default)]
    pub quota: ModuleQuota,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Capability {
    HttpGet { allowed_hosts: Vec<String> },
    TcpConnect { allowed_ports: Vec<u16> },
    DbQuery { allowed_tables: Vec<String> },
    Log,
    EmitEvent { allowed_kinds: Vec<String> },
}

/// Per-module resource quota. Semua field 0 = inherit global / unlimited.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModuleQuota {
    /// Linear memory cap. 0 = inherit `AppContextConfig.wasm_memory_limit_mb`.
    /// Non-zero must be <= global ceiling.
    #[serde(default)]
    pub max_memory_mb: u64,
    /// Per-module concurrency cap. 0 = only global `WasmPool` applies.
    #[serde(default)]
    pub max_concurrent: u32,
    /// Token bucket rate (requests/sec). 0 = unlimited.
    #[serde(default)]
    pub rate_limit_per_sec: u32,
}
