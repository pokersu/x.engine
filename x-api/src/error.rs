use axum::response::IntoResponse;
use axum::http::StatusCode;
use axum::Json;

use x_types::error::StorageError;

/// Unified API error type.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("internal: {0}")]
    Internal(String),

    #[error("unavailable: {0}")]
    Unavailable(String),

    #[error("payload too large: {0}")]
    PayloadTooLarge(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::InvalidArgument(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::AlreadyExists(msg) => (StatusCode::CONFLICT, msg.clone()),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            Self::Unavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            Self::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg.clone()),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

impl ApiError {
    pub fn from_storage(e: StorageError, entity: &str) -> Self {
        match e {
            StorageError::NotFound { entity: _, id } => Self::NotFound(format!("{entity} {id}")),
            StorageError::Conflict(msg) => Self::Conflict(msg),
            StorageError::TerminalTarget { entity: _, id } => Self::Conflict(format!("{entity} {id} is in a terminal state")),
            StorageError::PoolExhausted => Self::Unavailable("connection pool exhausted".into()),
            other => Self::Internal(other.to_string()),
        }
    }
}
