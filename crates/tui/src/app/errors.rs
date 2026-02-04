//! Error-to-message mapping.
//!
//! This module handles mapping of API client errors to localized user-facing
//! messages. It translates error codes and error types into appropriate text
//! keys from the i18n system.

use api_types::error::ErrorCode;

use crate::text::{Locale, TextKey, format as text_format, t};

/// Maps a client error to a localized error message.
///
/// This function handles all error types returned by the API client and
/// translates them into user-friendly messages using the i18n system.
pub(crate) fn login_message_for_error(err: crate::client::ClientError, locale: Locale) -> String {
    match err {
        crate::client::ClientError::Unauthorized => {
            t(locale, TextKey::ErrorInvalidCredentials).to_string()
        }
        crate::client::ClientError::Forbidden(payload) => match payload.code {
            ErrorCode::MembershipLastOwner => {
                t(locale, TextKey::ErrorMembershipLastOwner).to_string()
            }
            ErrorCode::MembershipOwnerImmutable => {
                t(locale, TextKey::ErrorMembershipOwnerImmutable).to_string()
            }
            ErrorCode::MembershipOwnerRemoveForbidden => {
                t(locale, TextKey::ErrorMembershipOwnerRemoveForbidden).to_string()
            }
            _ => t(locale, TextKey::ErrorOperationForbidden).to_string(),
        },
        crate::client::ClientError::NotFound(payload) => match payload.code {
            ErrorCode::NotFound => t(locale, TextKey::ErrorResourceNotFound).to_string(),
            _ => payload.message,
        },
        crate::client::ClientError::Conflict(payload) => {
            text_format(locale, TextKey::ErrorConflict, &[("message", &payload.message)])
        }
        crate::client::ClientError::Validation(payload) => {
            if payload.message.contains("ambiguous vault name") {
                t(locale, TextKey::ErrorValidationAmbiguousVault).to_string()
            } else {
                text_format(locale, TextKey::ErrorValidation, &[("message", &payload.message)])
            }
        }
        crate::client::ClientError::BadRequest(payload) => {
            text_format(locale, TextKey::ErrorBadRequest, &[("message", &payload.message)])
        }
        crate::client::ClientError::Server(payload) => {
            text_format(locale, TextKey::ErrorServerError, &[("message", &payload.message)])
        }
        crate::client::ClientError::Client(message) => message,
        crate::client::ClientError::Transport(err) => {
            text_format(locale, TextKey::ErrorServerUnreachable, &[("error", &err.to_string())])
        }
    }
}
