use std::sync::Arc;
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::capabilities::CapabilitySet;
use crate::context::AppContext;

pub struct WasmHostState {
    pub ctx: Arc<AppContext>,
    pub capabilities: Arc<CapabilitySet>,
    pub wasi: WasiCtx,
    pub table: ResourceTable,
    pub limits: StoreLimits,
    pub module_id: String,
}

impl WasmHostState {
    pub fn new(
        ctx: Arc<AppContext>,
        capabilities: Arc<CapabilitySet>,
        memory_limit_mb: u64,
        module_id: String,
    ) -> Self {
        let memory_bytes = (memory_limit_mb as usize).saturating_mul(1024 * 1024);
        let limits = StoreLimitsBuilder::new()
            .memory_size(memory_bytes)
            .trap_on_grow_failure(true)
            .build();
        // Do not inherit_stdio: guest gets dummy stdio. Any module that
        // wants to log must use the wasm log capability, which goes
        // through host_impls and respects CapabilitySet. Leaving
        // inherit_stdio on lets a guest write to host stderr and read
        // host stdin, which is unintended I/O leakage.
        let wasi = WasiCtxBuilder::new().build();
        Self {
            ctx,
            capabilities,
            wasi,
            table: ResourceTable::new(),
            limits,
            module_id,
        }
    }
}

impl WasiView for WasmHostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}
