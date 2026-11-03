//! Multi-tenant registry + auth + per-tenant resource caps (M18).
//!
//! Tenant identity: path segment `/t/{tenant}/...` or explicit default.
//! Auth: Bearer token via argon2 PHC (or plaintext dev).
//! Storage: `modules/{tenant}/{id}.wasm`.
//!
//! M18 caps (all optional, None = unlimited):
//!   max_modules, max_concurrent_wasm, rate_limit_per_sec

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::http::HeaderValue;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{info, warn};

use crate::config::TenantConfig;
use crate::quota::TokenBucket;

#[derive(Debug)]
pub enum ConcurrencyError {
    Timeout,
    Closed,
}

#[derive(Debug)]
pub enum AuthError {
    UnknownTenant,
    MissingToken,
    BadToken,
    Internal,
}

// ── TenantPermit (RAII with gauge) ──────────────────────────────────
pub struct TenantPermit {
    _permit: OwnedSemaphorePermit,
    in_flight: Arc<AtomicUsize>,
    tenant: String,
}

impl Drop for TenantPermit {
    fn drop(&mut self) {
        let n = self
            .in_flight
            .fetch_sub(1, Ordering::Relaxed)
            .saturating_sub(1);
        metrics::gauge!("tenant_in_flight", "tenant" => self.tenant.clone()).set(n as f64);
    }
}

// ── TenantState ─────────────────────────────────────────────────────
pub struct TenantState {
    pub name: String,
    pub max_modules: Option<usize>,
    rate: Option<TokenBucket>,
    concurrency: Option<Arc<Semaphore>>,
    acquire_timeout: Duration,
    in_flight: Arc<AtomicUsize>,
}

