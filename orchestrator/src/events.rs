//! EventBus berbasis tokio::sync::broadcast.
//! publish() sengaja sinkron sehingga aman dipanggil dari spawn_blocking
//! maupun async context.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorEvent {
    pub kind: String,
    pub payload: serde_json::Value,
    pub source_module: String,
}

#[derive(Clone)]
pub struct EventBus {
    tx: Arc<broadcast::Sender<OrchestratorEvent>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        // Receiver awal dibuang; send() akan return Err(0 subscribers),
        // bukan panic, sehingga aman sebelum subscriber terpasang.
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx: Arc::new(tx) }
    }

    /// Return jumlah subscriber yang menerima. 0 jika tidak ada.
    pub fn publish(&self, event: OrchestratorEvent) -> usize {
        self.tx.send(event).unwrap_or_default()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.tx.subscribe()
    }
}
