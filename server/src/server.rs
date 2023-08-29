use axum::{
    extract::State,
    headers::Header,
    routing::{get, post},
    Router,
};

use std::{net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;

use crate::cash_flow;
use crate::entry;
use engine::Engine;

static TELEGRAM_HEADER: axum::http::HeaderName =
    axum::http::HeaderName::from_static("telegram-user-id");

pub type SharedState = State<Arc<RwLock<Engine>>>;

/// `TypedHeader` for custom telegram header
#[derive(Debug)]
struct TelegramHeader(u64);

impl Header for TelegramHeader {
    fn name() -> &'static axum::http::HeaderName {
        &TELEGRAM_HEADER
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i axum::http::HeaderValue>,
    {
        let value = values.next().ok_or_else(axum::headers::Error::invalid)?;
        let Ok(value) = value.to_str() else {
            return Err(axum::headers::Error::invalid());
        };
        let Ok(value) = value.parse() else {
            return Err(axum::headers::Error::invalid());
        };

        Ok(TelegramHeader(value))
    }

    fn encode<E: Extend<axum::http::HeaderValue>>(&self, values: &mut E) {
        let value = axum::http::HeaderValue::from_str(&self.0.to_string()).unwrap();
        values.extend(std::iter::once(value));
    }
}

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
