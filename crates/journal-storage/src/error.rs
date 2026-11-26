use journal_domain::DomainError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("database integrity check failed: {0}")]
    Integrity(String),

    #[error("data corruption or parse error: {0}")]
    DataParse(String),
}

pub type StorageResult<T> = Result<T, StorageError>;
