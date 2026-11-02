//! AppContext: state bersama, di-wrap `Arc<AppContext>` dan dishare ke plugin.

use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use dashmap::DashMap;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{debug, info};

use crate::capabilities::CapabilitySet;
use crate::events::EventBus;
use crate::error::{OrchestratorError, Result};

#[derive(Debug, Clone)]
pub struct AppContextConfig {
    pub db_path: PathBuf,
    pub http_timeout: Duration,
    pub max_db_connections: u32,
    pub max_sockets: usize,
    pub max_concurrent_wasm: usize,
    pub wasm_acquire_timeout: Duration,
    pub wasm_memory_limit_mb: u64,
}

impl Default for AppContextConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("orchestrator.db"),
            http_timeout: Duration::from_secs(30),
            max_db_connections: 8,
            max_sockets: 256,
            max_concurrent_wasm: 2,
            wasm_acquire_timeout: Duration::from_secs(10),
            wasm_memory_limit_mb: 256,
        }
    }
}

// ─── WasmPool ──────────────────────────────────────────────────────────
/// Batasi berapa banyak eksekusi Wasm boleh paralel. Semaphore ini
/// memastikan guest CPU-bound (yang menahan tokio worker hingga epoch
/// trap 60s) tidak menghabiskan worker pool.
pub struct WasmPool {
    sem: Arc<Semaphore>,
    inflight: Arc<AtomicUsize>,
    total: usize,
    acquire_timeout: Duration,
}

impl WasmPool {
    pub fn new(max: usize, acquire_timeout: Duration) -> Self {
        assert_nonzero(max, "max_concurrent_wasm");
        Self {
            sem: Arc::new(Semaphore::new(max)),
            inflight: Arc::new(AtomicUsize::new(0)),
            total: max,
            acquire_timeout,
        }
    }

    pub async fn acquire(&self) -> Result<WasmPermit> {
        let permit = tokio::time::timeout(
            self.acquire_timeout,
            self.sem.clone().acquire_owned(),
        )
        .await
        .map_err(|_| {
            OrchestratorError::CapabilityDenied(format!(
                "wasm pool acquire timeout after {:?} ({}/{} in flight)",
                self.acquire_timeout,
                self.total - self.sem.available_permits(),
                self.total,
            ))
        })?
        .map_err(|e| {
            OrchestratorError::CapabilityDenied(format!("wasm pool closed: {e}"))
        })?;

        let n = self.inflight.fetch_add(1, Ordering::Relaxed) + 1;
        metrics::gauge!("wasm_pool_in_flight").set(n as f64);
        Ok(WasmPermit {
            _permit: permit,
            inflight: self.inflight.clone(),
        })
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn available(&self) -> usize {
        self.sem.available_permits()
    }
}

pub struct WasmPermit {
    _permit: OwnedSemaphorePermit,
    inflight: Arc<AtomicUsize>,
}

impl Drop for WasmPermit {
    fn drop(&mut self) {
        let n = self.inflight.fetch_sub(1, Ordering::Relaxed) - 1;
        metrics::gauge!("wasm_pool_in_flight").set(n as f64);
    }
}

// ─── SocketRegistry ────────────────────────────────────────────────────
pub struct SocketRegistry {
    inner: DashMap<u32, (TcpStream, OwnedSemaphorePermit)>,
    next: AtomicU32,
    permits: Arc<Semaphore>,
    max: usize,
}

impl SocketRegistry {
    pub fn new(max: usize) -> Self {
        assert_nonzero(max, "max_sockets");
        Self {
            inner: DashMap::new(),
            next: AtomicU32::new(1),
            permits: Arc::new(Semaphore::new(max)),
            max,
        }
    }

    pub fn insert(&self, stream: TcpStream) -> Result<u32> {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                OrchestratorError::CapabilityDenied(format!(
                    "socket registry at capacity ({}/{})",
                    self.inner.len(),
                    self.max
                ))
            })?;
        let handle = self.next.fetch_add(1, Ordering::Relaxed);
        self.inner.insert(handle, (stream, permit));
        metrics::gauge!("socket_registry_active").set(self.inner.len() as f64);
        Ok(handle)
    }

    pub fn with<F, T>(&self, handle: u32, f: F) -> Result<T>
    where
        F: FnOnce(&TcpStream) -> std::io::Result<T>,
    {
        let entry = self
            .inner
            .get(&handle)
            .ok_or(OrchestratorError::SocketNotFound(handle))?;
        let (stream, _permit) = entry.value();
        f(stream).map_err(OrchestratorError::Io)
    }

    pub fn remove(&self, handle: u32) -> Option<TcpStream> {
        let removed = self.inner.remove(&handle).map(|(_, (s, _p))| s);
        if removed.is_some() {
            metrics::gauge!("socket_registry_active").set(self.inner.len() as f64);
        }
        removed
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn max(&self) -> usize {
        self.max
    }

    pub fn available(&self) -> usize {
        self.permits.available_permits()
    }
}

fn assert_nonzero(max: usize, name: &str) {
    if max == 0 {
        panic!("{} must be >= 1", name);
    }
}

