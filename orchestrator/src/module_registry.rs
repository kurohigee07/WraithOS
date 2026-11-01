//! Registry modul. Loader wasm butuh shared Arc<Engine> + optional
//! signature verification + per-module quota (sidecar).

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use dashmap::DashMap;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use shared_types::{ModuleManifest, ModuleQuota};
use tracing::{info, warn};
use wasmtime::Engine;

use crate::fs_util::atomic_write;
use crate::quota::ModuleQuotaState;
use crate::wasm::module::WasmModule;

pub struct ModuleRegistry {
    engine: Arc<Engine>,
    trusted_keys: Vec<String>,
    default_quota: ModuleQuota,
    global_memory_limit_mb: u64,
    acquire_timeout: Duration,
    wasm: DashMap<String, Arc<WasmModule>>,
}

impl ModuleRegistry {
    pub fn new(
        engine: Arc<Engine>,
        trusted_keys: Vec<String>,
        default_quota: ModuleQuota,
        global_memory_limit_mb: u64,
        acquire_timeout: Duration,
    ) -> Self {
        Self {
            engine,
            trusted_keys,
            default_quota,
            global_memory_limit_mb,
            acquire_timeout,
            wasm: DashMap::new(),
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────
    pub fn count(&self) -> usize { self.wasm.len() }

    pub fn all(&self) -> Vec<(String, Arc<WasmModule>)> {
        self.wasm.iter().map(|e| (e.key().clone(), e.value().clone())).collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<WasmModule>> {
        self.wasm.get(id).map(|e| e.value().clone())
    }

    pub fn has(&self, id: &str) -> bool { self.wasm.contains_key(id) }

    pub fn engine(&self) -> Arc<Engine> { self.engine.clone() }

    pub fn default_quota(&self) -> ModuleQuota { self.default_quota.clone() }

    pub fn global_memory_limit_mb(&self) -> u64 { self.global_memory_limit_mb }

    pub fn acquire_timeout(&self) -> Duration { self.acquire_timeout }

    // ── Mutation ──────────────────────────────────────────────────────
    pub fn insert_if_absent(
        &self,
        id: String,
        module: Arc<WasmModule>,
    ) -> Result<(), Arc<WasmModule>> {
        if self.wasm.contains_key(&id) {
            return Err(module);
        }
        self.wasm.insert(id, module);
        Ok(())
    }

    pub fn insert_replace(
        &self,
        id: String,
        module: Arc<WasmModule>,
    ) -> Option<Arc<WasmModule>> {
        self.wasm.insert(id, module)
    }

    pub fn remove(&self, id: &str) -> Option<Arc<WasmModule>> {
        self.wasm.remove(id).map(|(_, m)| m)
    }

    // ── Signature ─────────────────────────────────────────────────────
    pub fn has_trusted_keys(&self) -> bool { !self.trusted_keys.is_empty() }

    pub fn verify_signature(&self, bytes: &[u8], sig_bytes: &[u8]) -> anyhow::Result<()> {
        if self.trusted_keys.is_empty() {
            anyhow::bail!("no trusted keys configured");
        }
        verify_bytes(bytes, sig_bytes, &self.trusted_keys)
    }

    // ── Build quota state (validates against global cap) ─────────────
    pub fn build_quota_state(&self, quota: &ModuleQuota) -> anyhow::Result<Arc<ModuleQuotaState>> {
        if quota.max_memory_mb > 0 && quota.max_memory_mb > self.global_memory_limit_mb {
            anyhow::bail!(
                "max_memory_mb ({}) exceeds global cap ({})",
                quota.max_memory_mb,
                self.global_memory_limit_mb
            );
        }
        Ok(Arc::new(ModuleQuotaState::from_quota(quota, self.acquire_timeout)))
    }

    // ── Loader from disk (startup) ────────────────────────────────────
    pub fn load_wasm_dir(&self, dir: &std::path::Path) -> anyhow::Result<usize> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(dir = %dir.display(), "modules dir not found; skipping");
                return Ok(0);
            }
            Err(e) => {
                return Err(anyhow::anyhow!("read modules dir {}: {e}", dir.display()));
            }
        };

        let mut loaded = 0usize;
        for entry_res in entries {
            let entry = match entry_res {
                Ok(e) => e,
                Err(e) => { warn!(err = %e, "skip unreadable dir entry"); continue; }
            };
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("wasm") { continue; }
            if path.file_name().and_then(|s| s.to_str())
                .map(|s| s.starts_with('.')).unwrap_or(false) { continue; }

            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => { warn!(path = %path.display(), err = %e, "skip: read failed"); continue; }
            };

            if !self.trusted_keys.is_empty() {
                let sig_path = path.with_extension("wasm.sig");
                let sig_bytes = match std::fs::read(&sig_path) {
                    Ok(b) => b,
                    Err(e) => {
                        warn!(path = %path.display(), err = %e, "skip: sig missing");
                        continue;
                    }
                };
                if let Err(e) = verify_bytes(&bytes, &sig_bytes, &self.trusted_keys) {
                    warn!(path = %path.display(), err = %e, "skip: bad sig");
                    continue;
                }
                info!(path = %path.display(), "wasm signature verified");
            }

            let file_stem = path.file_stem().and_then(|s| s.to_str())
                .unwrap_or("unknown").to_string();

            // Read sidecar quota if present; else use default.
            let quota = read_quota_sidecar(dir, &file_stem)
                .unwrap_or_else(|| self.default_quota.clone());

            let quota_state = match self.build_quota_state(&quota) {
                Ok(s) => s,
                Err(e) => {
                    warn!(path = %path.display(), err = %e, "skip: invalid quota");
                    continue;
                }
            };

            let manifest = ModuleManifest {
                id: file_stem.clone(),
                version: "0.0.0".to_string(),
                capabilities: vec![],
                tenant: "default".to_string(),
                quota,
            };

            match WasmModule::load(self.engine.clone(), manifest, &bytes, quota_state) {
                Ok(m) => {
                    info!(id = %file_stem, path = %path.display(), "wasm module loaded");
                    self.wasm.insert(file_stem, Arc::new(m));
                    loaded += 1;
                }
                Err(e) => warn!(path = %path.display(), err = %e, "skip: load failed"),
            }
        }
        Ok(loaded)
    }
}

