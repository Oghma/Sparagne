use axum::{Json, http::StatusCode, response::IntoResponse};
use engine::EngineError;

use serde::Serialize;
pub use server::run;

mod cash_flow;
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
            ExpenseNew, IncomeNew, TransactionCreated, TransactionList, TransactionListResponse,
            TransactionUpdate, TransactionView, TransactionVoid, TransferFlowNew,
            TransferWalletNew,
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

impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        let error = match self {
            ServerError::Engine(err) => err.to_string(),
            ServerError::Generic(err) => err,
        };

        (StatusCode::BAD_REQUEST, Json(Error { error })).into_response()
    }
}

impl From<EngineError> for ServerError {
    fn from(value: EngineError) -> Self {
        Self::Engine(value)
    }
}