// ─── AppContext ────────────────────────────────────────────────────────
pub struct AppContext {
    pub http: reqwest::Client,
    pub db: SqlitePool,
    pub sockets: SocketRegistry,
    pub wasm_pool: WasmPool,
    pub capabilities: Arc<CapabilitySet>,
    pub event_bus: EventBus,
    pub config: AppContextConfig,
}

impl AppContext {
    pub async fn build(
        config: AppContextConfig,
        capabilities: CapabilitySet,
        event_bus: EventBus,
    ) -> anyhow::Result<Self> {
        // Validate config nilai kritis (user-provided via TOML).
        if config.max_sockets == 0 {
            return Err(anyhow::anyhow!("config.max_sockets must be >= 1"));
        }
        if config.max_concurrent_wasm == 0 {
            return Err(anyhow::anyhow!("config.max_concurrent_wasm must be >= 1"));
        }
        if config.max_db_connections == 0 {
            return Err(anyhow::anyhow!("config.max_db_connections must be >= 1"));
        }

        install_crypto_provider()?;

        let http = build_http_client(config.http_timeout)
            .context("failed to construct reqwest client")?;

        let db = build_sqlite_pool(&config.db_path, config.max_db_connections)
            .await
            .context("failed to construct sqlite pool")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS execution_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tenant TEXT NOT NULL,
                module_id TEXT NOT NULL,
                status TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                error_message TEXT,
                executed_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&db)
        .await
        .context("failed to initialize execution_logs table")?;

        info!(
            db = %config.db_path.display(),
            max_conn = config.max_db_connections,
            http_timeout_secs = config.http_timeout.as_secs(),
            max_sockets = config.max_sockets,
            max_concurrent_wasm = config.max_concurrent_wasm,
            wasm_acquire_timeout_secs = config.wasm_acquire_timeout.as_secs(),
            wasm_memory_limit_mb = config.wasm_memory_limit_mb,
            "AppContext initialized"
        );

        Ok(Self {
            http,
            db,
            sockets: SocketRegistry::new(config.max_sockets),
            wasm_pool: WasmPool::new(config.max_concurrent_wasm, config.wasm_acquire_timeout),
            capabilities: Arc::new(capabilities),
            event_bus,
            config,
        })
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        debug!("AppContext shutting down");
        self.db.close().await;
        Ok(())
    }
}

fn install_crypto_provider() -> anyhow::Result<()> {
    match rustls::crypto::ring::default_provider().install_default() {
        Ok(()) => {
            debug!("rustls crypto provider installed (ring)");
            Ok(())
        }
        Err(_) => {
            debug!("rustls crypto provider already installed");
            Ok(())
        }
    }
}

fn build_http_client(timeout: Duration) -> anyhow::Result<reqwest::Client> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    reqwest::Client::builder()
        .use_preconfigured_tls(tls)
        .timeout(timeout)
        .user_agent(concat!("orchestrator/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("reqwest ClientBuilder::build failed")
}

async fn build_sqlite_pool(
    path: &std::path::Path,
    max_conn: u32,
) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    SqlitePoolOptions::new()
        .max_connections(max_conn)
        .connect_with(opts)
        .await
        .context("SqlitePoolOptions::connect_with failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream as StdTcpStream};

    fn loopback_pair() -> (TcpListener, StdTcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local_addr");
        let client = StdTcpStream::connect(addr).expect("connect loopback");
        (listener, client)
    }

    #[test]
    fn socket_cap_fail_closed() {
        let (_l1, c1) = loopback_pair();
        let (_l2, c2) = loopback_pair();
        let (_l3, c3) = loopback_pair();

        let reg = SocketRegistry::new(2);
        assert_eq!(reg.available(), 2);

        let h1 = reg.insert(c1).expect("insert 1");
        let h2 = reg.insert(c2).expect("insert 2");
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.available(), 0);

        match reg.insert(c3) {
            Err(OrchestratorError::CapabilityDenied(_)) => {}
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }

        assert!(reg.remove(h1).is_some());
        assert_eq!(reg.available(), 1);
        assert!(reg.remove(h2).is_some());
        assert_eq!(reg.available(), 2);
    }

    #[tokio::test]
    async fn wasm_pool_caps_concurrency() {
        let pool = WasmPool::new(2, Duration::from_millis(100));
        assert_eq!(pool.total(), 2);
        assert_eq!(pool.available(), 2);

        let p1 = pool.acquire().await.expect("acquire 1");
        let p2 = pool.acquire().await.expect("acquire 2");
        assert_eq!(pool.available(), 0);

        // Kapasitas penuh: acquire ke-3 harus timeout.
        let err = pool.acquire().await.err().expect("must saturate");
        match err {
            OrchestratorError::CapabilityDenied(msg) => {
                assert!(msg.contains("timeout"), "unexpected message: {msg}");
            }
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }

        // Release satu, slot kembali tersedia.
        drop(p1);
        assert_eq!(pool.available(), 1);
        let p3 = pool.acquire().await.expect("acquire after release");
        assert_eq!(pool.available(), 0);

        drop(p2);
        drop(p3);
        assert_eq!(pool.available(), 2);
    }
}
