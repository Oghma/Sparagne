use axum::{http::StatusCode, response::IntoResponse, Json};
use engine::EngineError;

use serde::Serialize;
pub use server::run;

mod cash_flow;
mod entry;
mod server;
mod statistics;
mod user;
mod vault;

pub mod types {
    pub mod cash_flow {
        pub use crate::cash_flow::CashFlowGet;
        pub use engine::CashFlow;
    }

    pub mod vault {
        pub use crate::vault::Vault;
        pub use crate::vault::VaultNew;
    }

    pub mod user {
        pub use crate::user::PairUser;
    }

    pub mod entry {
        pub use crate::entry::EntryDelete;
        pub use crate::entry::EntryNew;
    }

    pub mod stats {
        pub use crate::statistics::Statistic;
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
