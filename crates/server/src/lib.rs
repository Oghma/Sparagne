use axum::{Json, http::StatusCode, response::IntoResponse};
use engine::EngineError;

use serde::Serialize;
pub use server::{run, run_with_listener, spawn_with_listener};

mod cash_flow;
mod memberships;
mod server;
mod statistics;
mod transactions;
mod user;
mod vault;

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
}

pub enum ServerError {
    Engine(EngineError),
    Generic(String),
}

//TODO: Find a better solution
#[derive(Serialize)]
struct Error {
    error: String,
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
        | EngineError::InvalidFlow(_)
        | EngineError::CurrencyMismatch(_) => StatusCode::UNPROCESSABLE_ENTITY,
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
        let (status, error) = match self {
            ServerError::Engine(err) => (status_for_engine_error(&err), message_for_engine_error(err)),
            ServerError::Generic(err) => (StatusCode::BAD_REQUEST, err),
        };

        (status, Json(Error { error })).into_response()
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

    #[test]
    fn engine_forbidden_maps_to_403() {
        let res = ServerError::from(EngineError::Forbidden("forbidden".to_string())).into_response();
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
}
