//! HTTP acceptance tests for every row of `docs/v2/SYNC.md` §3.
//!
//! The router runs in-process: no socket, no clock injection beyond the
//! token TTL, one fresh in-memory pair of databases per test.

#![allow(clippy::unwrap_used, clippy::expect_used)]

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
    Command, CommandEnvelope, Currency, Entry, FlowMode,
    sync::{
        MemberEntry, PullResponse, PushOutcome, PushResponse, PushResult, TokenResponse,
        VaultSummary,
    },
};
use sparagne_server::{AppState, Config, router};
use tower::ServiceExt;
use uuid::Uuid;

const T0: i64 = 1_700_000_000;
const PASSWORD: &str = "correct horse";

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Api {
    app: Router,
}

struct Res {
    status: StatusCode,
    body: Value,
}

impl Res {
    fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_value(self.body.clone()).expect("unexpected body shape")
    }

    /// The `error.code` of an [`ErrorBody`](sparagne_core::sync::ErrorBody).
    fn code(&self) -> String {
        self.body["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }
}

impl Api {
    fn new() -> Self {
        Self::with_config(Config::default())
    }

    fn with_config(config: Config) -> Self {
        Self {
            app: router(AppState::in_memory(config).expect("state")),
        }
    }

    async fn call(
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

    async fn get(&self, uri: &str, token: &str) -> Res {
        self.call(Method::GET, uri, Some(token), None).await
    }

    async fn post(&self, uri: &str, token: &str, body: Value) -> Res {
        self.call(Method::POST, uri, Some(token), Some(body)).await
    }

    async fn put(&self, uri: &str, token: &str, body: Value) -> Res {
        self.call(Method::PUT, uri, Some(token), Some(body)).await
    }

    async fn delete(&self, uri: &str, token: &str) -> Res {
        self.call(Method::DELETE, uri, Some(token), None).await
    }

    /// Registers an account and returns its token.
    async fn register(&self, username: &str) -> String {
        let res = self.raw_register(username, PASSWORD).await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        res.json::<TokenResponse>().token
    }

    async fn raw_register(&self, username: &str, password: &str) -> Res {
        self.call(
            Method::POST,
            "/auth/register",
            None,
            Some(json!({ "username": username, "password": password })),
        )
        .await
    }

    async fn login(&self, username: &str, password: &str) -> Res {
        self.call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": username, "password": password })),
        )
        .await
    }

    /// Creates a vault owned by `author` and returns its id: a push whose
    /// first command is the vault's own `CreateVault`.
    async fn create_vault(&self, token: &str, author: &str, name: &str) -> Uuid {
        let envelope = CommandEnvelope::create_vault(author, name, Currency::Eur);
        let id = envelope.id;
        let res = self.push(token, id, vec![envelope]).await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        assert_eq!(applied(&res.json::<PushResponse>().results[0]), 1);
        id
    }

    async fn push(&self, token: &str, vault: Uuid, commands: Vec<CommandEnvelope>) -> Res {
        self.post(
            &format!("/vaults/{vault}/push"),
            token,
            json!({ "commands": commands }),
        )
        .await
    }
}

fn at(secs: i64) -> DateTime<FixedOffset> {
    Utc.timestamp_opt(secs, 0)
        .single()
        .expect("timestamp")
        .fixed_offset()
}

fn envelope(vault: Uuid, author: &str, command: Command) -> CommandEnvelope {
    CommandEnvelope::new(vault, author, command)
}

fn wallet(name: &str, opening: i64) -> Command {
    Command::CreateWallet {
        name: name.to_string(),
        opening_balance: opening,
        occurred_at: at(T0),
    }
}

fn income(amount: i64, flow: Option<Uuid>) -> Command {
    Command::Income(entry(amount, flow))
}

fn expense(amount: i64, flow: Option<Uuid>) -> Command {
    Command::Expense(entry(amount, flow))
}

fn entry(amount: i64, flow: Option<Uuid>) -> Entry {
    Entry {
        amount,
        wallet_id: None,
        flow_id: flow,
        category: None,
        note: None,
        occurred_at: at(T0),
        person: None,
    }
}

fn empty_flow(name: &str) -> Command {
    Command::CreateFlow {
        name: name.to_string(),
        mode: FlowMode::Unlimited,
        allow_negative: false,
        opening_allocation: 0,
        occurred_at: at(T0),
    }
}

fn applied(result: &PushResult) -> i64 {
    match result.outcome {
        PushOutcome::Applied { seq, .. } => seq,
        PushOutcome::Rejected { .. } => panic!("expected applied, got {:?}", result.outcome),
    }
}

fn rejection(result: &PushResult) -> (String, String) {
    match &result.outcome {
        PushOutcome::Rejected { code, message, .. } => (code.clone(), message.clone()),
        PushOutcome::Applied { .. } => panic!("expected rejected, got {:?}", result.outcome),
    }
}

// ---------------------------------------------------------------------------
// Health and routing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_is_public() {
    let api = Api::new();
    let res = api.call(Method::GET, "/health", None, None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn unknown_route_is_not_found() {
    let api = Api::new();
    let res = api.call(Method::GET, "/nope", None, None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.code(), "not_found");
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_returns_a_token_and_me_reports_the_username() {
    let api = Api::new();
    let res = api.raw_register("alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED);
    let token: TokenResponse = res.json();
    assert_eq!(token.username, "alice");
    assert!(token.expires_at > Utc::now().timestamp());
    assert!(!token.token.is_empty());

    let me = api.get("/me", &token.token).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body, json!({ "username": "alice" }));
}

#[tokio::test]
async fn register_can_be_disabled() {
    let api = Api::with_config(Config {
        allow_registration: false,
        ..Config::default()
    });
    let res = api.raw_register("alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "registration_disabled");
}

#[tokio::test]
async fn register_refuses_a_taken_username() {
    let api = Api::new();
    api.register("alice").await;
    let res = api.raw_register("alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.code(), "already_exists");
}

#[tokio::test]
async fn register_validates_the_username() {
    let api = Api::new();
    // "Alice" is no longer here: usernames are lowercased before validation.
    for username in ["ab", "with space", &"a".repeat(33)] {
        let res = api.raw_register(username, PASSWORD).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{username}");
        assert_eq!(res.code(), "invalid_request");
    }
}

#[tokio::test]
async fn register_validates_the_password() {
    let api = Api::new();
    let res = api.raw_register("alice", "short").await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
}

#[tokio::test]
async fn register_rejects_a_malformed_body() {
    let api = Api::new();
    let res = api
        .call(
            Method::POST,
            "/auth/register",
            None,
            Some(json!({ "username": "alice" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
}

#[tokio::test]
async fn login_checks_the_password() {
    let api = Api::new();
    api.register("alice").await;

    let ok = api.login("alice", PASSWORD).await;
    assert_eq!(ok.status, StatusCode::OK);
    let token: TokenResponse = ok.json();
    assert_eq!(api.get("/me", &token.token).await.status, StatusCode::OK);

    let wrong = api.login("alice", "not my password").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.code(), "unauthorized");

    let unknown = api.login("bob", PASSWORD).await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_revokes_the_token() {
    let api = Api::new();
    let token = api.register("alice").await;
    let res = api.post("/auth/logout", &token, json!({})).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    let me = api.get("/me", &token).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    assert_eq!(me.code(), "unauthorized");
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let api = Api::with_config(Config {
        token_ttl_days: 0,
        ..Config::default()
    });
    let res = api.raw_register("alice", PASSWORD).await;
    let token: TokenResponse = res.json();
    let me = api.get("/me", &token.token).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    assert_eq!(me.code(), "unauthorized");
}

#[tokio::test]
async fn a_missing_or_unknown_token_is_refused() {
    let api = Api::new();
    assert_eq!(
        api.call(Method::GET, "/me", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.get("/me", "made-up-token").await.status,
        StatusCode::UNAUTHORIZED
    );
}

// ---------------------------------------------------------------------------
// Vault creation, inside the first push
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_first_push_creates_the_vault_and_the_owner_membership() {
    let api = Api::new();
    let token = api.register("alice").await;
    let env = CommandEnvelope::create_vault("alice", "Main", Currency::Eur);
    let id = env.id;

    // The rest of the batch rides along in the same request.
    let res = api
        .push(
            &token,
            id,
            vec![env, envelope(id, "alice", wallet("Cash", 10_000))],
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let response: PushResponse = res.json();
    assert_eq!(response.results[0].command_id, id);
    assert_eq!(applied(&response.results[0]), 1);
    assert_eq!(applied(&response.results[1]), 2);
    assert_eq!(response.last_seq, 2);

    let members: Vec<MemberEntry> = api
        .get(&format!("/vaults/{id}/members"), &token)
        .await
        .json();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].username, "alice");
    assert_eq!(members[0].role.as_str(), "owner");

    // And the vault shows up in the listing, owned.
    let listed: Vec<VaultSummary> = api.get("/vaults", &token).await.json();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].name, "Main");
    assert_eq!(listed[0].last_seq, 2);
}

#[tokio::test]
async fn creating_a_vault_again_is_idempotent() {
    let api = Api::new();
    let token = api.register("alice").await;
    let env = CommandEnvelope::create_vault("alice", "Main", Currency::Eur);
    let id = env.id;

    let first: PushResponse = api.push(&token, id, vec![env.clone()]).await.json();
    let again = api.push(&token, id, vec![env]).await;
    assert_eq!(again.status, StatusCode::OK, "{:?}", again.body);
    assert_eq!(again.json::<PushResponse>(), first);
    assert_eq!(
        api.get("/vaults", &token)
            .await
            .json::<Vec<VaultSummary>>()
            .len(),
        1
    );
}

#[tokio::test]
async fn creating_a_vault_refuses_another_author() {
    let api = Api::new();
    let token = api.register("alice").await;
    let env = CommandEnvelope::create_vault("bob", "Main", Currency::Eur);
    let res = api.push(&token, env.id, vec![env]).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "author_mismatch");
}

#[tokio::test]
async fn creating_a_vault_accepts_a_name_the_caller_already_uses() {
    let api = Api::new();
    let token = api.register("alice").await;
    let first = api.create_vault(&token, "alice", "Main").await;
    // Names are labels: the second Main is a vault of its own, not a 409.
    let second = api.create_vault(&token, "alice", "Main").await;
    assert_ne!(first, second);
    let listed: Vec<VaultSummary> = api.get("/vaults", &token).await.json();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|v| v.name == "Main"));
    assert!(listed.iter().any(|v| v.id == first));
    assert!(listed.iter().any(|v| v.id == second));
}

#[tokio::test]
async fn a_push_to_an_unknown_vault_that_does_not_create_it_is_not_found() {
    let api = Api::new();
    let token = api.register("alice").await;
    let unknown = Uuid::now_v7();

    // No first command at all.
    let empty = api.push(&token, unknown, vec![]).await;
    assert_eq!(empty.status, StatusCode::NOT_FOUND);
    assert_eq!(empty.code(), "not_found");

    // A first command that is not the vault's own creation.
    let res = api
        .push(
            &token,
            unknown,
            vec![envelope(unknown, "alice", wallet("Cash", 0))],
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.code(), "not_found");

    // A `create_vault` whose command id is not the vault id: the envelope is
    // addressed to a vault it does not mint, so it creates nothing.
    let mut stray = CommandEnvelope::create_vault("alice", "Main", Currency::Eur);
    stray.vault_id = unknown;
    let res = api.push(&token, unknown, vec![stray]).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.code(), "not_found");

    assert!(
        api.get("/vaults", &token)
            .await
            .json::<Vec<VaultSummary>>()
            .is_empty()
    );
}

#[tokio::test]
async fn creating_a_vault_that_already_belongs_to_someone_else_is_not_found() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let stranger = api.register("mallory").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;

    // Mallory knows the id and tries to claim it with a create_vault of her
    // own: the vault exists and she is not a member, so it does not exist.
    let mut forged = CommandEnvelope::create_vault("mallory", "Mine", Currency::Eur);
    forged.id = vault;
    forged.vault_id = vault;
    let res = api.push(&stranger, vault, vec![forged]).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.code(), "not_found");
}

#[tokio::test]
async fn post_vaults_is_gone() {
    let api = Api::new();
    let token = api.register("alice").await;
    let env = CommandEnvelope::create_vault("alice", "Main", Currency::Eur);
    let res = api.post("/vaults", &token, json!(env)).await;
    assert_eq!(res.status, StatusCode::METHOD_NOT_ALLOWED);
}

// ---------------------------------------------------------------------------
// Push
// ---------------------------------------------------------------------------

#[tokio::test]
async fn push_applies_a_batch_in_order() {
    let api = Api::new();
    let token = api.register("alice").await;
    let vault = api.create_vault(&token, "alice", "Main").await;

    let commands = vec![
        envelope(vault, "alice", wallet("Cash", 0)),
        envelope(vault, "alice", income(5_000, None)),
    ];
    let res = api.push(&token, vault, commands).await;
    assert_eq!(res.status, StatusCode::OK);
    let response: PushResponse = res.json();
    assert_eq!(applied(&response.results[0]), 2);
    assert_eq!(applied(&response.results[1]), 3);
    assert_eq!(response.last_seq, 3);
}

#[tokio::test]
async fn push_is_idempotent() {
    let api = Api::new();
    let token = api.register("alice").await;
    let vault = api.create_vault(&token, "alice", "Main").await;
    let commands = vec![
        envelope(vault, "alice", wallet("Cash", 0)),
        envelope(vault, "alice", income(5_000, None)),
    ];

    let first: PushResponse = api.push(&token, vault, commands.clone()).await.json();
    let again: PushResponse = api.push(&token, vault, commands).await.json();
    assert_eq!(first.results, again.results);
    assert_eq!(again.last_seq, 3);
}

#[tokio::test]
async fn a_rejected_command_does_not_stop_the_batch() {
    let api = Api::new();
    let token = api.register("alice").await;
    let vault = api.create_vault(&token, "alice", "Main").await;

    let setup = vec![
        envelope(vault, "alice", wallet("Cash", 10_000)),
        envelope(vault, "alice", empty_flow("Vacanze")),
    ];
    let done: PushResponse = api.push(&token, vault, setup).await.json();
    let flow = match done.results[1].outcome {
        PushOutcome::Applied { result_id, .. } => result_id.expect("flow id"),
        PushOutcome::Rejected { .. } => panic!("flow not created"),
    };

    let batch = vec![
        envelope(vault, "alice", expense(1_000, Some(flow))),
        envelope(vault, "alice", income(2_000, Some(flow))),
    ];
    let response: PushResponse = api.push(&token, vault, batch).await.json();
    let (code, message) = rejection(&response.results[0]);
    assert_eq!(code, "insufficient_funds");
    assert!(message.contains("Vacanze"), "{message}");
    // The rejection never took a seq: the income lands right after the flow.
    assert_eq!(applied(&response.results[1]), 4);
    assert_eq!(response.last_seq, 4);
}

#[tokio::test]
async fn push_refuses_a_command_addressed_to_another_vault() {
    let api = Api::new();
    let token = api.register("alice").await;
    let vault = api.create_vault(&token, "alice", "Main").await;
    let other = api.create_vault(&token, "alice", "Other").await;

    let commands = vec![envelope(other, "alice", wallet("Cash", 0))];
    let res = api.push(&token, vault, commands).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");

    // Nothing of the batch was applied.
    let pull: PullResponse = api
        .get(&format!("/vaults/{vault}/pull"), &token)
        .await
        .json();
    assert_eq!(pull.last_seq, 1);
}

#[tokio::test]
async fn push_refuses_another_author() {
    let api = Api::new();
    let token = api.register("alice").await;
    let vault = api.create_vault(&token, "alice", "Main").await;
    let commands = vec![envelope(vault, "bob", wallet("Cash", 0))];
    let res = api.push(&token, vault, commands).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "author_mismatch");
}

