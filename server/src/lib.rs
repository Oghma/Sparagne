use axum::{http::StatusCode, response::IntoResponse, Json};
use engine::EngineError;

use serde::Serialize;
pub use server::run;

mod cash_flow;
mod entry;
mod server;
mod user;
mod vault;

pub mod types {
    pub use crate::vault::VaultNew;
}

pub struct ServerError(EngineError);

//TODO: Find a better solution
#[derive(Serialize)]
struct Error {
    error: String,
}

impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::BAD_REQUEST,
            Json(Error {
                error: format!("{}", self.0),
            }),
        )
            .into_response()
    }
}
