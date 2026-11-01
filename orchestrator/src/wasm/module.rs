use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use shared_types::ModuleManifest;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};

use crate::context::AppContext;
use crate::module::OrchestratorModule;
use crate::wasm::state::WasmHostState;
use crate::wasm::{Plugin, EPOCH_DEADLINE_TICKS};

pub struct WasmModule {
    manifest: ModuleManifest,
    engine: Arc<Engine>,
    component: Component,
    linker: Linker<WasmHostState>,
}

impl WasmModule {
    pub fn load(
        engine: Arc<Engine>,
        manifest: ModuleManifest,
        wasm_bytes: &[u8],
    ) -> anyhow::Result<Self> {
        let component = Component::from_binary(&engine, wasm_bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse wasm component: {e}"))?;

        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| anyhow::anyhow!("failed to add WASI p2: {e}"))?;
        Plugin::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(|e| anyhow::anyhow!("failed to add plugin host fns: {e}"))?;

        Ok(Self { manifest, engine, component, linker })
    }
}

#[async_trait]
impl OrchestratorModule for WasmModule {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }

    async fn on_load(&mut self, _ctx: Arc<AppContext>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn execute(
        &self,
        ctx: Arc<AppContext>,
        payload: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let module_id = self.manifest.id.clone();

        // Bounded concurrency: cegah guest CPU-bound menghabiskan worker pool.
        let wait_start = std::time::Instant::now();
        let _permit = match ctx.wasm_pool.acquire().await {
            Ok(p) => p,
            Err(e) => {
                metrics::counter!(
                    "wasm_executions_total",
                    "module" => module_id,
                    "result" => "saturated",
                ).increment(1);
                return Err(anyhow::anyhow!("wasm pool: {e}"));
            }
        };
        metrics::histogram!(
            "wasm_pool_wait_seconds",
            "module" => module_id.clone(),
        ).record(wait_start.elapsed().as_secs_f64());

        // Execution timing: exclude queue wait.
        let start = std::time::Instant::now();
        let result = self.execute_impl(ctx, payload).await;
        let elapsed = start.elapsed().as_secs_f64();
        let outcome = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!(
            "wasm_executions_total",
            "module" => module_id.clone(),
            "result" => outcome,
        ).increment(1);
        metrics::histogram!(
            "wasm_execution_duration_seconds",
            "module" => module_id,
        ).record(elapsed);
        result
    }

    async fn on_shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl WasmModule {
    async fn execute_impl(
        &self,
        ctx: Arc<AppContext>,
        payload: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let mut store = Store::new(&self.engine, WasmHostState::new(ctx));
        // Trap jika guest melewati EPOCH_DEADLINE_TICKS (60s default).
        store.set_epoch_deadline(EPOCH_DEADLINE_TICKS);
        // Memory limit: linear memory guest dibatasi StoreLimits.
        // trap_on_grow_failure=true: memory.grow failure = trap langsung.
        store.limiter(|state| &mut state.limits);

        let bindings = Plugin::instantiate_async(&mut store, &self.component, &self.linker)
            .await
            .map_err(|e| anyhow::anyhow!("failed to instantiate wasm component: {e}"))?;

        let payload_str =
            serde_json::to_string(&payload).context("failed to serialize payload")?;

        let outcome = bindings
            .call_run(&mut store, &payload_str)
            .await
            .map_err(|e| anyhow::anyhow!("wasm export `run` trapped: {e}"))?;

        let result_str =
            outcome.map_err(|e| anyhow::anyhow!("guest returned error: {e}"))?;

        let value = serde_json::from_str(&result_str)
            .context("failed to deserialize guest result")?;
        Ok(value)
    }
}