#[tokio::test]
async fn a_viewer_cannot_push() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let viewer = api.register("bob").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    let res = api
        .put(
            &format!("/vaults/{vault}/members"),
            &owner,
            json!({ "username": "bob", "role": "viewer" }),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    let commands = vec![envelope(vault, "bob", wallet("Cash", 0))];
    let res = api.push(&viewer, vault, commands).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "forbidden");
}

#[tokio::test]
async fn an_editor_can_push() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let editor = api.register("bob").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    api.put(
        &format!("/vaults/{vault}/members"),
        &owner,
        json!({ "username": "bob", "role": "editor" }),
    )
    .await;

    let commands = vec![envelope(vault, "bob", wallet("Cash", 0))];
    let response: PushResponse = api.push(&editor, vault, commands).await.json();
    assert_eq!(applied(&response.results[0]), 2);
}

#[tokio::test]
async fn a_stranger_gets_a_blind_404() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let stranger = api.register("mallory").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;

    let commands = vec![envelope(vault, "mallory", wallet("Cash", 0))];
    let push = api.push(&stranger, vault, commands).await;
    assert_eq!(push.status, StatusCode::NOT_FOUND);
    assert_eq!(push.code(), "not_found");

    let pull = api.get(&format!("/vaults/{vault}/pull"), &stranger).await;
    assert_eq!(pull.status, StatusCode::NOT_FOUND);

    let members = api
        .get(&format!("/vaults/{vault}/members"), &stranger)
        .await;
    assert_eq!(members.status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Persons and owners
// ---------------------------------------------------------------------------

/// An expense on the only wallet for `person`.
fn expense_for(amount: i64, person: &str) -> Command {
    Command::Expense(Entry {
        person: Some(person.to_string()),
        ..entry(amount, None)
    })
}

/// alice's vault "Main" with a wallet, elisa as an editor and carol as a
/// viewer; mallory has an account but no membership. Returns alice's token
/// and the vault.
async fn household(api: &Api) -> (String, Uuid) {
    let alice = api.register("alice").await;
    for name in ["elisa", "carol", "mallory"] {
        api.register(name).await;
    }
    let vault = api.create_vault(&alice, "alice", "Main").await;
    for (username, role) in [("elisa", "editor"), ("carol", "viewer")] {
        let res = api
            .put(
                &format!("/vaults/{vault}/members"),
                &alice,
                json!({ "username": username, "role": role }),
            )
            .await;
        assert_eq!(res.status, StatusCode::NO_CONTENT, "{:?}", res.body);
    }
    let setup = vec![envelope(vault, "alice", wallet("Cash", 100_000))];
    let response: PushResponse = api.push(&alice, vault, setup).await.json();
    applied(&response.results[0]);
    (alice, vault)
}

/// The `not_a_member` refusal of a command that names `name`, with the name
/// in `detail` too.
fn assert_not_a_member(result: &PushResult, name: &str) {
    assert_eq!(
        rejection(result),
        (
            "not_a_member".to_string(),
            format!("{name} is not a member of this vault")
        )
    );
    match &result.outcome {
        PushOutcome::Rejected { detail, .. } => assert_eq!(detail.as_deref(), Some(name)),
        PushOutcome::Applied { .. } => unreachable!("checked above"),
    }
}

#[tokio::test]
async fn a_not_a_member_refusal_names_the_person_in_a_field_of_its_own() {
    let api = Api::new();
    let (alice, vault) = household(&api).await;

    let batch = vec![
        envelope(vault, "alice", expense_for(1_000, "mallory")),
        envelope(vault, "alice", expense(2_000, None)),
    ];
    let res = api.push(&alice, vault, batch).await;
    let results = &res.body["results"];
    assert_eq!(results[0]["status"], "rejected");
    assert_eq!(results[0]["code"], "not_a_member");
    // The app reads the name here, not out of the English sentence.
    assert_eq!(results[0]["detail"], "mallory");
    // Nothing else carries the key, not even as a null, so a client from
    // before it reads the body as it always did.
    assert_eq!(results[1]["status"], "applied");
    assert!(results[1].get("detail").is_none());
}

#[tokio::test]
async fn a_person_outside_the_vault_is_refused_and_the_batch_goes_on() {
    let api = Api::new();
    let (alice, vault) = household(&api).await;

    let batch = vec![
        envelope(vault, "alice", expense_for(1_000, "mallory")),
        envelope(vault, "alice", expense(2_000, None)),
        envelope(vault, "alice", expense_for(3_000, "giorgio")),
        envelope(vault, "alice", expense_for(4_000, "alice")),
    ];
    let ids: Vec<Uuid> = batch.iter().map(|e| e.id).collect();
    let response: PushResponse = api.push(&alice, vault, batch).await.json();
    // An account that is not a member and a name nobody has are refused
    // alike, and neither takes a seq.
    assert_not_a_member(&response.results[0], "mallory");
    assert_eq!(applied(&response.results[1]), 3);
    assert_not_a_member(&response.results[2], "giorgio");
    assert_eq!(applied(&response.results[3]), 4);
    assert_eq!(response.last_seq, 4);

    // The refused commands never entered the log.
    let pull: PullResponse = api
        .get(&format!("/vaults/{vault}/pull"), &alice)
        .await
        .json();
    let logged: Vec<Uuid> = pull.commands.iter().map(|r| r.envelope.id).collect();
    assert!(!logged.contains(&ids[0]));
    assert!(!logged.contains(&ids[2]));
    assert!(logged.contains(&ids[1]) && logged.contains(&ids[3]));
}

#[tokio::test]
async fn any_member_may_be_the_person_but_only_by_the_exact_username() {
    let api = Api::new();
    let (alice, vault) = household(&api).await;

    let batch = vec![
        // An editor and a viewer: the role says who may write, not who a
        // row may be for.
        envelope(vault, "alice", expense_for(1_000, "elisa")),
        envelope(vault, "alice", expense_for(1_000, "carol")),
        // Usernames are lowercase: "Elisa" is nobody.
        envelope(vault, "alice", expense_for(1_000, "Elisa")),
        // Spaces around a name do not count, here as in the core.
        envelope(vault, "alice", expense_for(1_000, " elisa ")),
        // A blank person is the author, and names nobody.
        envelope(vault, "alice", expense_for(1_000, "  ")),
    ];
    let response: PushResponse = api.push(&alice, vault, batch).await.json();
    applied(&response.results[0]);
    applied(&response.results[1]);
    assert_not_a_member(&response.results[2], "Elisa");
    applied(&response.results[3]);
    applied(&response.results[4]);
}

#[tokio::test]
async fn a_replayed_push_keeps_its_seq_after_the_person_left() {
    let api = Api::new();
    let (alice, vault) = household(&api).await;
    let batch = vec![envelope(vault, "alice", expense_for(1_000, "elisa"))];
    let first: PushResponse = api.push(&alice, vault, batch.clone()).await.json();
    applied(&first.results[0]);

    // elisa leaves before the client hears back, and the client retries.
    let res = api
        .delete(&format!("/vaults/{vault}/members/elisa"), &alice)
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{:?}", res.body);
    let again: PushResponse = api.push(&alice, vault, batch).await.json();
    assert_eq!(again, first, "the command is in the log: same answer");

    // A new command naming her is refused now.
    let late = vec![envelope(vault, "alice", expense_for(1_000, "elisa"))];
    let response: PushResponse = api.push(&alice, vault, late).await.json();
    assert_not_a_member(&response.results[0], "elisa");
}

#[tokio::test]
async fn owners_patches_and_executions_name_members_too() {
    let api = Api::new();
    let (alice, vault) = household(&api).await;

    let template = |owner: &str| Command::CreateRecurring {
        transaction_kind: sparagne_core::TransactionKind::Expense,
        amount: 78_000,
        wallet_id: None,
        flow_id: None,
        category: Some("Casa".to_string()),
        note: Some("mutuo".to_string()),
        schedule: sparagne_core::Schedule {
            frequency: sparagne_core::Frequency::Monthly { day: 1 },
            interval: 1,
            start_date: chrono::NaiveDate::from_ymd_opt(2023, 11, 1).expect("date"),
            end_date: None,
        },
        owner: Some(owner.to_string()),
    };
    let refused_template = envelope(vault, "alice", template("mallory"));
    let mutuo = envelope(vault, "alice", template("elisa"));
    let mutuo_id = mutuo.id;
    let response: PushResponse = api
        .push(&alice, vault, vec![refused_template, mutuo])
        .await
        .json();
    assert_not_a_member(&response.results[0], "mallory");
    applied(&response.results[1]);

    let owner = |owner: &str| Command::UpdateRecurring {
        recurring_id: mutuo_id,
        patch: sparagne_core::RecurringPatch {
            owner: Some(owner.to_string()),
            ..Default::default()
        },
    };
    let execute = |person: &str, day: u32| Command::ExecuteRecurring {
        recurring_id: mutuo_id,
        period_date: chrono::NaiveDate::from_ymd_opt(2023, day, 1).expect("date"),
        occurred_at: at(T0),
        person: Some(person.to_string()),
    };
    let expense = envelope(vault, "alice", expense(1_000, None));
    let expense_id = expense.id;
    let person = |person: &str| Command::UpdateTransaction {
        transaction_id: expense_id,
        patch: sparagne_core::TransactionPatch {
            person: Some(person.to_string()),
            ..Default::default()
        },
    };
    let batch = vec![
        envelope(vault, "alice", owner("mallory")),
        envelope(vault, "alice", owner("")),
        envelope(vault, "alice", execute("mallory", 11)),
        envelope(vault, "alice", execute("elisa", 11)),
        expense,
        envelope(vault, "alice", person("mallory")),
        envelope(vault, "alice", person("carol")),
    ];
    let response: PushResponse = api.push(&alice, vault, batch).await.json();
    assert_not_a_member(&response.results[0], "mallory");
    applied(&response.results[1]);
    assert_not_a_member(&response.results[2], "mallory");
    applied(&response.results[3]);
    applied(&response.results[4]);
    assert_not_a_member(&response.results[5], "mallory");
    applied(&response.results[6]);
}

// ---------------------------------------------------------------------------
// Pull
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pull_walks_the_log() {
    let api = Api::new();
    let token = api.register("alice").await;
    let vault = api.create_vault(&token, "alice", "Main").await;
    api.push(
        &token,
        vault,
        vec![
            envelope(vault, "alice", wallet("Cash", 0)),
            envelope(vault, "alice", income(5_000, None)),
        ],
    )
    .await;

    let all: PullResponse = api
        .get(&format!("/vaults/{vault}/pull?since=0"), &token)
        .await
        .json();
    assert_eq!(all.commands.len(), 3);
    assert_eq!(all.commands[0].seq, 1);
    assert_eq!(all.commands[0].envelope.vault_id, vault);
    assert_eq!(all.last_seq, 3);
    assert!(all.commands.iter().all(|r| r.created_at > 0));

    let tail: PullResponse = api
        .get(&format!("/vaults/{vault}/pull?since=2"), &token)
        .await
        .json();
    assert_eq!(tail.commands.len(), 1);
    assert_eq!(tail.commands[0].seq, 3);
    assert_eq!(tail.last_seq, 3);

    let one: PullResponse = api
        .get(&format!("/vaults/{vault}/pull?since=0&limit=1"), &token)
        .await
        .json();
    assert_eq!(one.commands.len(), 1);
    assert_eq!(one.commands[0].seq, 1);
    assert_eq!(one.last_seq, 3, "last_seq tells the client there is more");

    let default: PullResponse = api
        .get(&format!("/vaults/{vault}/pull"), &token)
        .await
        .json();
    assert_eq!(default.commands.len(), 3);
}

#[tokio::test]
async fn pull_refuses_a_malformed_query() {
    let api = Api::new();
    let token = api.register("alice").await;
    let vault = api.create_vault(&token, "alice", "Main").await;
    let res = api
        .get(&format!("/vaults/{vault}/pull?since=soon"), &token)
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
}

#[tokio::test]
async fn a_viewer_can_pull() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let viewer = api.register("bob").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    api.put(
        &format!("/vaults/{vault}/members"),
        &owner,
        json!({ "username": "bob", "role": "viewer" }),
    )
    .await;

    let pull: PullResponse = api
        .get(&format!("/vaults/{vault}/pull"), &viewer)
        .await
        .json();
    assert_eq!(pull.commands.len(), 1);
    assert_eq!(pull.last_seq, 1);
}

