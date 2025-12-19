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

use crate::{cash_flow, flows, memberships, statistics, transactions, user, vault, wallets};
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
struct TelegramHeader(String);

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
        Ok(TelegramHeader(value.to_string()))
    }

    fn encode<E: Extend<axum::http::HeaderValue>>(&self, values: &mut E) {
        match axum::http::HeaderValue::from_str(&self.0) {
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
        .route("/cashFlow/get", post(cash_flow::get))
        .route("/transactions", post(transactions::list))
        .route("/transactions/get", post(transactions::get_detail))
        .route("/wallets", post(wallets::wallet_new))
        .route(
            "/wallets/{id}",
            axum::routing::patch(wallets::wallet_update),
        )
        .route("/flows", post(flows::flow_new))
        .route("/flows/{id}", axum::routing::patch(flows::flow_update))
        .route("/income", post(transactions::income_new))
        .route("/expense", post(transactions::expense_new))
        .route("/refund", post(transactions::refund_new))
        .route("/transferWallet", post(transactions::transfer_wallet_new))
        .route("/transferFlow", post(transactions::transfer_flow_new))
        .route(
            "/transactions/{id}",
            axum::routing::patch(transactions::update),
        )
        .route("/transactions/{id}/void", post(transactions::void_tx))
        .route("/vault/new", post(vault::vault_new))
        .route("/vault/get", post(vault::get))
        .route("/vault/snapshot", post(vault::snapshot))
        .route(
            "/vault/{vault_id}/members",
            get(memberships::list_vault_members).post(memberships::upsert_vault_member),
        )
        .route(
            "/vault/{vault_id}/members/{username}",
            axum::routing::delete(memberships::remove_vault_member),
        )
        .route(
            "/vault/{vault_id}/flows/{flow_id}/members",
            get(memberships::list_flow_members).post(memberships::upsert_flow_member),
        )
        .route(
            "/vault/{vault_id}/flows/{flow_id}/members/{username}",
            axum::routing::delete(memberships::remove_flow_member),
        )
        .route("/user/pair", post(user::pair).delete(user::unpair))
        .route("/stats/get", post(statistics::get_stats))
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

#[cfg(test)]
mod http_tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    use api_types::{
        flow,
        transaction::{TransactionDetailResponse, TransactionGet, TransactionList},
        wallet,
    };
    use base64::Engine as _;
    use chrono::{FixedOffset, Utc};
    use http_body_util::BodyExt as _;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue, Database};
    use tower::ServiceExt as _;

    const OWNER: &str = "owner";
    const OWNER_PW: &str = "pw";
    const FLOW_MEMBER: &str = "alice";
    const FLOW_MEMBER_PW: &str = "pw";

    fn basic_auth(username: &str, password: &str) -> String {
        let raw = format!("{username}:{password}");
        let encoded = base64::prelude::BASE64_STANDARD.encode(raw);
        format!("Basic {encoded}")
    }

    async fn setup() -> (Router, Arc<Engine>, sea_orm::DatabaseConnection) {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();

        async fn insert_user(db: &sea_orm::DatabaseConnection, username: &str, password: &str) {
            let active = crate::user::ActiveModel {
                username: ActiveValue::Set(username.to_string()),
                password: ActiveValue::Set(password.to_string()),
                telegram_id: ActiveValue::Set(None),
                pair_code: ActiveValue::Set(None),
            };
            active.insert(db).await.unwrap();
        }

        insert_user(&db, OWNER, OWNER_PW).await;
        insert_user(&db, FLOW_MEMBER, FLOW_MEMBER_PW).await;

        let engine = Arc::new(
            Engine::builder()
                .database(db.clone())
                .build()
                .await
                .unwrap(),
        );

        let state = ServerState {
            engine: engine.clone(),
            db: db.clone(),
        };

        (router(state), engine, db)
    }

    #[tokio::test]
    async fn flow_member_can_list_transactions_for_flow_but_cannot_get_detail() {
        let (app, engine, _db) = setup().await;

        let vault_id = engine
            .new_vault("Main", OWNER, Some(engine::Currency::Eur))
            .await
            .unwrap();
        let flow_id = engine
            .new_cash_flow(&vault_id, "Shared", 0, None, None, OWNER)
            .await
            .unwrap();
        engine
            .upsert_flow_member(&vault_id, flow_id, FLOW_MEMBER, "viewer", OWNER)
            .await
            .unwrap();

        let vault = engine
            .vault_snapshot(Some(&vault_id), None, OWNER)
            .await
            .unwrap();
        let wallet_id = vault
            .wallet
            .values()
            .find(|w| w.name.eq_ignore_ascii_case("Cash"))
            .unwrap()
            .id;

        let tx_id = engine
            .income(engine::IncomeCmd {
                vault_id: vault_id.clone(),
                amount_minor: 1000,
                flow_id: Some(flow_id),
                wallet_id: Some(wallet_id),
                meta: engine::TxMeta {
                    category: None,
                    note: None,
                    idempotency_key: None,
                    occurred_at: Utc::now(),
                },
                user_id: OWNER.to_string(),
            })
            .await
            .unwrap();

        // Flow member can list transactions for the shared flow.
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/transactions")
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(FLOW_MEMBER, FLOW_MEMBER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&TransactionList {
                    vault_id: vault_id.clone(),
                    flow_id: Some(flow_id),
                    wallet_id: None,
                    limit: Some(50),
                    cursor: None,
                    from: None,
                    to: None,
                    kinds: None,
                    include_voided: Some(false),
                    include_transfers: Some(false),
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Flow member cannot fetch transaction detail (vault-only).
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/transactions/get")
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(FLOW_MEMBER, FLOW_MEMBER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&TransactionGet {
                    vault_id: vault_id.clone(),
                    id: tx_id,
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn vault_owner_can_get_transaction_detail_and_wrong_vault_is_404() {
        let (app, engine, _db) = setup().await;

        let vault_id = engine
            .new_vault("Main", OWNER, Some(engine::Currency::Eur))
            .await
            .unwrap();
        let flow_id = engine
            .new_cash_flow(&vault_id, "Shared", 0, None, None, OWNER)
            .await
            .unwrap();
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, OWNER)
            .await
            .unwrap();
        let wallet_id = vault
            .wallet
            .values()
            .find(|w| w.name.eq_ignore_ascii_case("Cash"))
            .unwrap()
            .id;
        let tx_id = engine
            .income(engine::IncomeCmd {
                vault_id: vault_id.clone(),
                amount_minor: 1000,
                flow_id: Some(flow_id),
                wallet_id: Some(wallet_id),
                meta: engine::TxMeta {
                    category: None,
                    note: None,
                    idempotency_key: None,
                    occurred_at: Utc::now(),
                },
                user_id: OWNER.to_string(),
            })
            .await
            .unwrap();

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/transactions/get")
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(OWNER, OWNER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&TransactionGet {
                    vault_id: vault_id.clone(),
                    id: tx_id,
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let detail: TransactionDetailResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail.transaction.id, tx_id);

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/transactions/get")
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(OWNER, OWNER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&TransactionGet {
                    vault_id: "other".to_string(),
                    id: tx_id,
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn vault_owner_can_list_transactions_vault_wide() {
        let (app, engine, _db) = setup().await;

        let vault_id = engine
            .new_vault("Main", OWNER, Some(engine::Currency::Eur))
            .await
            .unwrap();
        let flow_id = engine
            .new_cash_flow(&vault_id, "Shared", 0, None, None, OWNER)
            .await
            .unwrap();
        let vault = engine
            .vault_snapshot(Some(&vault_id), None, OWNER)
            .await
            .unwrap();
        let wallet_id = vault
            .wallet
            .values()
            .find(|w| w.name.eq_ignore_ascii_case("Cash"))
            .unwrap()
            .id;
        let tx_id = engine
            .income(engine::IncomeCmd {
                vault_id: vault_id.clone(),
                amount_minor: 1000,
                flow_id: Some(flow_id),
                wallet_id: Some(wallet_id),
                meta: engine::TxMeta {
                    category: None,
                    note: None,
                    idempotency_key: None,
                    occurred_at: Utc::now(),
                },
                user_id: OWNER.to_string(),
            })
            .await
            .unwrap();

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/transactions")
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(OWNER, OWNER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&TransactionList {
                    vault_id: vault_id.clone(),
                    flow_id: None,
                    wallet_id: None,
                    limit: Some(50),
                    cursor: None,
                    from: None,
                    to: None,
                    kinds: None,
                    include_voided: Some(false),
                    include_transfers: Some(false),
                })
                .unwrap(),
            ))
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let list: api_types::transaction::TransactionListResponse =
            serde_json::from_slice(&body).unwrap();
        assert!(list.transactions.iter().any(|t| t.id == tx_id));
    }

    #[tokio::test]
    async fn vault_owner_can_create_and_update_wallet() {
        let (app, engine, _db) = setup().await;

        let vault_id = engine
            .new_vault("Main", OWNER, Some(engine::Currency::Eur))
            .await
            .unwrap();

        let utc = FixedOffset::east_opt(0).unwrap();
        let occurred_at = Utc::now().with_timezone(&utc);

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/wallets")
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(OWNER, OWNER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&wallet::WalletNew {
                    vault_id: vault_id.clone(),
                    name: "Bank".to_string(),
                    opening_balance_minor: 1234,
                    occurred_at,
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let created: wallet::WalletCreated = serde_json::from_slice(&body).unwrap();

        let req = axum::http::Request::builder()
            .method("PATCH")
            .uri(format!("/wallets/{}", created.id))
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(OWNER, OWNER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&wallet::WalletUpdate {
                    vault_id: vault_id.clone(),
                    name: Some("Bank X".to_string()),
                    archived: Some(true),
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let snapshot = engine
            .vault_snapshot(Some(&vault_id), None, OWNER)
            .await
            .unwrap();
        let wallet = snapshot.wallet.get(&created.id).unwrap();
        assert_eq!(wallet.name, "Bank X");
        assert!(wallet.archived);
    }

    #[tokio::test]
    async fn vault_owner_can_create_and_update_flow() {
        let (app, engine, _db) = setup().await;

        let vault_id = engine
            .new_vault("Main", OWNER, Some(engine::Currency::Eur))
            .await
            .unwrap();

        let utc = FixedOffset::east_opt(0).unwrap();
        let occurred_at = Utc::now().with_timezone(&utc);

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/flows")
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(OWNER, OWNER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&flow::FlowNew {
                    vault_id: vault_id.clone(),
                    name: "Vacanze".to_string(),
                    mode: flow::FlowMode::NetCapped { cap_minor: 10_000 },
                    opening_balance_minor: 500,
                    occurred_at,
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let created: flow::FlowCreated = serde_json::from_slice(&body).unwrap();

        let req = axum::http::Request::builder()
            .method("PATCH")
            .uri(format!("/flows/{}", created.id))
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth(OWNER, OWNER_PW),
            )
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&flow::FlowUpdate {
                    vault_id: vault_id.clone(),
                    name: Some("Vacanze 2026".to_string()),
                    archived: Some(true),
                    mode: Some(flow::FlowMode::IncomeCapped { cap_minor: 20_000 }),
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let snapshot = engine
            .vault_snapshot(Some(&vault_id), None, OWNER)
            .await
            .unwrap();
        let flow = snapshot.cash_flow.get(&created.id).unwrap();
        assert_eq!(flow.name, "Vacanze 2026");
        assert!(flow.archived);
        assert_eq!(flow.max_balance, Some(20_000));
        assert!(flow.income_balance.is_some());
    }
}
