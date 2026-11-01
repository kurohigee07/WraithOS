//! Error library-level dengan thiserror.

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
}

pub type Result<T> = std::result::Result<T, OrchestratorError>;
