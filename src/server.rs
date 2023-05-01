use axum::{
    extract::State,
    routing::{get, post},
    Router,
};

use std::{net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;

use crate::engine::Engine;

mod cash_flow;
mod entry;

pub type SharedState = State<Arc<RwLock<Engine>>>;

pub async fn run(engine: Engine) {
    let state = Arc::new(RwLock::new(engine));

    let app = Router::new()
        .route("/allCashFlows", get(cash_flow::cashflow_names))
        .route("/cashFlow", post(cash_flow::cashflow_new))
        .route("/entry", post(entry::entry_new))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
