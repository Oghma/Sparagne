use axum::{
    extract::State,
    headers::{authorization::Basic, Authorization, Header},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::post,
    Router, TypedHeader,
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use std::{net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;

use crate::{user, vault};
use engine::Engine;

static TELEGRAM_HEADER: axum::http::HeaderName =
    axum::http::HeaderName::from_static("telegram-user-id");

#[derive(Clone)]
pub struct ServerState {
    pub engine: Arc<RwLock<Engine>>,
    pub db: DatabaseConnection,
}

/// `TypedHeader` for custom telegram header
///
/// Telegram requests must contain "telegram-user-id" entry in the header.
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

async fn auth<B>(
    auth_header: TypedHeader<Authorization<Basic>>,
    telegram_header: Option<TypedHeader<TelegramHeader>>,
    State(state): State<ServerState>,
    mut request: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    if auth_header.username().is_empty() || auth_header.password().is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let user: Option<user::Model> = user::Entity::find()
        .filter(user::Column::Username.contains(auth_header.username()))
        .filter(user::Column::Password.contains(auth_header.password()))
        .one(&state.db)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let mut user = if let Some(user) = user {
        user
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if let Some(header) = telegram_header {
        let header = header.0;
        let user_entry = user::Entity::find()
            .filter(user::Column::TelegramId.eq(header.0))
            .one(&state.db)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        user = if let Some(user) = user_entry {
            user
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        };
    }

    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

pub async fn run(engine: Engine, db: DatabaseConnection) {
    let state = ServerState {
        engine: Arc::new(RwLock::new(engine)),
        db,
    };

    let app = Router::new()
        // .route("/allCashFlows", get(cash_flow::cashflow_names))
        // .route("/cashFlow", post(cash_flow::cashflow_new))
        // .route("/entry", post(entry::entry_new))
        .route("/vault", post(vault::vault_new))
        .route("/user/pair", post(user::pair).delete(user::unpair))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
