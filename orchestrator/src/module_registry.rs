//! Registry modul yang aktif. Loader wasm butuh shared Arc<Engine>.

use std::sync::Arc;

use dashmap::DashMap;
use shared_types::ModuleManifest;
use tracing::{info, warn};
use wasmtime::Engine;

use crate::wasm::module::WasmModule;

pub struct ModuleRegistry {
    engine: Arc<Engine>,
    wasm: DashMap<String, Arc<WasmModule>>,
}

impl ModuleRegistry {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self { engine, wasm: DashMap::new() }
    }

    pub fn load_wasm_dir(&self, dir: &std::path::Path) -> anyhow::Result<usize> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(dir = %dir.display(), "modules dir not found; skipping wasm load");
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

            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => { warn!(path = %path.display(), err = %e, "skip wasm: read failed"); continue; }
            };

            let file_stem = path.file_stem().and_then(|s| s.to_str())
                .unwrap_or("unknown").to_string();

            let manifest = ModuleManifest {
                id: file_stem.clone(),
                version: "0.0.0".to_string(),
                capabilities: vec![],
            };

            match WasmModule::load(self.engine.clone(), manifest, &bytes) {
                Ok(m) => {
                    info!(id = %file_stem, path = %path.display(), "wasm module loaded");
                    self.wasm.insert(file_stem, Arc::new(m));
                    loaded += 1;
                }
                Err(e) => warn!(path = %path.display(), err = %e, "skip wasm: load failed"),
            }
        }
        Ok(loaded)
    }

    pub fn count(&self) -> usize { self.wasm.len() }

    pub fn all(&self) -> Vec<(String, Arc<WasmModule>)> {
        self.wasm.iter().map(|e| (e.key().clone(), e.value().clone())).collect()
    }
}