// ---------------------------------------------------------------------------
// Members
// ---------------------------------------------------------------------------

#[tokio::test]
async fn members_can_be_added_changed_and_removed() {
    let api = Api::new();
    let owner = api.register("alice").await;
    api.register("bob").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    let uri = format!("/vaults/{vault}/members");

    let set = api
        .put(&uri, &owner, json!({ "username": "bob", "role": "editor" }))
        .await;
    assert_eq!(set.status, StatusCode::NO_CONTENT);
    let members: Vec<MemberEntry> = api.get(&uri, &owner).await.json();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].role.as_str(), "owner");
    assert_eq!(members[1].username, "bob");
    assert_eq!(members[1].role.as_str(), "editor");

    let downgrade = api
        .put(&uri, &owner, json!({ "username": "bob", "role": "viewer" }))
        .await;
    assert_eq!(downgrade.status, StatusCode::NO_CONTENT);
    let members: Vec<MemberEntry> = api.get(&uri, &owner).await.json();
    assert_eq!(members[1].role.as_str(), "viewer");

    let removed = api.delete(&format!("{uri}/bob"), &owner).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let members: Vec<MemberEntry> = api.get(&uri, &owner).await.json();
    assert_eq!(members.len(), 1);
}

#[tokio::test]
async fn the_owner_role_cannot_be_granted() {
    let api = Api::new();
    let owner = api.register("alice").await;
    api.register("bob").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    let res = api
        .put(
            &format!("/vaults/{vault}/members"),
            &owner,
            json!({ "username": "bob", "role": "owner" }),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
}

#[tokio::test]
async fn the_owner_cannot_be_changed_or_removed() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    let uri = format!("/vaults/{vault}/members");

    let demote = api
        .put(
            &uri,
            &owner,
            json!({ "username": "alice", "role": "viewer" }),
        )
        .await;
    assert_eq!(demote.status, StatusCode::FORBIDDEN);
    assert_eq!(demote.code(), "forbidden");

    let removed = api.delete(&format!("{uri}/alice"), &owner).await;
    assert_eq!(removed.status, StatusCode::FORBIDDEN);
    assert_eq!(removed.code(), "forbidden");
}

