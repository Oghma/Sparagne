use api_types::error::{ErrorCode, ErrorDetails, ErrorEnvelope, ErrorPayload};
use axum::{Json, http::StatusCode, response::IntoResponse};
use engine::EngineError;
use std::collections::BTreeMap;

pub use server::{run, run_with_listener, spawn_with_listener};

mod cash_flow;
mod categories;
mod flows;
mod memberships;
mod server;
mod statistics;
mod transactions;
mod user;
mod vault;
mod wallets;

pub mod types {
    pub mod cash_flow {
        pub use api_types::cash_flow::CashFlowGet;
        pub use engine::CashFlow;
    }

    pub mod vault {
        pub use api_types::vault::{Vault, VaultNew};
    }

    pub mod user {
        pub use api_types::user::PairUser;
    }

    pub mod transaction {
        pub use api_types::transaction::{
            ExpenseNew, IncomeNew, LegTarget, TransactionCreated, TransactionDetailResponse,
            TransactionGet, TransactionHeaderView, TransactionLegView, TransactionList,
            TransactionListResponse, TransactionUpdate, TransactionView, TransactionVoid,
            TransferFlowNew, TransferWalletNew,
        };
    }

    pub mod stats {
        pub use api_types::stats::Statistic;
    }

    pub mod category {
        pub use api_types::category::{
            CategoryAliasCreate, CategoryAliasCreated, CategoryAliasDelete, CategoryAliasList,
            CategoryAliasListResponse, CategoryAliasView, CategoryCreate, CategoryCreated,
            CategoryList, CategoryListResponse, CategoryMerge, CategoryMergeConflict,
            CategoryMergePreview, CategoryMergePreviewResponse, CategoryUpdate, CategoryView,
        };
    }

    pub mod wallet {
        pub use api_types::wallet::{WalletCreated, WalletNew, WalletUpdate};
    }

    pub mod flow {
        pub use api_types::flow::{FlowCreated, FlowMode, FlowNew, FlowUpdate};
    }
}

pub enum ServerError {
    Engine(EngineError),
    Generic(String),
}

