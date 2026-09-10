//! Shared harness for the sync acceptance tests (`docs/v2/SYNC.md`).
//!
//! `Api` is a trimmed copy of the request/response plumbing in `api.rs`: JSON
//! in, JSON out, bearer tokens, the router driven in-process with
//! `tower::ServiceExt::oneshot`. `Client` is a "real" client: an in-memory
//! [`Core`], a token and a username, synced over HTTP exactly as
//! `docs/v2/SYNC.md` §4-§5 prescribes.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, Entry, FlowMode, SyncReport, TransactionFilter,
    sync::{PullResponse, PushOutcome, PushResponse, PushResult, TokenResponse},
};
use sparagne_server::{AppState, Config, router};
use tower::ServiceExt;
use uuid::Uuid;

pub const T0: i64 = 1_700_000_000;
pub const PASSWORD: &str = "correct horse battery staple";

// ---------------------------------------------------------------------------
// HTTP harness
// ---------------------------------------------------------------------------

pub struct Api {
    pub app: Router,
    pub state: AppState,
}

pub struct Res {
    pub status: StatusCode,
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

impl Default for Api {
    fn default() -> Self {
        Self::new()
    }
}

impl Api {
    pub fn new() -> Self {
        let state = AppState::in_memory(Config::default()).expect("state");
        Self {
            app: router(state.clone()),
            state,
        }
    }

    pub async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Res {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let request = match body {
            Some(value) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("request");
        let response = self.app.clone().oneshot(request).await.expect("response");
        let status = response.status();
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
        Res { status, body }
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

    /// Registers `username` with the fixed test [`PASSWORD`] and returns its
    /// token.
    pub async fn register(&self, username: &str) -> String {
        let res = self
            .call(
                Method::POST,
                "/auth/register",
                None,
                Some(json!({ "username": username, "password": PASSWORD })),
            )
            .await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        res.json::<TokenResponse>().token
    }
}

// ---------------------------------------------------------------------------
// A synchronizing client
// ---------------------------------------------------------------------------

/// One of the two (or three) clients of the acceptance test: a local [`Core`]
/// plus the account it syncs as.
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

    /// Registers a fresh account on `api` and returns a client for it.
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

    /// `docs/v2/SYNC.md` §4-§5: create the vault on the server if this is the
    /// first sync, push the rest of the outbox, then pull until the local
    /// watermark reaches the server's last seq. Returns every [`SyncReport`]
    /// produced along the way (the push, then each pull page).
    pub async fn sync(&mut self, api: &Api, vault: Uuid) -> Vec<SyncReport> {
        let mut reports = Vec::new();

        if self.core.sync_state(vault).unwrap().last_server_seq == 0 {
            let outbox = self.core.push_request(vault).unwrap();
            if let Some(first) = outbox.commands.first()
                && matches!(first.command, Command::CreateVault { .. })
            {
                let res = api.post("/vaults", &self.token, json!(first)).await;
                assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
                let result: PushResult = res.json();
                let seq = match result.outcome {
                    PushOutcome::Applied { seq, .. } => seq,
                    PushOutcome::Rejected {
                        ref code,
                        ref message,
                    } => {
                        panic!("create_vault rejected: {code}: {message}")
                    }
                };
                let response = PushResponse {
                    results: vec![result],
                    last_seq: seq,
                };
                reports.push(self.core.apply_push_response(vault, &response).unwrap());
            }
        }

        let request = self.core.push_request(vault).unwrap();
        if !request.commands.is_empty() {
            let res = api
                .post(
                    &format!("/vaults/{vault}/push"),
                    &self.token,
                    json!(request),
                )
                .await;
            assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
            let response: PushResponse = res.json();
            reports.push(self.core.apply_push_response(vault, &response).unwrap());
        }

        loop {
            let since = self.core.sync_state(vault).unwrap().last_server_seq;
            let res = api
                .get(&format!("/vaults/{vault}/pull?since={since}"), &self.token)
                .await;
            assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
            let response: PullResponse = res.json();
            let last_seq = response.last_seq;
            reports.push(self.core.integrate_pull(vault, &response).unwrap());
            if self.core.sync_state(vault).unwrap().last_server_seq >= last_seq {
                break;
            }
        }

        reports
    }

    /// A vault not present locally: pull from zero (`docs/v2/SYNC.md` §4.3).
    pub async fn join(&mut self, api: &Api, vault: Uuid) -> SyncReport {
        let res = api
            .get(&format!("/vaults/{vault}/pull?since=0"), &self.token)
            .await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        let response: PullResponse = res.json();
        self.core.integrate_pull(vault, &response).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Command builders
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Projection comparison
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Shared setup
// ---------------------------------------------------------------------------

/// alice registers, creates "Casa" with a wallet holding 100.00 and an income
/// of 50.00, and syncs; bob registers, is added as an editor and joins by
/// pulling from zero. `docs/v2/SYNC.md` §6.
pub async fn basics(api: &Api) -> (Client, Client, Uuid, Uuid) {
    let mut alice = Client::register(api, "alice").await;
    let vault = alice.create_vault("Casa");
    let wallet = alice.exec(vault, wallet_cmd("Cash", 10_000));
    alice.exec(vault, income_cmd(5_000, wallet, None, "stipendio", T0));
    alice.sync(api, vault).await;

    assert_eq!(api.state.core().last_seq(vault).unwrap(), 3);
    let members: Vec<sparagne_core::sync::MemberEntry> = api
        .get(&format!("/vaults/{vault}/members"), &alice.token)
        .await
        .json();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].username, "alice");
    assert_eq!(members[0].role, sparagne_core::sync::MemberRole::Owner);

    let mut bob = Client::register(api, "bob").await;
    let put = api
        .put(
            &format!("/vaults/{vault}/members"),
            &alice.token,
            json!({ "username": "bob", "role": "editor" }),
        )
        .await;
    assert_eq!(put.status, StatusCode::NO_CONTENT, "{:?}", put.body);

    let bob_vaults: Vec<sparagne_core::sync::VaultSummary> =
        api.get("/vaults", &bob.token).await.json();
    let casa = bob_vaults
        .iter()
        .find(|v| v.id == vault)
        .expect("Casa visible to bob");
    assert_eq!(casa.name, "Casa");
    assert_eq!(casa.role, sparagne_core::sync::MemberRole::Editor);

    let report = bob.join(api, vault).await;
    assert!(report.rebased);
    assert_eq!(projection(&bob.core, vault), projection(&alice.core, vault));

    (alice, bob, vault, wallet)
}
