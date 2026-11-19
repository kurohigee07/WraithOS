//! EventBus berbasis tokio::sync::broadcast.
//! publish() is intentionally synchronous so it is safe from spawn_blocking
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
        // Initial receiver is dropped; send() returns Err(0 subscribers),
        // not a panic, so it is safe before a subscriber attaches.
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx: Arc::new(tx) }
    }

    /// Return the number of subscribers that received. 0 if none.
    pub fn publish(&self, event: OrchestratorEvent) -> usize {
        self.tx.send(event).unwrap_or_default()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.tx.subscribe()
    }
}
