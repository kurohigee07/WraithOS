use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser;
use tracing::{info, warn};

use orchestrator::telemetry;
use orchestrator::tenant::{migrate_flat_to_default, TenantRegistry};
use orchestrator::wasm::{build_engine, EpochTicker, EPOCH_TICK};
use orchestrator::{
    AppContext, CapabilitySet, Cli, Config, EventBus, LogFormat, ModuleRegistry, OrchestratorEvent,
    RhaiWorker,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // ── Config load + CLI overrides ──────────────────────────────────────
    let mut config = if cli.config.exists() {
        Config::load(&cli.config)?
    } else {
        eprintln!(
            "[bootstrap] config {} not found; using defaults",
            cli.config.display()
        );
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
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
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
    let prom_handle = telemetry::install().context("install metrics recorder")?;

    let started_at = std::time::Instant::now();
    let engine = build_engine()?;
    let _ticker = EpochTicker::spawn(engine.clone(), EPOCH_TICK);
    info!("wasmtime engine + epoch ticker up");

    // ── AppContext + capability ─────────────────────────────────────────
    let caps = CapabilitySet::from_capabilities(&config.capabilities);
    let bus = EventBus::new(1024);
    let ctx = Arc::new(
        AppContext::build(config.to_app_context(), caps, bus.clone())
            .await
            .context("failed to build AppContext")?,
    );

    // ── Metrics server (opsional) ──────────────────────────────────────

    // ── Wasm modules ────────────────────────────────────────────────────
    // ── Migration + tenant registry ─────────────────────────────────────
    let migrated = migrate_flat_to_default(&config.modules_dir)
        .context("migrate flat modules to default tenant")?;
    if migrated > 0 {
        info!(migrated, "flat modules migrated to default tenant");
    }
    // Validate auth config: require_auth=true needs default tenant token
    if config.require_auth
        && !config
            .tenants
            .get(&config.default_tenant)
            .map(|t| t.token_hash.is_some() || t.token_plain.is_some())
            .unwrap_or(false)
    {
        anyhow::bail!(
            "config.require_auth=true but default tenant '{}' has no token_hash/token_plain",
            config.default_tenant
        );
    }

    let tenant_acquire_timeout = config.to_app_context().wasm_acquire_timeout;
    let cargo_home = config.cargo_home.clone().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/opc".to_string());
        std::path::PathBuf::from(format!("{home}/.cargo"))
    });
    if !cargo_home.join("bin").join("cargo").exists() {
        warn!(
            cargo_home = %cargo_home.display(),
            "cargo binary not found — /modules/build will fail"
        );
    } else {
        info!(cargo_home = %cargo_home.display(), "build pipeline ready");
    }

    // Startup: verify build sandbox (bwrap) if build enabled
    if config.enable_build {
        match std::process::Command::new("bwrap")
            .arg("--version")
            .output()
        {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
                // --version alone is insufficient: user namespaces may be
                // disabled even though the binary is installed. Probe an
                // actual unshare so we fail at startup, not at first build.
                let probe = std::process::Command::new("bwrap")
                    .args([
                        "--unshare-all",
                        "--die-with-parent",
                        "--ro-bind",
                        "/",
                        "/",
                        "/bin/true",
                    ])
                    .output();
                match probe {
                    Ok(p) if p.status.success() => {
                        info!(bwrap = %version, "build sandbox (bwrap) available");
                    }
                    Ok(p) => {
                        let stderr = String::from_utf8_lossy(&p.stderr).trim().to_string();
                        anyhow::bail!(
                            "config.enable_build=true but bwrap functional probe failed: {stderr}. Check /proc/sys/user/max_user_namespaces and SELinux policy. Or set enable_build=false."
                        );
                    }
                    Err(e) => {
                        anyhow::bail!("config.enable_build=true but bwrap spawn failed: {e}");
                    }
                }
            }
            _ => {
                anyhow::bail!(
                    "config.enable_build=true but bwrap missing. \
                     Install: sudo dnf install -y bubblewrap (EPEL 9). \
                     Or set enable_build=false."
                );
            }
        }
    }

    let tenants = TenantRegistry::from_config(
        config.tenants.clone(),
        config.default_tenant.clone(),
        tenant_acquire_timeout,
    )
    .context("tenant registry")?;
    let tenants = Arc::new(tenants);
    info!(count = tenants.len() as u32, "tenant registry ready");

    let default_quota = shared_types::ModuleQuota::default();
    let registry = ModuleRegistry::new(
        engine.clone(),
        config.trusted_signer_keys.clone(),
        default_quota,
        config.wasm_memory_limit_mb,
        config.to_app_context().wasm_acquire_timeout,
    );
    let loaded = registry
        .load_wasm_dir(&config.modules_dir, ctx.capabilities.clone())
        .context("load wasm modules")?;
    info!(wasm_loaded = loaded, dir = %config.modules_dir.display(), "module registry ready");

    // Startup probe removed (v0.21.2). Was executing every module 2x per boot
    // (probe + memory-bomb test), polluting audit log and adding latency for
    // modules with HTTP side effects. Verify runtime via POST /modules/{id}/execute.

    let registry = Arc::new(registry);
    let wizard = orchestrator::wizard::WizardState::new(config.data_dir.clone());
    let api_state = telemetry::ApiState {
        prom: prom_handle,
        ctx: ctx.clone(),
        registry: registry.clone(),
        started_at,
        modules_dir: config.modules_dir.clone(),
        tenants: tenants.clone(),
        build_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        cargo_home: cargo_home.clone(),
        require_auth: config.require_auth,
        enable_build: config.enable_build,
        enable_build_network: config.enable_build_network,
        data_dir: config.data_dir.clone(),
        wizard: wizard.clone(),
    };
    let metrics_server: Option<(
        tokio::task::JoinHandle<()>,
        tokio::sync::oneshot::Sender<()>,
    )> = match config.metrics_bind.as_deref() {
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
    let rhai_limits = config.script.to_limits();
    let worker = RhaiWorker::new(
        &script,
        rhai_limits,
        bus.subscribe(),
        bus.clone(),
        ctx.clone(),
    )
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
                Ok(ev) if ev.kind == "sample.emitted" => {
                    info!(payload = %ev.payload, source = %ev.source_module, "wasm event observed");
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

    match worker_handle.await {
        Ok(Ok(())) => info!("rhai worker exited cleanly"),
        Ok(Err(e)) => warn!(err = %e, "rhai worker returned error"),
        Err(j) => warn!(err = %j, "rhai worker join failed"),
    }

    // derived_handle loops on event_bus.recv(). The EventBus is held
    // alive by ApiState (Arc<AppContext>.event_bus), which the metrics
    // server owns until we shut it down below. derived_rx would never
    // observe RecvError::Closed while metrics is still running, so an
    // .await here deadlocks until systemd's TimeoutStopSec sends
    // SIGKILL. Abort explicitly; this task is a demo log subscriber
    // and has no cleanup to run.
    derived_handle.abort();
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
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .init();
        }
    }
    Ok(())
}

async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm =
            signal(SignalKind::terminate()).map_err(|e| anyhow::anyhow!("SIGTERM handler: {e}"))?;
        let mut sigint =
            signal(SignalKind::interrupt()).map_err(|e| anyhow::anyhow!("SIGINT handler: {e}"))?;
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
