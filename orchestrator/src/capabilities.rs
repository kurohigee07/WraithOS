use shared_types::Capability;
use crate::error::{OrchestratorError, Result};

#[derive(Debug, Clone)]
enum HostMatcher {
    Exact(String),
    Suffix(String),
}

impl HostMatcher {
    fn parse(spec: &str) -> Self {
        if let Some(sfx) = spec.strip_prefix("*.") {
            Self::Suffix(sfx.to_ascii_lowercase())
        } else {
            Self::Exact(spec.to_ascii_lowercase())
        }
    }
    fn matches(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        match self {
            Self::Exact(e) => e == &host,
            Self::Suffix(sfx) => {
                host.len() > sfx.len() + 1
                    && host.ends_with(sfx)
                    && host.as_bytes()[host.len() - sfx.len() - 1] == b'.'
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    http_hosts: Vec<HostMatcher>,
    tcp_ports: Vec<u16>,
    tcp_hosts: Vec<HostMatcher>,
    db_tables: Vec<String>,
    emit_kinds: Vec<String>,
    allow_log: bool,
}

impl CapabilitySet {
    pub fn from_capabilities(caps: &[Capability]) -> Self {
        let mut set = Self::default();
        for cap in caps {
            match cap {
                Capability::HttpGet { allowed_hosts } => {
                    set.http_hosts.extend(allowed_hosts.iter().map(|h| HostMatcher::parse(h)));
                }
                Capability::TcpConnect { allowed_ports } => {
                    set.tcp_ports.extend(allowed_ports.iter().copied());
                }
                Capability::DbQuery { allowed_tables } => {
                    set.db_tables.extend(allowed_tables.iter().cloned());
                }
                Capability::Log => set.allow_log = true,
                Capability::EmitEvent { allowed_kinds } => {
                    set.emit_kinds.extend(allowed_kinds.iter().cloned());
                }
            }
        }
        set
    }

    pub fn check_http(&self, url: &reqwest::Url) -> Result<()> {
        let host = url.host_str().ok_or_else(|| OrchestratorError::InvalidUrl(url.to_string()))?;
        if self.http_hosts.iter().any(|m| m.matches(host)) {
            Ok(())
        } else {
            Err(OrchestratorError::CapabilityDenied(format!("http host '{host}' not allowed")))
        }
    }

    pub fn check_tcp(&self, host: &str, port: u16) -> Result<()> {
        if !self.tcp_ports.contains(&port) {
            return Err(OrchestratorError::CapabilityDenied(format!("tcp port {port} not allowed")));
        }
        if !self.tcp_hosts.is_empty() && !self.tcp_hosts.iter().any(|m| m.matches(host)) {
            return Err(OrchestratorError::CapabilityDenied(format!("tcp host '{host}' not allowed")));
        }
        Ok(())
    }

    pub fn check_db_table(&self, table: &str) -> Result<()> {
        if self.db_tables.is_empty() {
            return Err(OrchestratorError::CapabilityDenied("no db access".into()));
        }
        if self.db_tables.iter().any(|t| t == table) {
            Ok(())
        } else {
            Err(OrchestratorError::CapabilityDenied(format!("db table '{table}' not allowed")))
        }
    }

    pub fn check_log(&self) -> Result<()> {
        if self.allow_log { Ok(()) } else { Err(OrchestratorError::CapabilityDenied("log not allowed".into())) }
    }

    pub fn check_emit_event(&self, kind: &str) -> Result<()> {
        if self.emit_kinds.is_empty() {
            return Err(OrchestratorError::CapabilityDenied("emit-event not allowed".into()));
        }
        if self.emit_kinds.iter().any(|k| k == "*" || k == kind) {
            Ok(())
        } else {
            Err(OrchestratorError::CapabilityDenied(format!("emit kind '{kind}' not allowed")))
        }
    }
}
