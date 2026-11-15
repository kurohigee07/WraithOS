//! Rhai event worker. Compiles the script once; builds a fresh Engine per
//! event so per-run closures (emit sink) don't leak between events. All
//! engines apply the limits in [`RhaiLimits`].

use std::sync::{Arc, Mutex};

use rhai::{Engine, Scope, AST};
use tokio::sync::{broadcast, oneshot};
use tracing::{debug, error, info, warn};

use crate::context::AppContext;
use crate::events::{EventBus, OrchestratorEvent};

// H2 defaults. Rationale and worst-case timings in docs/security.md.
pub const DEFAULT_MAX_OPERATIONS: u64 = 10_000_000;
pub const DEFAULT_MAX_STRING_SIZE: usize = 1_048_576;
pub const DEFAULT_MAX_ARRAY_SIZE: usize = 100_000;
pub const DEFAULT_MAX_MAP_SIZE: usize = 10_000;
pub const DEFAULT_MAX_EXPR_DEPTH: usize = 64;
pub const DEFAULT_MAX_FUNCTION_EXPR_DEPTH: usize = 32;

/// Per-execution Rhai budget. Defaults are production-safe; override via
/// `[script]` in orchestrator.toml. A zero value disables that limit
/// (Rhai treats 0 as unlimited).
#[derive(Debug, Clone)]
pub struct RhaiLimits {
    pub max_operations: u64,
    pub max_string_size: usize,
    pub max_array_size: usize,
    pub max_map_size: usize,
    pub max_expr_depth: usize,
    pub max_function_expr_depth: usize,
}

impl Default for RhaiLimits {
    fn default() -> Self {
        Self {
            max_operations: DEFAULT_MAX_OPERATIONS,
            max_string_size: DEFAULT_MAX_STRING_SIZE,
            max_array_size: DEFAULT_MAX_ARRAY_SIZE,
            max_map_size: DEFAULT_MAX_MAP_SIZE,
            max_expr_depth: DEFAULT_MAX_EXPR_DEPTH,
            max_function_expr_depth: DEFAULT_MAX_FUNCTION_EXPR_DEPTH,
        }
    }
}

fn build_engine(limits: &RhaiLimits) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(limits.max_operations);
    engine.set_max_string_size(limits.max_string_size);
    engine.set_max_array_size(limits.max_array_size);
    engine.set_max_map_size(limits.max_map_size);
    engine.set_max_expr_depths(limits.max_expr_depth, limits.max_function_expr_depth);
    engine
}

pub struct RhaiWorker {
    ast: Arc<AST>,
    limits: Arc<RhaiLimits>,
    event_rx: broadcast::Receiver<OrchestratorEvent>,
    event_bus: EventBus,
    log_allowed: bool,
    capabilities: Arc<crate::capabilities::CapabilitySet>,
    _ctx: Arc<AppContext>,
}

impl RhaiWorker {
    pub fn new(
        script: &str,
        limits: RhaiLimits,
        event_rx: broadcast::Receiver<OrchestratorEvent>,
        event_bus: EventBus,
        ctx: Arc<AppContext>,
    ) -> anyhow::Result<Self> {
        let limits = Arc::new(limits);
        let engine = build_engine(&limits);
        let ast = engine
            .compile(script)
            .map_err(|e| anyhow::anyhow!("rhai compile failed: {e}"))?;

        let log_allowed = ctx.capabilities.check_log().is_ok();
        let capabilities = ctx.capabilities.clone();
        debug!(
            max_operations = limits.max_operations,
            log_allowed, "rhai worker initialized"
        );

        Ok(Self {
            ast: Arc::new(ast),
            limits,
            event_rx,
            event_bus,
            log_allowed,
            capabilities,
            _ctx: ctx,
        })
    }

