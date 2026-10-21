//! Shared harness for the server tests: `Api` drives the router in-process
//! with `tower::ServiceExt::oneshot` (JSON in and out, bearer tokens), and
//! `Client` is an in-memory [`Core`] that syncs over HTTP as the app does.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, Entry, FlowMode, SyncReport, TransactionFilter,
    sync::{
        MemberEntry, MemberRole, PullResponse, PushOutcome, PushResponse, PushResult,
        TokenResponse, VaultSummary,
    },
};
use sparagne_server::{AppState, Config, router};
use tower::ServiceExt;
use uuid::Uuid;

pub const T0: i64 = 1_700_000_000;
pub const PASSWORD: &str = "correct horse battery staple";
/// What the app uses: a push carries at most this many commands.
pub const BATCH: usize = 500;

pub struct Api {
    pub app: Router,
    pub state: AppState,
}

pub struct Res {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl Res {
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_value(self.body.clone()).expect("unexpected body shape")
    }

    /// The `error.code` of an [`ErrorBody`](sparagne_core::sync::ErrorBody).
    pub fn code(&self) -> String {
        self.body["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }
}

impl Api {
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    pub fn with_config(config: Config) -> Self {
        let state = AppState::in_memory(config).expect("state");
        Self {
            app: router(state.clone()),
            state,
        }
    }

    /// A request as [`Api::call`] sends it, for tests that add headers or
    /// extensions (the peer address) before [`Api::send`].
    pub fn request(
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Request<Body> {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        match body {
            Some(value) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("request")
    }

    pub async fn send(&self, request: Request<Body>) -> Res {
        let response = self.app.clone().oneshot(request).await.expect("response");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        Res {
            status,
            headers,
            body,
        }
    }

    pub async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Res {
        self.send(Self::request(method, uri, token, body)).await
    }

    pub async fn get(&self, uri: &str, token: &str) -> Res {
        self.call(Method::GET, uri, Some(token), None).await
    }

    pub async fn post(&self, uri: &str, token: &str, body: Value) -> Res {
        self.call(Method::POST, uri, Some(token), Some(body)).await
    }

    pub async fn put(&self, uri: &str, token: &str, body: Value) -> Res {
        self.call(Method::PUT, uri, Some(token), Some(body)).await
    }

    pub async fn delete(&self, uri: &str, token: &str) -> Res {
        self.call(Method::DELETE, uri, Some(token), None).await
    }

    /// `POST /auth/register` with any credentials, whatever the outcome.
    pub async fn register_as(&self, username: &str, password: &str) -> Res {
        let body = json!({ "username": username, "password": password });
        self.call(Method::POST, "/auth/register", None, Some(body))
            .await
    }

    /// Registers `username` with [`PASSWORD`] and returns its token.
    pub async fn register(&self, username: &str) -> String {
        let res = self.register_as(username, PASSWORD).await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        res.json::<TokenResponse>().token
    }

    /// `POST /auth/login`, whatever the outcome.
    pub async fn login(&self, username: &str, password: &str) -> Res {
        let body = json!({ "username": username, "password": password });
        self.call(Method::POST, "/auth/login", None, Some(body))
            .await
    }

    pub async fn push(&self, token: &str, vault: Uuid, commands: Vec<CommandEnvelope>) -> Res {
        let body = json!({ "commands": commands });
        self.post(&format!("/vaults/{vault}/push"), token, body)
            .await
    }

    /// Creates a vault owned by `author` with a push whose first command is
    /// the vault's own `CreateVault`, and returns its id.
    pub async fn create_vault(&self, token: &str, author: &str, name: &str) -> Uuid {
        let envelope = CommandEnvelope::create_vault(author, name, Currency::Eur);
        let id = envelope.id;
        let res = self.push(token, id, vec![envelope]).await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        assert_eq!(applied(&res.json::<PushResponse>().results[0]), 1);
        id
    }

    /// `PUT /vaults/{vault}/members`, whatever the outcome.
    pub async fn set_member(&self, token: &str, vault: Uuid, username: &str, role: &str) -> Res {
        let body = json!({ "username": username, "role": role });
        self.put(&format!("/vaults/{vault}/members"), token, body)
            .await
    }

    /// [`Api::set_member`], which must succeed.
    pub async fn add_member(&self, token: &str, vault: Uuid, username: &str, role: &str) {
        let res = self.set_member(token, vault, username, role).await;
        assert_eq!(res.status, StatusCode::NO_CONTENT, "{:?}", res.body);
    }
}

pub fn applied(result: &PushResult) -> i64 {
    match result.outcome {
        PushOutcome::Applied { seq, .. } => seq,
        PushOutcome::Rejected { .. } => panic!("expected applied, got {:?}", result.outcome),
    }
}

pub fn rejection(result: &PushResult) -> (String, String) {
    match &result.outcome {
        PushOutcome::Rejected { code, message, .. } => (code.clone(), message.clone()),
        PushOutcome::Applied { .. } => panic!("expected rejected, got {:?}", result.outcome),
    }
}

/// One member's device: a local [`Core`] plus the account it syncs as.
pub struct Client {
    pub core: Core,
    pub token: String,
    pub username: String,
}

impl Client {
    pub fn new(token: String, username: impl Into<String>) -> Self {
        Self {
            core: Core::open_in_memory().expect("in-memory core"),
            token,
            username: username.into(),
        }
    }

    pub async fn register(api: &Api, username: &str) -> Self {
        let token = api.register(username).await;
        Self::new(token, username)
    }

    pub fn exec(&mut self, vault: Uuid, command: Command) -> Uuid {
        self.core
            .execute(CommandEnvelope::new(vault, self.username.clone(), command))
            .expect("command applies locally")
            .result_id
            .unwrap_or(vault)
    }

    /// A fresh `CreateVault` command, executed locally (outbox only).
    pub fn create_vault(&mut self, name: &str) -> Uuid {
        self.core
            .execute(CommandEnvelope::create_vault(
                self.username.clone(),
                name,
                Currency::Eur,
            ))
            .expect("create_vault applies locally")
            .result_id
            .expect("create_vault returns the vault id")
    }

    /// Pushes the outbox in batches until it is empty (the first push also
    /// creates a vault the server has never seen), then pulls until nothing
    /// more is waiting. Returns every [`SyncReport`] along the way.
    pub async fn sync(&mut self, api: &Api, vault: Uuid) -> Vec<SyncReport> {
        let mut reports = Vec::new();

        while self.core.sync_state(vault).unwrap().outbox > 0 {
            let before = self.core.sync_state(vault).unwrap().outbox;
            let request = self.core.push_request(vault, BATCH).unwrap();
            let res = api.push(&self.token, vault, request.commands).await;
            assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
            let response: PushResponse = res.json();
            reports.push(self.core.apply_push_response(vault, &response).unwrap());
            assert!(
                self.core.sync_state(vault).unwrap().outbox < before,
                "a push that confirms nothing would loop forever"
            );
        }

        loop {
            let since = self.core.sync_state(vault).unwrap().last_server_seq;
            let res = api
                .get(&format!("/vaults/{vault}/pull?since={since}"), &self.token)
                .await;
            assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
            let response: PullResponse = res.json();
            let report = self.core.integrate_pull(vault, &response).unwrap();
            let more = report.has_more;
            reports.push(report);
            if !more || self.core.sync_state(vault).unwrap().last_server_seq <= since {
                break;
            }
        }

        reports
    }

    /// A vault not present locally: pull from zero.
    pub async fn join(&mut self, api: &Api, vault: Uuid) -> SyncReport {
        let res = api
            .get(&format!("/vaults/{vault}/pull?since=0"), &self.token)
            .await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        let response: PullResponse = res.json();
        self.core.integrate_pull(vault, &response).unwrap()
    }
}

pub fn at(secs: i64) -> DateTime<FixedOffset> {
    Utc.timestamp_opt(secs, 0)
        .single()
        .expect("timestamp")
        .fixed_offset()
}

pub fn wallet_cmd(name: &str, opening: i64) -> Command {
    Command::CreateWallet {
        name: name.to_string(),
        opening_balance: opening,
        occurred_at: at(T0),
    }
}

pub fn flow_cmd(name: &str, mode: FlowMode, allow_negative: bool, opening: i64) -> Command {
    Command::CreateFlow {
        name: name.to_string(),
        mode,
        allow_negative,
        opening_allocation: opening,
        occurred_at: at(T0),
    }
}

pub fn entry(
    amount: i64,
    wallet: Option<Uuid>,
    flow: Option<Uuid>,
    category: Option<&str>,
    secs: i64,
) -> Entry {
    Entry {
        amount,
        wallet_id: wallet,
        flow_id: flow,
        category: category.map(str::to_string),
        note: None,
        occurred_at: at(secs),
        person: None,
    }
}

pub fn income_cmd(
    amount: i64,
    wallet: Uuid,
    flow: Option<Uuid>,
    category: &str,
    secs: i64,
) -> Command {
    Command::Income(entry(amount, Some(wallet), flow, Some(category), secs))
}

pub fn expense_cmd(
    amount: i64,
    wallet: Uuid,
    flow: Option<Uuid>,
    category: &str,
    secs: i64,
) -> Command {
    Command::Expense(entry(amount, Some(wallet), flow, Some(category), secs))
}

/// Every transaction, voided and transfers included.
pub fn all_txns() -> TransactionFilter {
    TransactionFilter {
        include_voided: true,
        include_transfers: true,
        ..Default::default()
    }
}

/// Everything a user can see of a vault, in one comparable value.
pub type Projection = (
    sparagne_core::VaultSnapshot,
    Vec<sparagne_core::TransactionView>,
    Vec<sparagne_core::CategoryView>,
    Vec<sparagne_core::AliasView>,
);

pub fn projection(core: &Core, vault: Uuid) -> Projection {
    (
        core.snapshot(vault).unwrap(),
        core.list_transactions(vault, &all_txns(), 100, None)
            .unwrap()
            .items,
        core.categories(vault, true).unwrap(),
        core.aliases(vault).unwrap(),
    )
}

/// alice registers, creates "Casa" with a wallet holding 100.00 and an income
/// of 50.00, and syncs; bob registers, is added as an editor and joins by
/// pulling from zero.
pub async fn basics(api: &Api) -> (Client, Client, Uuid, Uuid) {
    let mut alice = Client::register(api, "alice").await;
    let vault = alice.create_vault("Casa");
    let wallet = alice.exec(vault, wallet_cmd("Cash", 10_000));
    alice.exec(vault, income_cmd(5_000, wallet, None, "stipendio", T0));
    alice.sync(api, vault).await;

    assert_eq!(api.state.core().last_seq(vault).unwrap(), 3);
    let members: Vec<MemberEntry> = api
        .get(&format!("/vaults/{vault}/members"), &alice.token)
        .await
        .json();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].username, "alice");
    assert_eq!(members[0].role, MemberRole::Owner);

    let mut bob = Client::register(api, "bob").await;
    api.add_member(&alice.token, vault, "bob", "editor").await;

    let bob_vaults: Vec<VaultSummary> = api.get("/vaults", &bob.token).await.json();
    let casa = bob_vaults
        .iter()
        .find(|v| v.id == vault)
        .expect("Casa visible to bob");
    assert_eq!(casa.name, "Casa");
    assert_eq!(casa.role, MemberRole::Editor);

    let report = bob.join(api, vault).await;
    assert!(report.rebased);
    assert_eq!(projection(&bob.core, vault), projection(&alice.core, vault));

    (alice, bob, vault, wallet)
}
