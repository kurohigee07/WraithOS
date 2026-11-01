use std::sync::Arc;
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::context::AppContext;

pub struct WasmHostState {
    pub ctx: Arc<AppContext>,
    pub wasi: WasiCtx,
    pub table: ResourceTable,
    pub limits: StoreLimits,
}

impl WasmHostState {
    pub fn new(ctx: Arc<AppContext>, memory_limit_mb: u64) -> Self {
        let memory_bytes = (memory_limit_mb as usize).saturating_mul(1024 * 1024);
        let limits = StoreLimitsBuilder::new()
            .memory_size(memory_bytes)
            .trap_on_grow_failure(true)
            .build();
        let wasi = WasiCtxBuilder::new().inherit_stdio().build();
        Self { ctx, wasi, table: ResourceTable::new(), limits }
    }
}

impl WasiView for WasmHostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}