    pub async fn run(mut self, mut shutdown: oneshot::Receiver<()>) -> anyhow::Result<()> {
        loop {
            let event = tokio::select! {
                biased;
                _ = &mut shutdown => {
                    debug!("rhai worker: shutdown signal received");
                    break;
                }
                ev = self.event_rx.recv() => match ev {
                    Ok(ev) => ev,
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("rhai worker: event bus closed");
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(missed = n, "rhai worker lagged");
                        continue;
                    }
                }
            };

            if event.source_module == "rhai" {
                continue;
            }
            self.dispatch(event).await;
        }
        info!("rhai worker exited");
        Ok(())
    }

    #[tracing::instrument(
        skip_all,
        fields(
            event_kind = %event.kind,
            source = %event.source_module,
            outcome = tracing::field::Empty,
        ),
    )]
    async fn dispatch(&self, event: OrchestratorEvent) {
        let start = std::time::Instant::now();
        let source_label = event.source_module.clone();
        let ast = self.ast.clone();
        let limits = self.limits.clone();
        let source_module = event.source_module.clone();
        let payload_json = match serde_json::to_string(&event.payload) {
            Ok(s) => s,
            Err(e) => {
                error!(err = %e, "rhai worker: serialize payload failed");
                return;
            }
        };
        let kind = event.kind.clone();

        let log_allowed = self.log_allowed;
        let capabilities = self.capabilities.clone();
        let sink: Arc<Mutex<Vec<OrchestratorEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_for_engine = sink.clone();

        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut engine = build_engine(&limits);

            engine.register_fn("log_info", move |msg: &str| {
                if log_allowed {
                    info!(target: "rhai", source = %source_module, "{msg}");
                }
            });

            let emit_sink = sink_for_engine;
            let emit_caps = capabilities;
            engine.register_fn("emit_event", move |k: &str, p: &str| {
                if emit_caps.check_emit_event(k).is_err() {
                    error!(kind = k, "rhai emit_event: capability denied");
                    return;
                }
                match serde_json::from_str::<serde_json::Value>(p) {
                    Ok(v) => {
                        if let Ok(mut g) = emit_sink.lock() {
                            g.push(OrchestratorEvent {
                                kind: k.to_string(),
                                payload: v,
                                source_module: "rhai".to_string(),
                            });
                        }
                    }
                    Err(_) => {
                        error!(kind = k, "rhai emit_event: payload is not json");
                    }
                }
            });

            let mut scope = Scope::new();
            scope.push("event_kind", kind);
            scope.push("event_payload", payload_json);

            engine
                .run_ast_with_scope(&mut scope, ast.as_ref())
                .map_err(|e| anyhow::anyhow!("rhai runtime: {e}"))?;
            Ok(())
        })
        .await;

        let emitted: Vec<OrchestratorEvent> = match sink.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => Vec::new(),
        };
        for ev in emitted {
            self.event_bus.publish(ev);
        }

        let outcome = match &result {
            Ok(Ok(())) => "ok",
            _ => "error",
        };

        match result {
            Ok(Ok(())) => debug!(event_kind = %event.kind, "rhai script ok"),
            Ok(Err(e)) => error!(event_kind = %event.kind, err = %e, "rhai script failed"),
            Err(join_err) => {
                if join_err.is_panic() {
                    error!("rhai worker panicked during script execution");
                } else {
                    error!(err = %join_err, "rhai worker join error");
                }
            }
        }

        metrics::counter!(
            "rhai_events_processed_total",
            "source" => source_label,
        )
        .increment(1);
        metrics::histogram!("rhai_script_duration_seconds").record(start.elapsed().as_secs_f64());

        tracing::Span::current().record("outcome", outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_script_ok() {
        let engine = build_engine(&RhaiLimits::default());
        let ast = engine.compile("let x = 1 + 1; x").unwrap();
        let mut scope = Scope::new();
        let out: i64 = engine.eval_ast_with_scope(&mut scope, &ast).unwrap();
        assert_eq!(out, 2);
    }

    #[test]
    fn infinite_loop_traps() {
        let limits = RhaiLimits {
            max_operations: 1_000,
            ..Default::default()
        };
        let engine = build_engine(&limits);
        let ast = engine.compile("let i = 0; while true { i += 1; }").unwrap();
        let mut scope = Scope::new();
        let err = engine
            .eval_ast_with_scope::<()>(&mut scope, &ast)
            .expect_err("must trap");
        assert!(format!("{err:?}").contains("ErrorTooManyOperations"));
    }

    #[test]
    fn string_bomb_traps() {
        let limits = RhaiLimits {
            max_operations: u64::MAX,
            max_string_size: 1_000,
            ..Default::default()
        };
        let engine = build_engine(&limits);
        let ast = engine
            .compile(r#"let s = ""; loop { s += "xxxxxxxxxx"; }"#)
            .unwrap();
        let mut scope = Scope::new();
        let err = engine
            .eval_ast_with_scope::<()>(&mut scope, &ast)
            .expect_err("must trap");
        assert!(format!("{err:?}").contains("ErrorDataTooLarge"));
    }

    #[test]
    fn array_bomb_traps() {
        let limits = RhaiLimits {
            max_operations: u64::MAX,
            max_array_size: 100,
            ..Default::default()
        };
        let engine = build_engine(&limits);
        let ast = engine.compile("let a = []; loop { a.push(1); }").unwrap();
        let mut scope = Scope::new();
        let err = engine
            .eval_ast_with_scope::<()>(&mut scope, &ast)
            .expect_err("must trap");
        assert!(format!("{err:?}").contains("ErrorDataTooLarge"));
    }
}
