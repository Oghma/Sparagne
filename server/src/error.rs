//! Request-level errors and their HTTP mapping (`docs/v2/SYNC.md` §3).

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header::RETRY_AFTER},
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
    /// Seconds before trying again, sent as `Retry-After`.
    pub retry_after: Option<u64>,
}

impl ApiError {
    #[must_use]
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            retry_after: None,
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

    /// A rate limit tripped: `429` with `Retry-After`, never less than a
    /// second.
    #[must_use]
    pub fn too_many_requests(retry_after: u64) -> Self {
        let retry_after = retry_after.max(1);
        Self {
            retry_after: Some(retry_after),
            ..Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                format!("too many attempts, retry in {retry_after} seconds"),
            )
        }
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
            DomainError::Forbidden(_) => StatusCode::FORBIDDEN,
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
        let mut response = (self.status, Json(body)).into_response();
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forbidden_domain_error_is_403() {
        let err = ApiError::from(DomainError::Forbidden("only the owner".into()));
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(err.code, "forbidden");
    }

    #[test]
    fn other_domain_errors_keep_their_status() {
        let cases = [
            (DomainError::NotFound("x".into()), StatusCode::NOT_FOUND),
            (DomainError::AlreadyExists("x".into()), StatusCode::CONFLICT),
            (
                DomainError::InvalidName("x".into()),
                StatusCode::BAD_REQUEST,
            ),
            (
                DomainError::Storage("x".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (err, status) in cases {
            assert_eq!(ApiError::from(err).status, status);
        }
    }

    #[test]
    fn too_many_requests_sends_retry_after() {
        let response = ApiError::too_many_requests(42).into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers().get(RETRY_AFTER),
            Some(&HeaderValue::from(42_u64))
        );
        assert_eq!(ApiError::too_many_requests(0).retry_after, Some(1));
    }

    #[test]
    fn other_errors_send_no_retry_after() {
        let response = ApiError::forbidden().into_response();
        assert!(response.headers().get(RETRY_AFTER).is_none());
    }
}