impl TenantState {
    fn from_config(name: &str, cfg: &TenantConfig, acquire_timeout: Duration) -> Self {
        let rate = cfg.rate_limit_per_sec.map(TokenBucket::new);
        let concurrency = cfg
            .max_concurrent_wasm
            .filter(|n| *n > 0)
            .map(|n| Arc::new(Semaphore::new(n as usize)));
        Self {
            name: name.to_string(),
            max_modules: cfg.max_modules,
            rate,
            concurrency,
            acquire_timeout,
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn try_acquire_rate(&self) -> Result<(), u64> {
        match &self.rate {
            Some(b) => b.try_acquire(),
            None => Ok(()),
        }
    }

    async fn acquire_concurrency(&self) -> Result<Option<TenantPermit>, ConcurrencyError> {
        match &self.concurrency {
            Some(sem) => {
                let permit =
                    tokio::time::timeout(self.acquire_timeout, sem.clone().acquire_owned())
                        .await
                        .map_err(|_| ConcurrencyError::Timeout)?
                        .map_err(|_| ConcurrencyError::Closed)?;
                let n = self.in_flight.fetch_add(1, Ordering::Relaxed) + 1;
                metrics::gauge!("tenant_in_flight", "tenant" => self.name.clone()).set(n as f64);
                Ok(Some(TenantPermit {
                    _permit: permit,
                    in_flight: self.in_flight.clone(),
                    tenant: self.name.clone(),
                }))
            }
            None => Ok(None),
        }
    }

    fn check_capacity(&self, current_count: usize) -> Result<(), (usize, usize)> {
        match self.max_modules {
            Some(max) if current_count >= max => Err((current_count, max)),
            _ => Ok(()),
        }
    }
}

// ── TenantRegistry ──────────────────────────────────────────────────
struct TenantEntry {
    cfg: TenantConfig,
    state: Arc<TenantState>,
}

pub struct TenantRegistry {
    entries: HashMap<String, TenantEntry>,
    default: String,
}

impl TenantRegistry {
    pub fn from_config(
        mut entries: HashMap<String, TenantConfig>,
        default: String,
        acquire_timeout: Duration,
    ) -> anyhow::Result<Self> {
        entries.entry(default.clone()).or_default();

        let mut built = HashMap::new();
        for (name, cfg) in entries {
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
            let state = Arc::new(TenantState::from_config(&name, &cfg, acquire_timeout));
            built.insert(name, TenantEntry { cfg, state });
        }

        info!(
            count = built.len(),
            default = %default,
            "tenant registry initialized"
        );
        Ok(Self {
            entries: built,
            default,
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn default_name(&self) -> &str {
        &self.default
    }

    pub fn contains(&self, tenant: &str) -> bool {
        self.entries.contains_key(tenant)
    }

    pub fn verify(&self, tenant: &str, auth: Option<&HeaderValue>) -> Result<(), AuthError> {
        let entry = self.entries.get(tenant).ok_or(AuthError::UnknownTenant)?;
        let cfg = &entry.cfg;

        if cfg.token_hash.is_none() && cfg.token_plain.is_none() {
            return Ok(());
        }

        let token = extract_bearer(auth).ok_or(AuthError::MissingToken)?;

        if let Some(hash) = &cfg.token_hash {
            let parsed = PasswordHash::new(hash).map_err(|_| AuthError::Internal)?;
            return Argon2::default()
                .verify_password(token.as_bytes(), &parsed)
                .map_err(|_| AuthError::BadToken);
        }

        if let Some(plain) = &cfg.token_plain {
            if ct_eq(token.as_bytes(), plain.as_bytes()) {
                return Ok(());
            }
            return Err(AuthError::BadToken);
        }

        Ok(())
    }

    // ── M18 rate/concurrency/capacity accessors ──────────────────────
    pub fn try_acquire_rate(&self, tenant: &str) -> Result<(), u64> {
        match self.entries.get(tenant) {
            Some(e) => e.state.try_acquire_rate(),
            None => Ok(()),
        }
    }

    pub async fn acquire_concurrency(
        &self,
        tenant: &str,
    ) -> Result<Option<TenantPermit>, ConcurrencyError> {
        match self.entries.get(tenant) {
            Some(e) => e.state.acquire_concurrency().await,
            None => Ok(None),
        }
    }

    pub fn check_capacity(&self, tenant: &str, current_count: usize) -> Result<(), (usize, usize)> {
        match self.entries.get(tenant) {
            Some(e) => e.state.check_capacity(current_count),
            None => Ok(()),
        }
    }

    pub fn max_modules(&self, tenant: &str) -> Option<usize> {
        self.entries.get(tenant).and_then(|e| e.state.max_modules)
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

pub fn migrate_flat_to_default(dir: &Path) -> anyhow::Result<usize> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(0);
        }
        Err(e) => return Err(anyhow::anyhow!("read {}: {e}", dir.display())),
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
        std::fs::rename(&path, &dest)
            .with_context(|| format!("move {} -> {}", path.display(), dest.display()))?;
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

    fn cfg_with_caps(
        rate: Option<u32>,
        conc: Option<u32>,
        max_mods: Option<usize>,
    ) -> TenantConfig {
        TenantConfig {
            token_hash: None,
            token_plain: None,
            max_modules: max_mods,
            max_concurrent_wasm: conc,
            rate_limit_per_sec: rate,
        }
    }

    #[test]
    fn ct_eq_basic() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }

    #[test]
    fn verify_no_auth_allows() {
        let reg = TenantRegistry::from_config(
            HashMap::new(),
            "default".into(),
            Duration::from_millis(100),
        )
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
                ..Default::default()
            },
        );
        let reg = TenantRegistry::from_config(m, "default".into(), Duration::from_millis(100))
            .expect("reg");

        let ok = HeaderValue::from_str("Bearer secret").unwrap();
        let bad = HeaderValue::from_str("Bearer wrong").unwrap();
        let no = HeaderValue::from_str("Basic xyz").unwrap();

        assert!(reg.verify("alice", Some(&ok)).is_ok());
        assert!(matches!(
            reg.verify("alice", Some(&bad)),
            Err(AuthError::BadToken)
        ));
        assert!(matches!(
            reg.verify("alice", Some(&no)),
            Err(AuthError::MissingToken)
        ));
        assert!(matches!(
            reg.verify("alice", None),
            Err(AuthError::MissingToken)
        ));
    }

    #[test]
    fn tenant_rate_limit() {
        let mut m = HashMap::new();
        m.insert("alice".into(), cfg_with_caps(Some(2), None, None));
        let reg = TenantRegistry::from_config(m, "default".into(), Duration::from_millis(100))
            .expect("reg");

        assert!(reg.try_acquire_rate("alice").is_ok());
        assert!(reg.try_acquire_rate("alice").is_ok());
        assert!(reg.try_acquire_rate("alice").is_err());
        // default has no rate configured → unlimited
        for _ in 0..100 {
            assert!(reg.try_acquire_rate("default").is_ok());
        }
    }

    #[tokio::test]
    async fn tenant_concurrency() {
        let mut m = HashMap::new();
        m.insert("alice".into(), cfg_with_caps(None, Some(1), None));
        let reg = TenantRegistry::from_config(m, "default".into(), Duration::from_millis(50))
            .expect("reg");

        let p = reg.acquire_concurrency("alice").await.expect("first");
        assert!(reg.acquire_concurrency("alice").await.is_err());
        drop(p);
        let _p2 = reg
            .acquire_concurrency("alice")
            .await
            .expect("after release");
    }

    #[test]
    fn tenant_module_capacity() {
        let mut m = HashMap::new();
        m.insert("alice".into(), cfg_with_caps(None, None, Some(3)));
        let reg = TenantRegistry::from_config(m, "default".into(), Duration::from_millis(100))
            .expect("reg");

        assert!(reg.check_capacity("alice", 0).is_ok());
        assert!(reg.check_capacity("alice", 2).is_ok());
        assert_eq!(reg.check_capacity("alice", 3), Err((3, 3)));
        assert_eq!(reg.check_capacity("alice", 5), Err((5, 3)));
        // default unlimited
        assert!(reg.check_capacity("default", 999999).is_ok());
    }
}
