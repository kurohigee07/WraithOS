use crate::error::{OrchestratorError, Result};
use shared_types::Capability;

#[derive(Debug, Clone)]
enum HostMatcher {
    Wildcard,
    Exact(String),
    Suffix(String),
}

impl HostMatcher {
    fn parse(spec: &str) -> Self {
        if spec == "*" {
            return Self::Wildcard;
        }
        if let Some(sfx) = spec.strip_prefix("*.") {
            Self::Suffix(sfx.to_ascii_lowercase())
        } else {
            Self::Exact(spec.to_ascii_lowercase())
        }
    }
    fn matches(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        match self {
            Self::Wildcard => true,
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
                    set.http_hosts
                        .extend(allowed_hosts.iter().map(|h| HostMatcher::parse(h)));
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
        let host = url
            .host_str()
            .ok_or_else(|| OrchestratorError::InvalidUrl(url.to_string()))?;
        if self.http_hosts.iter().any(|m| m.matches(host)) {
            Ok(())
        } else {
            Err(OrchestratorError::CapabilityDenied(format!(
                "http host '{host}' not allowed"
            )))
        }
    }

    pub fn check_tcp(&self, host: &str, port: u16) -> Result<()> {
        if !self.tcp_ports.contains(&port) {
            return Err(OrchestratorError::CapabilityDenied(format!(
                "tcp port {port} not allowed"
            )));
        }
        if !self.tcp_hosts.is_empty() && !self.tcp_hosts.iter().any(|m| m.matches(host)) {
            return Err(OrchestratorError::CapabilityDenied(format!(
                "tcp host '{host}' not allowed"
            )));
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
            Err(OrchestratorError::CapabilityDenied(format!(
                "db table '{table}' not allowed"
            )))
        }
    }

    pub fn check_log(&self) -> Result<()> {
        if self.allow_log {
            Ok(())
        } else {
            Err(OrchestratorError::CapabilityDenied(
                "log not allowed".into(),
            ))
        }
    }

    pub fn check_emit_event(&self, kind: &str) -> Result<()> {
        if self.emit_kinds.is_empty() {
            return Err(OrchestratorError::CapabilityDenied(
                "emit-event not allowed".into(),
            ));
        }
        if self.emit_kinds.iter().any(|k| k == "*" || k == kind) {
            Ok(())
        } else {
            Err(OrchestratorError::CapabilityDenied(format!(
                "emit kind '{kind}' not allowed"
            )))
        }
    }

    /// Return Ok if every capability in `self` is permitted by `ceiling`.
    /// Used when a caller submits X-Module-Capabilities on upload: the
    /// requested set must be a subset of the deployment-wide ceiling,
    /// otherwise the module would run with more authority than the
    /// admin allowed (CVE-class: capability ceiling bypass).
    pub fn is_subset_of(&self, ceiling: &CapabilitySet) -> std::result::Result<(), String> {
        for req in &self.http_hosts {
            if !ceiling.http_hosts.iter().any(|c| host_subsumes(c, req)) {
                return Err(format!(
                    "http host matcher {req:?} not permitted by ceiling"
                ));
            }
        }
        for port in &self.tcp_ports {
            if !ceiling.tcp_ports.contains(port) {
                return Err(format!("tcp port {port} not permitted by ceiling"));
            }
        }
        for req in &self.tcp_hosts {
            if !ceiling.tcp_hosts.iter().any(|c| host_subsumes(c, req)) {
                return Err(format!("tcp host matcher {req:?} not permitted by ceiling"));
            }
        }
        for table in &self.db_tables {
            if !ceiling.db_tables.contains(table) {
                return Err(format!("db table '{table}' not permitted by ceiling"));
            }
        }
        for kind in &self.emit_kinds {
            if !ceiling.emit_kinds.iter().any(|c| c == "*" || c == kind) {
                return Err(format!("emit kind '{kind}' not permitted by ceiling"));
            }
        }
        if self.allow_log && !ceiling.allow_log {
            return Err("log capability not permitted by ceiling".into());
        }
        Ok(())
    }
}

/// True when every host that matches `requested` is also matched by
/// `ceiling`. Used by CapabilitySet::is_subset_of. The only asymmetric
/// case is wildcard: a ceiling wildcard subsumes anything; a requested
/// wildcard is only subsumed by a ceiling wildcard.
fn host_subsumes(ceiling: &HostMatcher, requested: &HostMatcher) -> bool {
    match (ceiling, requested) {
        (HostMatcher::Wildcard, _) => true,
        (_, HostMatcher::Wildcard) => false,
        (HostMatcher::Suffix(c), HostMatcher::Suffix(r)) => c == r,
        (HostMatcher::Suffix(c), HostMatcher::Exact(r)) => {
            r.len() > c.len() + 1 && r.ends_with(c) && r.as_bytes()[r.len() - c.len() - 1] == b'.'
        }
        (HostMatcher::Exact(c), HostMatcher::Exact(r)) => c == r,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_matches_all() {
        let caps = CapabilitySet::from_capabilities(&[shared_types::Capability::HttpGet {
            allowed_hosts: vec!["*".to_string()],
        }]);
        assert!(caps
            .check_http(&reqwest::Url::parse("https://example.com/").unwrap())
            .is_ok());
        assert!(caps
            .check_http(&reqwest::Url::parse("https://evil.internal/").unwrap())
            .is_ok());
    }

    #[test]
    fn exact_match_only() {
        let caps = CapabilitySet::from_capabilities(&[shared_types::Capability::HttpGet {
            allowed_hosts: vec!["example.com".to_string()],
        }]);
        assert!(caps
            .check_http(&reqwest::Url::parse("https://example.com/").unwrap())
            .is_ok());
        assert!(caps
            .check_http(&reqwest::Url::parse("https://evil.com/").unwrap())
            .is_err());
    }

    #[test]
    fn subset_of_enforces_ceiling() {
        let ceiling = CapabilitySet::from_capabilities(&[
            Capability::HttpGet {
                allowed_hosts: vec!["api.example.com".into()],
            },
            Capability::TcpConnect {
                allowed_ports: vec![443],
            },
        ]);
        let subset = CapabilitySet::from_capabilities(&[Capability::HttpGet {
            allowed_hosts: vec!["api.example.com".into()],
        }]);
        assert!(subset.is_subset_of(&ceiling).is_ok());
        let over = CapabilitySet::from_capabilities(&[Capability::HttpGet {
            allowed_hosts: vec!["*".into()],
        }]);
        assert!(over.is_subset_of(&ceiling).is_err());
        let over_port = CapabilitySet::from_capabilities(&[Capability::TcpConnect {
            allowed_ports: vec![8443],
        }]);
        assert!(over_port.is_subset_of(&ceiling).is_err());
    }
}