fn status_for_engine_error(err: &EngineError) -> StatusCode {
    match err {
        EngineError::Forbidden(_) => StatusCode::FORBIDDEN,
        EngineError::KeyNotFound(_) => StatusCode::NOT_FOUND,
        EngineError::ExistingKey(_) => StatusCode::CONFLICT,
        EngineError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        EngineError::MaxBalanceReached(_)
        | EngineError::InsufficientFunds(_)
        | EngineError::InvalidAmount(_)
        | EngineError::InvalidName(_)
        | EngineError::InvalidId(_)
        | EngineError::InvalidCursor(_)
        | EngineError::InvalidFlow(_)
        | EngineError::InvalidRole(_)
        | EngineError::CurrencyMismatch(_) => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

fn code_for_engine_error(err: &EngineError) -> ErrorCode {
    match err {
        EngineError::Forbidden(message) => match message.as_str() {
            "cannot remove last flow owner" => ErrorCode::MembershipLastOwner,
            "cannot change vault owner role" => ErrorCode::MembershipOwnerImmutable,
            "cannot remove vault owner" => ErrorCode::MembershipOwnerRemoveForbidden,
            _ => ErrorCode::Forbidden,
        },
        EngineError::KeyNotFound(_) => ErrorCode::NotFound,
        EngineError::ExistingKey(_) => ErrorCode::Conflict,
        EngineError::Database(_) => ErrorCode::DatabaseError,
        EngineError::MaxBalanceReached(_) => ErrorCode::MaxBalanceReached,
        EngineError::InsufficientFunds(_) => ErrorCode::InsufficientFunds,
        EngineError::InvalidAmount(_) => ErrorCode::InvalidAmount,
        EngineError::InvalidName(_) => ErrorCode::InvalidName,
        EngineError::InvalidId(_) => ErrorCode::InvalidId,
        EngineError::InvalidCursor(_) => ErrorCode::InvalidCursor,
        EngineError::InvalidFlow(_) => ErrorCode::InvalidFlow,
        EngineError::InvalidRole(_) => ErrorCode::InvalidRole,
        EngineError::CurrencyMismatch(_) => ErrorCode::CurrencyMismatch,
    }
}

fn details_from_pairs(pairs: &[(&str, &str)]) -> ErrorDetails {
    let mut details = BTreeMap::new();
    for (key, value) in pairs {
        details.insert((*key).to_string(), (*value).to_string());
    }
    details
}

fn details_for_engine_error(err: &EngineError) -> Option<ErrorDetails> {
    match err {
        EngineError::Forbidden(message) => match message.as_str() {
            "cannot remove last flow owner" => Some(details_from_pairs(&[
                ("scope", "flow_membership"),
                ("reason", "last_owner"),
            ])),
            "cannot change vault owner role" => Some(details_from_pairs(&[
                ("scope", "vault_membership"),
                ("reason", "owner_immutable"),
            ])),
            "cannot remove vault owner" => Some(details_from_pairs(&[
                ("scope", "vault_membership"),
                ("reason", "owner_remove_forbidden"),
            ])),
            _ => None,
        },
        EngineError::InvalidAmount(_) => Some(details_from_pairs(&[("field", "amount_minor")])),
        EngineError::InvalidName(_) => Some(details_from_pairs(&[("field", "name")])),
        EngineError::InvalidId(_) => Some(details_from_pairs(&[("field", "id")])),
        EngineError::InvalidCursor(_) => Some(details_from_pairs(&[("field", "cursor")])),
        EngineError::InvalidRole(_) => Some(details_from_pairs(&[("field", "role")])),
        EngineError::CurrencyMismatch(_) => Some(details_from_pairs(&[("field", "currency")])),
        _ => None,
    }
}

fn message_for_engine_error(err: EngineError) -> String {
    match err {
        EngineError::Database(db_err) => {
            tracing::error!("database error: {db_err}");
            "internal server error".to_string()
        }
        other => other.to_string(),
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        let (status, payload) = match self {
            ServerError::Engine(err) => {
                let status = status_for_engine_error(&err);
                let code = code_for_engine_error(&err);
                let details = details_for_engine_error(&err);
                let message = message_for_engine_error(err);
                (
                    status,
                    ErrorPayload {
                        code,
                        message,
                        details,
                    },
                )
            }
            ServerError::Generic(message) => (
                StatusCode::BAD_REQUEST,
                ErrorPayload {
                    code: ErrorCode::BadRequest,
                    message,
                    details: None,
                },
            ),
        };

        (status, Json(ErrorEnvelope { error: payload })).into_response()
    }
}

impl From<EngineError> for ServerError {
    fn from(value: EngineError) -> Self {
        Self::Engine(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[test]
    fn engine_forbidden_maps_to_403() {
        let res =
            ServerError::from(EngineError::Forbidden("forbidden".to_string())).into_response();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn engine_not_found_maps_to_404() {
        let res = ServerError::from(EngineError::KeyNotFound("x".to_string())).into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn engine_conflict_maps_to_409() {
        let res = ServerError::from(EngineError::ExistingKey("x".to_string())).into_response();
        assert_eq!(res.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn engine_validation_maps_to_422() {
        let res = ServerError::from(EngineError::InvalidAmount("x".to_string())).into_response();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn generic_maps_to_400() {
        let res = ServerError::Generic("bad".to_string()).into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn membership_forbidden_includes_code() {
        let res = ServerError::from(EngineError::Forbidden(
            "cannot remove last flow owner".to_string(),
        ))
        .into_response();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let payload: ErrorEnvelope = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload.error.code, ErrorCode::MembershipLastOwner);
        let details = payload.error.details.unwrap_or_default();
        assert_eq!(details.get("scope"), Some(&"flow_membership".to_string()));
    }
}