#[tokio::test]
async fn setting_an_unknown_user_is_not_found() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    let uri = format!("/vaults/{vault}/members");

    let set = api
        .put(
            &uri,
            &owner,
            json!({ "username": "nobody", "role": "editor" }),
        )
        .await;
    assert_eq!(set.status, StatusCode::NOT_FOUND);
    assert_eq!(set.code(), "not_found");

    let removed = api.delete(&format!("{uri}/nobody"), &owner).await;
    assert_eq!(removed.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn removing_a_user_who_is_not_a_member_is_not_found() {
    let api = Api::new();
    let owner = api.register("alice").await;
    api.register("bob").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    let res = api
        .delete(&format!("/vaults/{vault}/members/bob"), &owner)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_the_owner_manages_members() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let editor = api.register("bob").await;
    api.register("carol").await;
    let vault = api.create_vault(&owner, "alice", "Main").await;
    let uri = format!("/vaults/{vault}/members");
    api.put(&uri, &owner, json!({ "username": "bob", "role": "editor" }))
        .await;

    let set = api
        .put(
            &uri,
            &editor,
            json!({ "username": "carol", "role": "viewer" }),
        )
        .await;
    assert_eq!(set.status, StatusCode::FORBIDDEN);
    assert_eq!(set.code(), "forbidden");

    let removed = api.delete(&format!("{uri}/carol"), &editor).await;
    assert_eq!(removed.status, StatusCode::FORBIDDEN);

    // A member can still read the list.
    let members: Vec<MemberEntry> = api.get(&uri, &editor).await.json();
    assert_eq!(members.len(), 2);
}

// ---------------------------------------------------------------------------
// Vault listing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vaults_lists_owned_and_shared_vaults() {
    let api = Api::new();
    let owner = api.register("alice").await;
    let member = api.register("bob").await;
    let main = api.create_vault(&owner, "alice", "Main").await;
    let other = api.create_vault(&owner, "alice", "Zurigo").await;
    let bobs = api.create_vault(&member, "bob", "Bob").await;
    api.push(
        &owner,
        main,
        vec![envelope(main, "alice", wallet("Cash", 0))],
    )
    .await;
    api.put(
        &format!("/vaults/{main}/members"),
        &owner,
        json!({ "username": "bob", "role": "editor" }),
    )
    .await;

    let alice: Vec<VaultSummary> = api.get("/vaults", &owner).await.json();
    assert_eq!(alice.len(), 2);
    assert_eq!(alice[0].id, main);
    assert_eq!(alice[0].name, "Main");
    assert_eq!(alice[0].owner, "alice");
    assert_eq!(alice[0].role.as_str(), "owner");
    assert_eq!(alice[0].currency, Currency::Eur);
    assert_eq!(alice[0].last_seq, 2);
    assert_eq!(alice[1].id, other);

    let bob: Vec<VaultSummary> = api.get("/vaults", &member).await.json();
    assert_eq!(bob.len(), 2);
    let shared = bob.iter().find(|v| v.id == main).expect("shared vault");
    assert_eq!(shared.role.as_str(), "editor");
    assert_eq!(shared.owner, "alice");
    let own = bob.iter().find(|v| v.id == bobs).expect("own vault");
    assert_eq!(own.role.as_str(), "owner");
    assert_eq!(own.last_seq, 1);
}

