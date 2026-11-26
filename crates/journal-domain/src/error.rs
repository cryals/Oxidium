use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("validation error on '{field}': {message}")]
    Validation { field: String, message: String },

    #[error("entity not found: {entity} ({id})")]
    NotFound { entity: String, id: String },

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("ambiguous match for '{query}': found {count} candidates")]
    Ambiguous { query: String, count: usize },

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("profile isolation violation: {0}")]
    ProfileIsolation(String),

    #[error("invalid state: {0}")]
    InvalidState(String),
}

impl DomainError {
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            message: message.into(),
        }
    }
}
