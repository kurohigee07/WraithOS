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
    /// Multi-tenant seed (M17). Untuk M15–M16 semua modul di tenant
    /// "default". Field ini dormant: M17 akan pakai untuk isolasi.
    #[serde(default = "default_tenant")]
    pub tenant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Capability {
    HttpGet { allowed_hosts: Vec<String> },
    TcpConnect { allowed_ports: Vec<u16> },
    DbQuery { allowed_tables: Vec<String> },
    Log,
}
