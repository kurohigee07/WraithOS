use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser;
use tracing::{info, warn};

use orchestrator::telemetry;
use orchestrator::wasm::{build_engine, EpochTicker, EPOCH_TICK};
use orchestrator::{
    AppContext, CapabilitySet, Cli, Config, EventBus, LogFormat, ModuleRegistry,
    OrchestratorEvent, OrchestratorModule, RhaiWorker,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // ── Config load + CLI overrides ──────────────────────────────────────
    let mut config = if cli.config.exists() {
        Config::load(&cli.config)?
    } else {
        eprintln!("[bootstrap] config {} not found; using defaults", cli.config.display());
        Config::default()
    };
    if let Some(d) = cli.modules_dir {
        config.modules_dir = d;
    }
    if let Some(l) = cli.log_level {
        config.log_level = l;
    }
    if let Some(f) = cli.log_format {
        config.log_format = f;
    }

    init_tracing(&config)?;

    if cli.dry_run {
        info!(config = ?config, "dry-run: config valid");
        return Ok(());
    }

    // Sanity: bila max_concurrent_wasm >= CPU count, tokio worker pool bisa
    // starve saat guest CPU-bound. Ini warning, bukan error — user boleh
    // override jika tahu apa yang dilakukan.
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    if config.max_concurrent_wasm >= cpus {
        warn!(
            max_concurrent_wasm = config.max_concurrent_wasm,
            cpus,
            "concurrent wasm equals or exceeds cpu count; tokio workers may starve              during guest CPU-bound phases"
        );
    }

    // ── Wasmtime engine (shared, epoch-enabled) + ticker ────────────────
    // Metrics recorder MUST be installed before any metrics::* macro.
    // Probe loop di bawah akan me-emit wasm_executions_total.
    let prom_handle = telemetry::install()
        .context("install metrics recorder")?;

    let started_at = std::time::Instant::now();
    let engine = build_engine()?;
    let _ticker = EpochTicker::spawn(engine.clone(), EPOCH_TICK);
    info!("wasmtime engine + epoch ticker up");

    // ── AppContext + capability ─────────────────────────────────────────
    let caps = CapabilitySet::from_capabilities(&config.capabilities);
    let ctx = Arc::new(
        AppContext::build(config.to_app_context(), caps)
            .await
            .context("failed to build AppContext")?,
    );

    let bus = EventBus::new(1024);

    // ── Metrics server (opsional) ──────────────────────────────────────

    // ── Wasm modules ────────────────────────────────────────────────────
    let registry = ModuleRegistry::new(engine.clone(), config.trusted_signer_keys.clone());
    let loaded = registry
        .load_wasm_dir(&config.modules_dir)
        .context("load wasm modules")?;
    info!(wasm_loaded = loaded, dir = %config.modules_dir.display(), "module registry ready");

    for (id, module) in registry.all() {
        // Probe 1: normal path
        let payload = serde_json::json!({"probe": "m10b", "id": id});
        match module.execute(ctx.clone(), payload).await {
            Ok(result) => info!(id = %id, result = %result, "wasm execute ok"),
            Err(e) => warn!(id = %id, err = %e, "wasm execute failed"),
        }

        // Probe 2: memory bomb. Guest minta 512 MB; StoreLimits cap 256 MB.
        // Ekspektasi: trap -> Err, host tetap hidup.
        let payload = serde_json::json!({"alloc_mb": 512});
        match module.execute(ctx.clone(), payload).await {
            Ok(result) => warn!(
                id = %id, result = %result,
                "wasm alloc unexpectedly ok; wasm_memory_limit_mb too high?"
            ),
            Err(e) => info!(id = %id, err = %e, "wasm alloc trapped as expected"),
        }
    }


    let registry = Arc::new(registry);
    let api_state = telemetry::ApiState {
        prom: prom_handle,
        ctx: ctx.clone(),
        registry: registry.clone(),
        started_at,
        modules_dir: config.modules_dir.clone(),
    };
    let metrics_server: Option<(tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>)> =
        match config.metrics_bind.as_deref() {
            Some("") | None => {
                info!("metrics disabled (metrics_bind empty or none)");
                None
            }
            Some(bind) => {
                let addr: std::net::SocketAddr = bind
                    .parse()
                    .with_context(|| format!("invalid metrics_bind: {bind}"))?;
                let (m_tx, m_rx) = tokio::sync::oneshot::channel::<()>();
                let handle = telemetry::spawn(addr, api_state, m_rx)
                    .await
                    .context("spawn metrics server")?;
                info!(%addr, "metrics server up");
                Some((handle, m_tx))
            }
        };

    // ── Rhai worker ─────────────────────────────────────────────────────
    let script = config.script.load()?;
    let worker = RhaiWorker::new(&script, bus.subscribe(), bus.clone(), ctx.clone())
        .context("failed to build RhaiWorker")?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let worker_handle = tokio::spawn(worker.run(shutdown_rx));

    let mut derived_rx = bus.subscribe();
    let derived_handle = tokio::spawn(async move {
        loop {
            match derived_rx.recv().await {
                Ok(ev) if ev.kind == "rhai.processed" => {
                    info!(payload = %ev.payload, "derived event observed");
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(missed = n, "derived subscriber lagged");
                }
            }
        }
    });

    let delivered = bus.publish(OrchestratorEvent {
        kind: "scan.completed".into(),
        payload: serde_json::json!({ "target": "example.com" }),
        source_module: "smoke".into(),
    });
    info!(delivered, "smoke event published");

    wait_for_shutdown_signal().await?;

    let _ = shutdown_tx.send(());
    drop(bus);

    match worker_handle.await {
        Ok(Ok(())) => info!("rhai worker exited cleanly"),
        Ok(Err(e)) => warn!(err = %e, "rhai worker returned error"),
        Err(j) => warn!(err = %j, "rhai worker join failed"),
    }
    let _ = derived_handle.await;

    if let Some((m_handle, m_tx)) = metrics_server {
        let _ = m_tx.send(());
        let _ = m_handle.await;
        info!("metrics server stopped");
    }

    ctx.shutdown().await?;
    info!("shutdown complete");
    Ok(())
}

fn init_tracing(config: &Config) -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    match config.log_format {
        LogFormat::Pretty => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
        LogFormat::Json => {
            tracing_subscriber::fmt().json().with_env_filter(filter).init();
        }
    }
    Ok(())
}

async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate())
            .map_err(|e| anyhow::anyhow!("SIGTERM handler: {e}"))?;
        let mut sigint = signal(SignalKind::interrupt())
            .map_err(|e| anyhow::anyhow!("SIGINT handler: {e}"))?;
        tokio::select! {
            _ = sigint.recv() => info!("received SIGINT"),
            _ = sigterm.recv() => info!("received SIGTERM"),
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .map_err(|e| anyhow::anyhow!("ctrl_c: {e}"))?;
        info!("received Ctrl+C");
        Ok(())
    }
}