#[tokio::test]
async fn vaults_lists_duplicate_names_in_stable_order() {
    async fn listed(api: &Api, token: &str) -> Vec<Uuid> {
        api.get("/vaults", token)
            .await
            .json::<Vec<VaultSummary>>()
            .into_iter()
            .map(|v| v.id)
            .collect()
    }

    let api = Api::new();
    let token = api.register("alice").await;
    let casa = api.create_vault(&token, "alice", "Casa").await;
    let banca = api.create_vault(&token, "alice", "Banca").await;
    let twin = api.create_vault(&token, "alice", "casa").await;

    // By name whatever the case, then by id: a v7 uuid, so by creation.
    assert_eq!(listed(&api, &token).await, vec![banca, casa, twin]);
    // A rename puts the vault among its new namesakes by that same order.
    let res = api
        .push(
            &token,
            banca,
            vec![envelope(
                banca,
                "alice",
                Command::RenameVault {
                    name: "CASA".to_string(),
                },
            )],
        )
        .await;
    applied(&res.json::<PushResponse>().results[0]);
    assert_eq!(listed(&api, &token).await, vec![casa, banca, twin]);
}

#[tokio::test]
async fn vaults_requires_a_token() {
    let api = Api::new();
    let res = api.call(Method::GET, "/vaults", None, None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}
