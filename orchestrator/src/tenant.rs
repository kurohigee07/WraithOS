//! Multi-tenant registry + auth + storage migration.
//!
//! Tenant identity: path segment `/t/{tenant}/...` or explicit default.
//! Auth: Bearer token verified against argon2 PHC hash (or plaintext dev).
//! Storage: `modules/{tenant}/{id}.wasm`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context as _;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::http::HeaderValue;
use tracing::{info, warn};

use crate::config::TenantConfig;

#[derive(Debug)]
pub enum AuthError {
    UnknownTenant,
    MissingToken,
    BadToken,
    Internal,
}

pub struct TenantRegistry {
    entries: HashMap<String, TenantConfig>,
    default: String,
}

impl TenantRegistry {
    pub fn from_config(
        mut entries: HashMap<String, TenantConfig>,
        default: String,
    ) -> anyhow::Result<Self> {
        entries.entry(default.clone()).or_default();

        for (name, cfg) in &entries {
            if let Some(h) = &cfg.token_hash {
                PasswordHash::new(h).with_context(|| {
                    format!("tenant '{name}' token_hash not a valid argon2 PHC string")
                })?;
            }
            if cfg.token_plain.is_some() {
                warn!(
                    tenant = %name,
                    "tenant uses token_plain (not for production); use token_hash"
                );
            }
        }

        info!(
            count = entries.len(),
            default = %default,
            "tenant registry initialized"
        );
        Ok(Self { entries, default })
    }

    pub fn default_name(&self) -> &str {
        &self.default
    }

    pub fn contains(&self, tenant: &str) -> bool {
        self.entries.contains_key(tenant)
    }

    pub fn verify(
        &self,
        tenant: &str,
        auth: Option<&HeaderValue>,
    ) -> Result<(), AuthError> {
        let entry = self.entries.get(tenant).ok_or(AuthError::UnknownTenant)?;

        // No auth configured → open (typical for default tenant, localhost-bound).
        if entry.token_hash.is_none() && entry.token_plain.is_none() {
            return Ok(());
        }

        let token = extract_bearer(auth).ok_or(AuthError::MissingToken)?;

        if let Some(hash) = &entry.token_hash {
            let parsed = PasswordHash::new(hash).map_err(|_| AuthError::Internal)?;
            return Argon2::default()
                .verify_password(token.as_bytes(), &parsed)
                .map_err(|_| AuthError::BadToken);
        }

        if let Some(plain) = &entry.token_plain {
            if ct_eq(token.as_bytes(), plain.as_bytes()) {
                return Ok(());
            }
            return Err(AuthError::BadToken);
        }

        Ok(())
    }
}

fn extract_bearer(h: Option<&HeaderValue>) -> Option<&str> {
    let h = h?;
    let s = h.to_str().ok()?;
    s.strip_prefix("Bearer ")
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Move flat `modules/*.wasm` (and sidecars) to `modules/default/`.
/// Idempotent: files already in default/ are left alone.
pub fn migrate_flat_to_default(dir: &Path) -> anyhow::Result<usize> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(0);
        }
        Err(e) => {
            return Err(anyhow::anyhow!("read {}: {e}", dir.display()));
        }
    };

    let default_dir = dir.join("default");
    let mut moved = 0usize;

    for entry_res in entries {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let is_module_file = name.ends_with(".wasm")
            || name.ends_with(".wasm.sig")
            || name.ends_with(".wasm.quota.json");
        if !is_module_file {
            continue;
        }

        std::fs::create_dir_all(&default_dir)
            .with_context(|| format!("create {}", default_dir.display()))?;
        let dest = default_dir.join(name);
        if dest.exists() {
            warn!(src = %path.display(), dest = %dest.display(), "migration: dest exists, skip");
            continue;
        }
        std::fs::rename(&path, &dest).with_context(|| {
            format!("move {} -> {}", path.display(), dest.display())
        })?;
        moved += 1;
    }

    if moved > 0 {
        info!(moved, "migrated flat modules -> default/");
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_basic() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }

    #[test]
    fn verify_no_auth_allows() {
        let reg = TenantRegistry::from_config(HashMap::new(), "default".into())
            .expect("registry");
        assert!(reg.verify("default", None).is_ok());
        assert!(matches!(
            reg.verify("missing", None),
            Err(AuthError::UnknownTenant)
        ));
    }

    #[test]
    fn verify_plain_token() {
        let mut m = HashMap::new();
        m.insert(
            "alice".to_string(),
            TenantConfig {
                token_hash: None,
                token_plain: Some("secret".into()),
            },
        );
        let reg = TenantRegistry::from_config(m, "default".into()).expect("reg");

        let ok = HeaderValue::from_str("Bearer secret").unwrap();
        let bad = HeaderValue::from_str("Bearer wrong").unwrap();
        let no = HeaderValue::from_str("Basic xyz").unwrap();

        assert!(reg.verify("alice", Some(&ok)).is_ok());
        assert!(matches!(reg.verify("alice", Some(&bad)), Err(AuthError::BadToken)));
        assert!(matches!(reg.verify("alice", Some(&no)), Err(AuthError::MissingToken)));
        assert!(matches!(reg.verify("alice", None), Err(AuthError::MissingToken)));
    }
}
