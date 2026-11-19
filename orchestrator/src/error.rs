//! Library-level errors via thiserror.

#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("capability denied: {0}")]
    CapabilityDenied(String),

    #[error("http transport error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("tls configuration error: {0}")]
    Tls(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid url: {0}")]
    InvalidUrl(String),

    #[error("socket handle not found: {0}")]
    SocketNotFound(u32),

    #[error("wasm pool acquire timeout after {wait:?} ({in_flight}/{total} in flight)")]
    WasmPoolTimeout {
        wait: std::time::Duration,
        in_flight: usize,
        total: usize,
    },

    #[error("wasm pool closed")]
    WasmPoolClosed,
}

impl OrchestratorError {
    pub fn is_wasm_pool_timeout(&self) -> bool {
        matches!(self, Self::WasmPoolTimeout { .. })
    }
}

pub type Result<T> = std::result::Result<T, OrchestratorError>;
