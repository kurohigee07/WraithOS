pub mod host_impls;
pub mod module;
pub mod state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use wasmtime::component::bindgen;
use wasmtime::{Config, Engine};

bindgen!({
    path: "wit",
    world: "plugin",
    imports: { default: async },
    exports: { default: async },
});

/// Interval tick epoch. 100ms → resolusi timeout guest 100ms.
pub const EPOCH_TICK: Duration = Duration::from_millis(100);

/// Batas eksekusi guest dalam jumlah tick. 600 * 100ms = 60s.
/// Set lebih besar dari HTTP timeout (30s) untuk headroom.
pub const EPOCH_DEADLINE_TICKS: u64 = 600;

pub fn build_engine() -> anyhow::Result<Arc<Engine>> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.epoch_interruption(true);
    let engine = Engine::new(&config)
        .map_err(|e| anyhow::anyhow!("failed to create wasmtime engine: {e}"))?;
    Ok(Arc::new(engine))
}

/// Background thread yang menaikkan epoch tiap EPOCH_TICK.
/// Guest yang melewati deadline akan trap dengan "epoch deadline reached".
pub struct EpochTicker {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl EpochTicker {
    pub fn spawn(engine: Arc<Engine>, tick: Duration) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        let handle = std::thread::Builder::new()
            .name("wasm-epoch-ticker".into())
            .spawn(move || {
                while !shutdown_clone.load(Ordering::Relaxed) {
                    std::thread::sleep(tick);
                    engine.increment_epoch();
                }
            })
            .ok();
        Self { shutdown, handle }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            // Thread butuh hingga `tick` untuk wake; 100ms shutdown lag acceptable.
            let _ = h.join();
        }
    }
}
