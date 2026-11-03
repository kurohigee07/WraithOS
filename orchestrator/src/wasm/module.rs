use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use shared_types::ModuleManifest;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};

use crate::context::AppContext;
use crate::module::OrchestratorModule;
use crate::quota::ModuleQuotaState;
use crate::wasm::state::WasmHostState;
use crate::wasm::{Plugin, EPOCH_DEADLINE_TICKS};

pub struct WasmModule {
    manifest: ModuleManifest,
    engine: Arc<Engine>,
    component: Component,
    linker: Linker<WasmHostState>,
    quota: Arc<ModuleQuotaState>,
}

/// Error type from quota-aware execution. Distinct variants for HTTP mapping.
pub enum QuotaExecError {
    RateLimited { retry_after_ms: u64 },
    ConcurrencyTimeout,
    Internal(anyhow::Error),
}

impl std::fmt::Display for QuotaExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuotaExecError::RateLimited { retry_after_ms } => {
                write!(f, "rate limited, retry after {}ms", retry_after_ms)
            }
            QuotaExecError::ConcurrencyTimeout => write!(f, "concurrency timeout"),
            QuotaExecError::Internal(e) => write!(f, "{}", e),
        }
    }
}

impl WasmModule {
    async fn log_execution(
        db: &sqlx::SqlitePool,
        tenant: String,
        module_id: String,
        status: &'static str,
        duration_ms: u64,
        error_msg: Option<String>,
    ) {
        let _ = sqlx::query(
            "INSERT INTO execution_logs (tenant, module_id, status, duration_ms, error_message) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(tenant)
        .bind(module_id)
        .bind(status)
        .bind(duration_ms as i64)
        .bind(error_msg)
        .execute(db)
        .await;
    }

    pub fn load(
        engine: Arc<Engine>,
        manifest: ModuleManifest,
        wasm_bytes: &[u8],
        quota: Arc<ModuleQuotaState>,
    ) -> anyhow::Result<Self> {
        let component = Component::from_binary(&engine, wasm_bytes)
            .map_err(|e| anyhow::anyhow!("failed to parse wasm component: {e}"))?;

        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| anyhow::anyhow!("failed to add WASI p2: {e}"))?;
        Plugin::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(|e| anyhow::anyhow!("failed to add plugin host fns: {e}"))?;

        Ok(Self {
            manifest,
            engine,
            component,
            linker,
            quota,
        })
    }

