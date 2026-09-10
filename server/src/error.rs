//! Request-level errors and their HTTP mapping (`docs/v2/SYNC.md` §3).

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sparagne_core::{
    DomainError,
    sync::{ErrorBody, ErrorDetail},
};
use thiserror::Error;

/// Result of anything that can answer with an [`ErrorBody`].
pub type ApiResult<T> = Result<T, ApiError>;

/// One request-level failure: an HTTP status plus the stable code clients
/// match on.
#[derive(Clone, Debug, Error)]
#[error("{code}: {message}")]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    #[must_use]
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    #[must_use]
    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "authentication required",
        )
    }

    #[must_use]
    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "not allowed")
    }

    #[must_use]
    pub fn author_mismatch() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "author_mismatch",
            "the author of a command must be the authenticated user",
        )
    }

    #[must_use]
    pub fn registration_disabled() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "registration_disabled",
            "registration is disabled on this server",
        )
    }

    /// Everything the caller may not see is a blind 404.
    #[must_use]
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "not found")
    }

    #[must_use]
    pub fn already_exists(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "already_exists", message)
    }

    /// Logs the detail and answers with a generic message.
    #[must_use]
    pub fn internal(detail: impl AsRef<str>) -> Self {
        tracing::error!(detail = detail.as_ref(), "internal error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            "internal error",
        )
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        let status = match err {
            DomainError::NotFound(_) => StatusCode::NOT_FOUND,
            DomainError::AlreadyExists(_) => StatusCode::CONFLICT,
            DomainError::Storage(_) => return Self::internal(err.to_string()),
            _ => StatusCode::BAD_REQUEST,
        };
        Self::new(status, err.code(), err.to_string())
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(err: rusqlite::Error) -> Self {
        Self::internal(err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code.to_string(),
                message: self.message,
            },
        };
        (self.status, Json(body)).into_response()
    }
}