fn read_quota_sidecar(dir: &std::path::Path, id: &str) -> Option<ModuleQuota> {
    let path = dir.join(format!("{id}.wasm.quota.json"));
    let s = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<ModuleQuota>(&s) {
        Ok(q) => Some(q),
        Err(e) => {
            warn!(path = %path.display(), err = %e, "sidecar parse failed");
            None
        }
    }
}

pub(crate) fn write_quota_sidecar(
    dir: &std::path::Path,
    id: &str,
    q: &ModuleQuota,
) -> anyhow::Result<()> {
    let path = dir.join(format!("{id}.wasm.quota.json"));
    let json = serde_json::to_vec_pretty(q)
        .context("serialize quota")?;
    atomic_write(&path, &json)
        .map_err(|e| anyhow::anyhow!("write sidecar {}: {e}", path.display()))
}

fn verify_bytes(
    wasm_bytes: &[u8],
    sig_bytes: &[u8],
    trusted_keys: &[String],
) -> anyhow::Result<()> {
    if sig_bytes.len() != 64 {
        anyhow::bail!("signature length {} != 64", sig_bytes.len());
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(sig_bytes);
    let sig = Signature::from_bytes(&sig_arr);

    for key_hex in trusted_keys {
        let key_bytes = hex::decode(key_hex.trim())
            .with_context(|| format!("decode hex key '{key_hex}'"))?;
        if key_bytes.len() != 32 {
            warn!(len = key_bytes.len(), "trusted key not 32 bytes; skipping");
            continue;
        }
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&key_bytes);
        let vk = match VerifyingKey::from_bytes(&key_arr) {
            Ok(v) => v,
            Err(e) => {
                warn!(key = %key_hex, err = %e, "trusted key invalid; skipping");
                continue;
            }
        };
        if vk.verify(wasm_bytes, &sig).is_ok() {
            return Ok(());
        }
    }
    anyhow::bail!("no trusted key verified the signature")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const TEST_SECRET_HEX: &str =
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

    fn test_signing_key() -> SigningKey {
        let bytes = hex::decode(TEST_SECRET_HEX).expect("test secret hex");
        let arr: [u8; 32] = bytes.try_into().expect("secret length");
        SigningKey::from_bytes(&arr)
    }

    #[test]
    fn verify_accepts_valid_signature() {
        let sk = test_signing_key();
        let vk_hex = hex::encode(sk.verifying_key().to_bytes());
        let msg = b"hello world";
        let sig = sk.sign(msg);
        verify_bytes(msg, &sig.to_bytes(), &[vk_hex]).expect("valid sig must verify");
    }

    #[test]
    fn verify_rejects_tampered_message() {
        let sk = test_signing_key();
        let vk_hex = hex::encode(sk.verifying_key().to_bytes());
        let sig = sk.sign(b"hello world");
        assert!(verify_bytes(b"tampered", &sig.to_bytes(), &[vk_hex]).is_err());
    }

    #[test]
    fn verify_rejects_wrong_key() {
        let sk = test_signing_key();
        let other_sk = SigningKey::from_bytes(&[7u8; 32]);
        let other_vk = hex::encode(other_sk.verifying_key().to_bytes());
        let sig = sk.sign(b"hello");
        assert!(verify_bytes(b"hello", &sig.to_bytes(), &[other_vk]).is_err());
    }

    #[test]
    fn verify_rejects_bad_sig_length() {
        let sk = test_signing_key();
        let vk_hex = hex::encode(sk.verifying_key().to_bytes());
        assert!(verify_bytes(b"x", &[0u8; 10], &[vk_hex]).is_err());
    }
}