    /// Quota-aware execution. Use from HTTP layer for proper error mapping.
    #[tracing::instrument(
        skip_all,
        fields(module = %self.manifest.id, outcome = tracing::field::Empty),
    )]
    pub async fn execute_quota(
        &self,
        ctx: Arc<AppContext>,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, QuotaExecError> {
        let module_id = self.manifest.id.clone();
        let tenant_id = self.manifest.tenant.clone();
        let span = tracing::Span::current();

        // 1. Rate limit (sync, instant reject)
        if let Err(retry_after_ms) = self.quota.try_acquire_rate() {
            metrics::counter!(
                "module_quota_rejected_total",
                "module" => module_id.clone(),
                "tenant" => tenant_id.clone(),
                "reason" => "rate",
            )
            .increment(1);
            metrics::counter!(
                "wasm_executions_total",
                "module" => module_id,
                "result" => "rate_limited",
            )
            .increment(1);
            span.record("outcome", "rate_limited");
            return Err(QuotaExecError::RateLimited { retry_after_ms });
        }

        // 2. Per-module concurrency
        let _module_permit = match self.quota.acquire_concurrency().await {
            Ok(p) => p,
            Err(_) => {
                metrics::counter!(
                    "module_quota_rejected_total",
                    "module" => module_id.clone(),
                    "reason" => "concurrency",
                )
                .increment(1);
                metrics::counter!(
                    "wasm_executions_total",
                    "module" => module_id,
                    "result" => "timeout",
                )
                .increment(1);
                span.record("outcome", "timeout");
                return Err(QuotaExecError::ConcurrencyTimeout);
            }
        };

        // 3. Global pool
        let pool_wait = Instant::now();
        let _pool_permit = match ctx.wasm_pool.acquire().await {
            Ok(p) => p,
            Err(e) => {
                metrics::counter!(
                    "wasm_executions_total",
                    "module" => module_id,
                    "result" => "error",
                )
                .increment(1);
                span.record("outcome", "error");
                return Err(QuotaExecError::Internal(anyhow::anyhow!("pool: {e}")));
            }
        };
        metrics::histogram!(
            "wasm_pool_wait_seconds",
            "module" => module_id.clone(),
            "tenant" => tenant_id.clone(),
        )
        .record(pool_wait.elapsed().as_secs_f64());

        // 4. Execute
        let start = Instant::now();
        let db_pool = ctx.db.clone();
        let result = self.execute_impl(ctx, payload).await;
        let elapsed = start.elapsed().as_secs_f64();
        let (status, err_msg) = match &result {
            Ok(_) => ("ok", None),
            Err(e) => ("error", Some(format!("{e}"))),
        };

        match &result {
            Ok(_) => {
                metrics::counter!(
                    "wasm_executions_total",
                    "module" => module_id.clone(),
                    "tenant" => tenant_id.clone(),
                    "result" => "ok",
                )
                .increment(1);
                span.record("outcome", "ok");
            }
            Err(_) => {
                metrics::counter!(
                    "wasm_executions_total",
                    "module" => module_id.clone(),
                    "tenant" => tenant_id.clone(),
                    "result" => "error",
                )
                .increment(1);
                span.record("outcome", "error");
            }
        }

        let db = db_pool;
        let t_id = tenant_id.clone();
        let m_id = module_id.clone();
        let dur = (elapsed * 1000.0) as u64;
        tokio::spawn(async move {
            Self::log_execution(&db, t_id, m_id, status, dur, err_msg).await;
        });
        metrics::histogram!(
            "wasm_execution_duration_seconds",
            "module" => module_id,
            "tenant" => tenant_id,
        )
        .record(elapsed);
        result
    }

    async fn execute_impl(
        &self,
        ctx: Arc<AppContext>,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, QuotaExecError> {
        let memory_mb = if self.manifest.quota.max_memory_mb > 0 {
            self.manifest.quota.max_memory_mb
        } else {
            ctx.config.wasm_memory_limit_mb
        };

        let mut store = Store::new(
            &self.engine,
            WasmHostState::new(ctx, memory_mb, self.manifest.id.clone()),
        );
        store.set_epoch_deadline(EPOCH_DEADLINE_TICKS);
        store.limiter(|state| &mut state.limits);

        let bindings = Plugin::instantiate_async(&mut store, &self.component, &self.linker)
            .await
            .map_err(|e| QuotaExecError::Internal(anyhow::anyhow!("instantiate: {e}")))?;

        let payload_str = serde_json::to_string(&payload)
            .context("serialize payload")
            .map_err(QuotaExecError::Internal)?;

        let outcome = bindings
            .call_run(&mut store, &payload_str)
            .await
            .map_err(|e| {
                QuotaExecError::Internal(anyhow::anyhow!("wasm export `run` trapped: {e}"))
            })?;

        let result_str =
            outcome.map_err(|e| QuotaExecError::Internal(anyhow::anyhow!("guest error: {e}")))?;

        let value = serde_json::from_str(&result_str)
            .context("deserialize guest result")
            .map_err(QuotaExecError::Internal)?;
        Ok(value)
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
        self.execute_quota(ctx, payload).await.map_err(|e| match e {
            QuotaExecError::RateLimited { retry_after_ms } => {
                anyhow::anyhow!("rate limited, retry after {retry_after_ms}ms")
            }
            QuotaExecError::ConcurrencyTimeout => anyhow::anyhow!("concurrency timeout"),
            QuotaExecError::Internal(e) => e,
        })
    }

    async fn on_shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
