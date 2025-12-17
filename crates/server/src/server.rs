use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, Error as AxumError, Header, authorization::Basic},
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use std::sync::Arc;

use crate::{cash_flow, memberships, statistics, transactions, user, vault};
use engine::Engine;

static TELEGRAM_HEADER: axum::http::HeaderName =
    axum::http::HeaderName::from_static("telegram-user-id");

#[derive(Clone)]
pub struct ServerState {
    pub engine: Arc<Engine>,
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

    fn decode<'i, I>(values: &mut I) -> Result<Self, AxumError>
    where
        Self: Sized,
        I: Iterator<Item = &'i axum::http::HeaderValue>,
    {
        let value = values.next().ok_or_else(AxumError::invalid)?;
        let Ok(value) = value.to_str() else {
            return Err(AxumError::invalid());
        };
        let Ok(value) = value.parse() else {
            return Err(AxumError::invalid());
        };

        Ok(TelegramHeader(value))
    }

    fn encode<E: Extend<axum::http::HeaderValue>>(&self, values: &mut E) {
        let as_string = self.0.to_string();
        match axum::http::HeaderValue::from_str(&as_string) {
            Ok(value) => values.extend(std::iter::once(value)),
            Err(_) => tracing::error!("failed to encode telegram-user-id header"),
        }
    }
}

async fn auth(
    auth_header: TypedHeader<Authorization<Basic>>,
    telegram_header: Option<TypedHeader<TelegramHeader>>,
    State(state): State<ServerState>,
    mut request: Request,
    next: Next,
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

fn router(state: ServerState) -> Router {
    Router::new()
        .route("/cashFlow", get(cash_flow::get))
        .route("/transactions", get(transactions::list))
        .route("/transactions/get", post(transactions::get_detail))
        .route("/income", post(transactions::income_new))
        .route("/expense", post(transactions::expense_new))
        .route("/refund", post(transactions::refund_new))
        .route("/transferWallet", post(transactions::transfer_wallet_new))
        .route("/transferFlow", post(transactions::transfer_flow_new))
        .route(
            "/transactions/:id",
            axum::routing::patch(transactions::update),
        )
        .route("/transactions/:id/void", post(transactions::void_tx))
        .route("/vault", post(vault::vault_new).get(vault::get))
        .route(
            "/vault/:vault_id/members",
            get(memberships::list_vault_members).post(memberships::upsert_vault_member),
        )
        .route(
            "/vault/:vault_id/members/:username",
            axum::routing::delete(memberships::remove_vault_member),
        )
        .route(
            "/vault/:vault_id/flows/:flow_id/members",
            get(memberships::list_flow_members).post(memberships::upsert_flow_member),
        )
        .route(
            "/vault/:vault_id/flows/:flow_id/members/:username",
            axum::routing::delete(memberships::remove_flow_member),
        )
        .route("/user/pair", post(user::pair).delete(user::unpair))
        .route("/stats", get(statistics::get_stats))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

pub async fn run(engine: Engine, db: DatabaseConnection) {
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:3000").await {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!("failed to bind server listener: {err}");
            return;
        }
    };
    if let Err(err) = run_with_listener(engine, db, listener).await {
        tracing::error!("server failed: {err}");
    }
}

pub async fn run_with_listener(
    engine: Engine,
    db: DatabaseConnection,
    listener: tokio::net::TcpListener,
) -> Result<(), std::io::Error> {
    let addr = listener.local_addr()?;
    tracing::info!("Server listening on {}", addr);

    let state = ServerState {
        engine: Arc::new(engine),
        db,
    };

    axum::serve(listener, router(state)).await
}

pub fn spawn_with_listener(
    engine: Engine,
    db: DatabaseConnection,
    listener: tokio::net::TcpListener,
) -> Result<std::net::SocketAddr, std::io::Error> {
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        if let Err(err) = run_with_listener(engine, db, listener).await {
            tracing::error!("server failed: {err}");
        }
    });

    Ok(addr)
}
