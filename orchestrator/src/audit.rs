//! Batched audit-log writer.
//!
//! Per-execute INSERT against a WAL-mode SQLite pool serializes on the
//! single-writer lock and fsync per commit. Measured ceiling with the
//! synchronous path was ~950 req/s for `/execute` while `/healthz`
//! reached 81k req/s and `/modules` (read-only) reached 66k req/s.
//!
//! This writer batches AuditEntry rows and flushes them as one
//! multi-row INSERT, trading a bounded write-latency window
//! (FLUSH_INTERVAL_MS default 50 ms, or BATCH_MAX rows) for ~10-50x
//! execute throughput. Channel capacity is bounded so under extreme
//! load the producer awaits and backpressure propagates naturally
//! instead of dropping entries.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

const BATCH_MAX: usize = 100;
const FLUSH_INTERVAL_MS: u64 = 50;
const CHANNEL_CAPACITY: usize = 10_000;

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub tenant: String,
    pub module_id: String,
    pub status: &'static str,
    pub duration_ms: u64,
    pub error_message: Option<String>,
}

pub struct AuditWriter {
    tx: mpsc::Sender<AuditEntry>,
    /// Mutex so `drain(&self)` can `.take()` the handle from behind
    /// an `Arc<AuditWriter>` (AppContext holds `Arc<AuditWriter>`,
    /// callers cannot move out of it).
    handle: Mutex<Option<JoinHandle<()>>>,
    /// Explicit shutdown signal so `drain()` can wake the writer
    /// without relying on channel-closed (which only fires when all
    /// senders, including the one held by this struct, are dropped).
    shutdown: Arc<Notify>,
}

impl AuditWriter {
    pub fn spawn(db: SqlitePool) -> Self {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let shutdown = Arc::new(Notify::new());
        let handle = tokio::spawn(writer_loop(db, rx, shutdown.clone()));
        Self {
            tx,
            handle: Mutex::new(Some(handle)),
            shutdown,
        }
    }

    pub fn sender(&self) -> mpsc::Sender<AuditEntry> {
        self.tx.clone()
    }

    /// Drain remaining entries and stop the writer task. Called from
    /// `AppContext::shutdown(&self)`. `&self` because `AppContext`
    /// holds `Arc<AuditWriter>`; `drain()` uses a Notify signal plus
    /// a Mutex-guarded handle so the writer wakes, drains the
    /// remaining queue, flushes, and exits. Bounded by a 3-second
    /// timeout: a stuck DB call must not block process exit.
    ///
    /// Note: entries pushed between the last `sender()` drop and this
    /// call are NOT lost — the writer drains `try_recv()` after the
    /// notify before final flush (see writer_loop).
    pub async fn drain(&self) {
        self.shutdown.notify_waiters();
        let mut guard = self.handle.lock().await;
        if let Some(h) = guard.take() {
            let _ = tokio::time::timeout(Duration::from_secs(3), h).await;
        }
    }
}

async fn writer_loop(db: SqlitePool, mut rx: mpsc::Receiver<AuditEntry>, shutdown: Arc<Notify>) {
    let mut batch: Vec<AuditEntry> = Vec::with_capacity(BATCH_MAX);
    let mut ticker = tokio::time::interval(Duration::from_millis(FLUSH_INTERVAL_MS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            maybe = rx.recv() => match maybe {
                Some(e) => {
                    batch.push(e);
                    if batch.len() >= BATCH_MAX {
                        flush(&db, &mut batch).await;
                    }
                }
                None => break,
            },
            _ = ticker.tick() => {
                if !batch.is_empty() {
                    flush(&db, &mut batch).await;
                }
            }
            _ = shutdown.notified() => break,
        }
    }

    // Drain any remaining queued entries after shutdown notify.
    // Without this, entries pushed between the last sender drop and
    // the shutdown signal would be dropped.
    while let Ok(e) = rx.try_recv() {
        batch.push(e);
        if batch.len() >= BATCH_MAX {
            flush(&db, &mut batch).await;
        }
    }
    if !batch.is_empty() {
        flush(&db, &mut batch).await;
    }
    debug!("audit writer stopped");
}

async fn flush(db: &SqlitePool, batch: &mut Vec<AuditEntry>) {
    let n = batch.len();
    let mut sql = String::from(
        "INSERT INTO execution_logs (tenant, module_id, status, duration_ms, error_message) VALUES ",
    );
    for i in 0..n {
        if i > 0 {
            sql.push(',');
        }
        sql.push_str("(?,?,?,?,?)");
    }

    let mut q = sqlx::query(&sql);
    for e in batch.iter() {
        q = q
            .bind(&e.tenant)
            .bind(&e.module_id)
            .bind(e.status)
            .bind(e.duration_ms as i64)
            .bind(&e.error_message);
    }

    if let Err(err) = q.execute(db).await {
        warn!(err = %err, rows = n, "audit batch insert failed");
    }
    batch.clear();
}
