//! Rhai worker: satu tokio task yang menerima event dan mengeksekusi script
//! Rhai lewat spawn_blocking. AST dikompilasi sekali; Engine dibuat fresh
//! per eksekusi agar closure per-run (emit_event sink) tidak bocor antar-event.

use std::sync::{Arc, Mutex};

use rhai::{Engine, Scope, AST};
use tokio::sync::{broadcast, oneshot};
use tracing::{debug, error, info, warn};

use crate::context::AppContext;
use crate::events::{EventBus, OrchestratorEvent};

pub struct RhaiWorker {
    ast: Arc<AST>,
    event_rx: broadcast::Receiver<OrchestratorEvent>,
    event_bus: EventBus,
    log_allowed: bool,
    /// CapabilitySet snapshot untuk gating emit_event di closure.
    /// Di-clone ke closure per-run sehingga tidak bocor antar-event.
    capabilities: Arc<crate::capabilities::CapabilitySet>,
    /// Retained agar Arc<AppContext> hidup selama worker berjalan.
    /// M5+ dapat memakai ini untuk capability DB/HTTP sync dari script.
    _ctx: Arc<AppContext>,
}

impl RhaiWorker {
    pub fn new(
        script: &str,
        event_rx: broadcast::Receiver<OrchestratorEvent>,
        event_bus: EventBus,
        ctx: Arc<AppContext>,
    ) -> anyhow::Result<Self> {
        let engine = Engine::new();
        let ast = engine
            .compile(script)
            .map_err(|e| anyhow::anyhow!("rhai compile failed: {e}"))?;

        let log_allowed = ctx.capabilities.check_log().is_ok();
        let capabilities = ctx.capabilities.clone();
        debug!(log_allowed, "rhai worker initialized");

        Ok(Self {
            ast: Arc::new(ast),
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

            // Prevent feedback loop: skip events emitted by rhai itself.
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

        // CRITICAL: rhai sinkron dan CPU-bound. Wajib spawn_blocking.
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut engine = Engine::new();

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

        // Drain sink di luar spawn_blocking. Jika script error di tengah,
        // event yang sudah di-emit tetap diteruskan (partial emit).
        let emitted: Vec<OrchestratorEvent> = match sink.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => Vec::new(),
        };
        for ev in emitted {
            self.event_bus.publish(ev);
        }

        // Compute outcome SEBELUM match agar `result` tidak partially moved.
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
