//! Shared DTO dan capability manifest antara host dan guest Wasm.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub id: String,
    pub version: String,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Capability {
    HttpGet { allowed_hosts: Vec<String> },
    TcpConnect { allowed_ports: Vec<u16> },
    DbQuery { allowed_tables: Vec<String> },
    Log,
}
