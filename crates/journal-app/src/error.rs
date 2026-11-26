use journal_domain::DomainError;
use journal_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Domain(#[from] DomainError),

    #[error("{0}")]
    Storage(#[from] StorageError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("XLSX export error: {0}")]
    Xlsx(String),

    #[error("Archive error: {0}")]
    Zip(String),

    #[error("Import error: {0}")]
    Import(String),

    #[error("Backup/restore error: {0}")]
    Backup(String),
}

pub type AppResult<T> = Result<T, AppError>;
