//! HTTP acceptance tests for every route of the API, each on a fresh
//! in-memory server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode};
use chrono::Utc;
use common::{Api, PASSWORD, T0, applied, at, entry, flow_cmd, rejection, wallet_cmd};
use serde_json::{Value, json};
use sparagne_core::{
    Command, CommandEnvelope, Currency, Entry, FlowMode,
    sync::{
        MemberEntry, PullResponse, PushOutcome, PushResponse, PushResult, TokenResponse,
        VaultSummary,
    },
};
use sparagne_server::Config;
use uuid::Uuid;

/// A fresh server where alice has registered and created "Main": the api,
/// her token and the vault.
async fn main_vault() -> (Api, String, Uuid) {
    let api = Api::new();
    let token = api.register("alice").await;
    let vault = api.create_vault(&token, "alice", "Main").await;
    (api, token, vault)
}

fn income(amount: i64, flow: Option<Uuid>) -> Command {
    Command::Income(entry(amount, None, flow, None, T0))
}

fn expense(amount: i64, flow: Option<Uuid>) -> Command {
    Command::Expense(entry(amount, None, flow, None, T0))
}

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

#[tokio::test]
async fn register_returns_a_token_that_opens_the_api() {
    let api = Api::new();
    let res = api.register_as("alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED);
    let token: TokenResponse = res.json();
    assert_eq!(token.username, "alice");
    assert!(token.expires_at > Utc::now().timestamp());
    assert!(!token.token.is_empty());

    let vaults = api.get("/vaults", &token.token).await;
    assert_eq!(vaults.status, StatusCode::OK);
    assert_eq!(vaults.body, json!([]));
}

#[tokio::test]
async fn register_can_be_disabled() {
    let api = Api::with_config(Config {
        allow_registration: false,
        ..Config::default()
    });
    let res = api.register_as("alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "registration_disabled");
}

#[tokio::test]
async fn register_refuses_a_taken_username() {
    let api = Api::new();
    api.register("alice").await;
    let res = api.register_as("alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.code(), "already_exists");
}

#[tokio::test]
async fn register_validates_the_username() {
    let api = Api::new();
    // "Alice" is no longer here: usernames are lowercased before validation.
    for username in ["ab", "with space", &"a".repeat(33)] {
        let res = api.register_as(username, PASSWORD).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{username}");
        assert_eq!(res.code(), "invalid_request");
    }
}

#[tokio::test]
async fn register_validates_the_password() {
    let api = Api::new();
    let res = api.register_as("alice", "short").await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
}

#[tokio::test]
async fn register_rejects_a_malformed_body() {
    let api = Api::new();
    let body = json!({ "username": "alice" });
    let res = api
        .call(Method::POST, "/auth/register", None, Some(body))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
}

#[tokio::test]
async fn login_returns_a_token_that_opens_the_api() {
    let api = Api::new();
    api.register("alice").await;
    let ok = api.login("alice", PASSWORD).await;
    assert_eq!(ok.status, StatusCode::OK);
    let token: TokenResponse = ok.json();
    assert_eq!(
        api.get("/vaults", &token.token).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn logout_revokes_the_token() {
    let api = Api::new();
    let token = api.register("alice").await;
    let res = api.post("/auth/logout", &token, json!({})).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    let res = api.get("/vaults", &token).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.code(), "unauthorized");
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let api = Api::with_config(Config {
        token_ttl_days: 0,
        ..Config::default()
    });
    let token: TokenResponse = api.register_as("alice", PASSWORD).await.json();
    let res = api.get("/vaults", &token.token).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.code(), "unauthorized");
}

#[tokio::test]
async fn a_missing_or_unknown_token_is_refused() {
    let api = Api::new();
    assert_eq!(
        api.call(Method::GET, "/vaults", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.get("/vaults", "made-up-token").await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn the_first_push_creates_the_vault_and_the_owner_membership() {
    let api = Api::new();
    let token = api.register("alice").await;
    let env = CommandEnvelope::create_vault("alice", "Main", Currency::Eur);
    let id = env.id;

    // The rest of the batch rides along in the same request.
    let cash = CommandEnvelope::new(id, "alice", wallet_cmd("Cash", 10_000));
    let res = api.push(&token, id, vec![env, cash]).await;
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
    let listed: Vec<VaultSummary> = api.get("/vaults", &token).await.json();
    assert_eq!(listed.len(), 1);
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
    let (api, token, first) = main_vault().await;
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
    let cash = CommandEnvelope::new(unknown, "alice", wallet_cmd("Cash", 0));
    let res = api.push(&token, unknown, vec![cash]).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.code(), "not_found");

    // A `create_vault` whose command id is not the vault id: the envelope is
    // addressed to a vault it does not mint, so it creates nothing.
    let mut stray = CommandEnvelope::create_vault("alice", "Main", Currency::Eur);
    stray.vault_id = unknown;
    let res = api.push(&token, unknown, vec![stray]).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.code(), "not_found");

    let listed: Vec<VaultSummary> = api.get("/vaults", &token).await.json();
    assert!(listed.is_empty());
}

#[tokio::test]
async fn creating_a_vault_that_already_belongs_to_someone_else_is_not_found() {
    let (api, _, vault) = main_vault().await;
    let stranger = api.register("mallory").await;

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

#[tokio::test]
async fn push_applies_a_batch_in_order() {
    let (api, token, vault) = main_vault().await;
    let commands = vec![
        CommandEnvelope::new(vault, "alice", wallet_cmd("Cash", 0)),
        CommandEnvelope::new(vault, "alice", income(5_000, None)),
    ];
    let res = api.push(&token, vault, commands).await;
    assert_eq!(res.status, StatusCode::OK);
    let response: PushResponse = res.json();
    assert_eq!(applied(&response.results[0]), 2);
    assert_eq!(applied(&response.results[1]), 3);
    assert_eq!(response.last_seq, 3);
}

#[tokio::test]
async fn a_rejected_command_does_not_stop_the_batch() {
    let (api, token, vault) = main_vault().await;
    let setup = vec![
        CommandEnvelope::new(vault, "alice", wallet_cmd("Cash", 10_000)),
        CommandEnvelope::new(
            vault,
            "alice",
            flow_cmd("Vacanze", FlowMode::Unlimited, false, 0),
        ),
    ];
    let done: PushResponse = api.push(&token, vault, setup).await.json();
    let flow = match done.results[1].outcome {
        PushOutcome::Applied { result_id, .. } => result_id.expect("flow id"),
        PushOutcome::Rejected { .. } => panic!("flow not created"),
    };

    let batch = vec![
        CommandEnvelope::new(vault, "alice", expense(1_000, Some(flow))),
        CommandEnvelope::new(vault, "alice", income(2_000, Some(flow))),
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
    let (api, token, vault) = main_vault().await;
    let other = api.create_vault(&token, "alice", "Other").await;

    let commands = vec![CommandEnvelope::new(other, "alice", wallet_cmd("Cash", 0))];
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
async fn an_editor_can_push() {
    let (api, owner, vault) = main_vault().await;
    let editor = api.register("bob").await;
    api.add_member(&owner, vault, "bob", "editor").await;

    let commands = vec![CommandEnvelope::new(vault, "bob", wallet_cmd("Cash", 0))];
    let response: PushResponse = api.push(&editor, vault, commands).await.json();
    assert_eq!(applied(&response.results[0]), 2);
}

#[tokio::test]
async fn a_stranger_gets_a_blind_404() {
    let (api, _, vault) = main_vault().await;
    let stranger = api.register("mallory").await;

    let commands = vec![CommandEnvelope::new(
        vault,
        "mallory",
        wallet_cmd("Cash", 0),
    )];
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

/// An expense on the only wallet for `person`.
fn expense_for(amount: i64, person: &str) -> Command {
    Command::Expense(Entry {
        person: Some(person.to_string()),
        ..entry(amount, None, None, None, T0)
    })
}

/// alice's "Main" with a wallet, elisa as an editor and carol as a viewer;
/// mallory has an account but no membership. Returns alice's token.
async fn household() -> (Api, String, Uuid) {
    let (api, alice, vault) = main_vault().await;
    for (username, role) in [("elisa", "editor"), ("carol", "viewer")] {
        api.register(username).await;
        api.add_member(&alice, vault, username, role).await;
    }
    api.register("mallory").await;
    let setup = vec![CommandEnvelope::new(
        vault,
        "alice",
        wallet_cmd("Cash", 100_000),
    )];
    let response: PushResponse = api.push(&alice, vault, setup).await.json();
    applied(&response.results[0]);
    (api, alice, vault)
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
    let (api, alice, vault) = household().await;
    let batch = vec![
        CommandEnvelope::new(vault, "alice", expense_for(1_000, "mallory")),
        CommandEnvelope::new(vault, "alice", expense(2_000, None)),
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
    let (api, alice, vault) = household().await;
    let batch = vec![
        CommandEnvelope::new(vault, "alice", expense_for(1_000, "mallory")),
        CommandEnvelope::new(vault, "alice", expense(2_000, None)),
        CommandEnvelope::new(vault, "alice", expense_for(3_000, "giorgio")),
        CommandEnvelope::new(vault, "alice", expense_for(4_000, "alice")),
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
    let (api, alice, vault) = household().await;
    let batch = vec![
        // An editor and a viewer: the role says who may write, not who a
        // row may be for.
        CommandEnvelope::new(vault, "alice", expense_for(1_000, "elisa")),
        CommandEnvelope::new(vault, "alice", expense_for(1_000, "carol")),
        // Usernames are lowercase: "Elisa" is nobody.
        CommandEnvelope::new(vault, "alice", expense_for(1_000, "Elisa")),
        // Spaces around a name do not count, here as in the core.
        CommandEnvelope::new(vault, "alice", expense_for(1_000, " elisa ")),
        // A blank person is the author, and names nobody.
        CommandEnvelope::new(vault, "alice", expense_for(1_000, "  ")),
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
    let (api, alice, vault) = household().await;
    let batch = vec![CommandEnvelope::new(
        vault,
        "alice",
        expense_for(1_000, "elisa"),
    )];
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
    let late = vec![CommandEnvelope::new(
        vault,
        "alice",
        expense_for(1_000, "elisa"),
    )];
    let response: PushResponse = api.push(&alice, vault, late).await.json();
    assert_not_a_member(&response.results[0], "elisa");
}

#[tokio::test]
async fn owners_patches_and_executions_name_members_too() {
    let (api, alice, vault) = household().await;

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
    let refused_template = CommandEnvelope::new(vault, "alice", template("mallory"));
    let mutuo = CommandEnvelope::new(vault, "alice", template("elisa"));
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
    let expense = CommandEnvelope::new(vault, "alice", expense(1_000, None));
    let expense_id = expense.id;
    let person = |person: &str| Command::UpdateTransaction {
        transaction_id: expense_id,
        patch: sparagne_core::TransactionPatch {
            person: Some(person.to_string()),
            ..Default::default()
        },
    };
    let batch = vec![
        CommandEnvelope::new(vault, "alice", owner("mallory")),
        CommandEnvelope::new(vault, "alice", owner("")),
        CommandEnvelope::new(vault, "alice", execute("mallory", 11)),
        CommandEnvelope::new(vault, "alice", execute("elisa", 11)),
        expense,
        CommandEnvelope::new(vault, "alice", person("mallory")),
        CommandEnvelope::new(vault, "alice", person("carol")),
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

#[tokio::test]
async fn pull_walks_the_log() {
    let (api, token, vault) = main_vault().await;
    let commands = vec![
        CommandEnvelope::new(vault, "alice", wallet_cmd("Cash", 0)),
        CommandEnvelope::new(vault, "alice", income(5_000, None)),
    ];
    api.push(&token, vault, commands).await;
    let pull = async |query: &str| {
        api.get(&format!("/vaults/{vault}/pull{query}"), &token)
            .await
    };

    let all: PullResponse = pull("?since=0").await.json();
    assert_eq!(all.commands.len(), 3);
    assert_eq!(all.commands[0].seq, 1);
    assert_eq!(all.commands[0].envelope.vault_id, vault);
    assert_eq!(all.last_seq, 3);
    assert!(all.commands.iter().all(|r| r.created_at > 0));

    // The fields the app decodes, and nothing else.
    let body = pull("?since=0").await.body;
    let keys = |value: &Value| {
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&body), ["commands", "last_seq"]);
    assert_eq!(
        keys(&body["commands"][0]),
        ["created_at", "envelope", "result_id", "seq"]
    );

    let tail: PullResponse = pull("?since=2").await.json();
    assert_eq!(tail.commands.len(), 1);
    assert_eq!(tail.commands[0].seq, 3);
    assert_eq!(tail.last_seq, 3);

    let one: PullResponse = pull("?since=0&limit=1").await.json();
    assert_eq!(one.commands.len(), 1);
    assert_eq!(one.commands[0].seq, 1);
    assert_eq!(one.last_seq, 3, "last_seq tells the client there is more");

    // Out of range values are brought back in range, not refused.
    let at_least_one: PullResponse = pull("?since=-5&limit=0").await.json();
    assert_eq!(at_least_one.commands.len(), 1);
    assert_eq!(at_least_one.commands[0].seq, 1);
    let capped: PullResponse = pull("?limit=1000000").await.json();
    assert_eq!(capped.commands.len(), 3);

    let default: PullResponse = pull("").await.json();
    assert_eq!(default.commands.len(), 3);
}

#[tokio::test]
async fn pull_refuses_a_malformed_query() {
    let (api, token, vault) = main_vault().await;
    let res = api
        .get(&format!("/vaults/{vault}/pull?since=soon"), &token)
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
}

#[tokio::test]
async fn a_viewer_can_pull() {
    let (api, owner, vault) = main_vault().await;
    let viewer = api.register("bob").await;
    api.add_member(&owner, vault, "bob", "viewer").await;

    let pull: PullResponse = api
        .get(&format!("/vaults/{vault}/pull"), &viewer)
        .await
        .json();
    assert_eq!(pull.commands.len(), 1);
    assert_eq!(pull.last_seq, 1);
}

#[tokio::test]
async fn members_can_be_added_changed_and_removed() {
    let (api, owner, vault) = main_vault().await;
    api.register("bob").await;
    let uri = format!("/vaults/{vault}/members");

    api.add_member(&owner, vault, "bob", "editor").await;
    let members: Vec<MemberEntry> = api.get(&uri, &owner).await.json();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].role.as_str(), "owner");
    assert_eq!(members[1].username, "bob");
    assert_eq!(members[1].role.as_str(), "editor");

    api.add_member(&owner, vault, "bob", "viewer").await;
    let members: Vec<MemberEntry> = api.get(&uri, &owner).await.json();
    assert_eq!(members[1].role.as_str(), "viewer");

    let removed = api.delete(&format!("{uri}/bob"), &owner).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let members: Vec<MemberEntry> = api.get(&uri, &owner).await.json();
    assert_eq!(members.len(), 1);
}

#[tokio::test]
async fn the_owner_role_cannot_be_granted() {
    let (api, owner, vault) = main_vault().await;
    api.register("bob").await;
    let res = api.set_member(&owner, vault, "bob", "owner").await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
}

#[tokio::test]
async fn the_owner_cannot_be_demoted() {
    let (api, owner, vault) = main_vault().await;
    let demote = api.set_member(&owner, vault, "alice", "viewer").await;
    assert_eq!(demote.status, StatusCode::FORBIDDEN);
    assert_eq!(demote.code(), "forbidden");
}

#[tokio::test]
async fn setting_an_unknown_user_is_not_found() {
    let (api, owner, vault) = main_vault().await;
    let set = api.set_member(&owner, vault, "nobody", "editor").await;
    assert_eq!(set.status, StatusCode::NOT_FOUND);
    assert_eq!(set.code(), "not_found");

    let removed = api
        .delete(&format!("/vaults/{vault}/members/nobody"), &owner)
        .await;
    assert_eq!(removed.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn removing_a_user_who_is_not_a_member_is_not_found() {
    let (api, owner, vault) = main_vault().await;
    api.register("bob").await;
    let res = api
        .delete(&format!("/vaults/{vault}/members/bob"), &owner)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_the_owner_manages_members() {
    let (api, owner, vault) = main_vault().await;
    let editor = api.register("bob").await;
    api.register("carol").await;
    api.add_member(&owner, vault, "bob", "editor").await;

    let set = api.set_member(&editor, vault, "carol", "viewer").await;
    assert_eq!(set.status, StatusCode::FORBIDDEN);
    assert_eq!(set.code(), "forbidden");

    // A member can still read the list.
    let members: Vec<MemberEntry> = api
        .get(&format!("/vaults/{vault}/members"), &editor)
        .await
        .json();
    assert_eq!(members.len(), 2);
}

#[tokio::test]
async fn vaults_lists_owned_and_shared_vaults() {
    let (api, owner, main) = main_vault().await;
    let member = api.register("bob").await;
    let other = api.create_vault(&owner, "alice", "Zurigo").await;
    let bobs = api.create_vault(&member, "bob", "Bob").await;
    let cash = CommandEnvelope::new(main, "alice", wallet_cmd("Cash", 0));
    api.push(&owner, main, vec![cash]).await;
    api.add_member(&owner, main, "bob", "editor").await;

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
        let vaults: Vec<VaultSummary> = api.get("/vaults", token).await.json();
        vaults.into_iter().map(|v| v.id).collect()
    }

    let api = Api::new();
    let token = api.register("alice").await;
    let casa = api.create_vault(&token, "alice", "Casa").await;
    let banca = api.create_vault(&token, "alice", "Banca").await;
    let twin = api.create_vault(&token, "alice", "casa").await;

    // By name whatever the case, then by id: a v7 uuid, so by creation.
    assert_eq!(listed(&api, &token).await, vec![banca, casa, twin]);
    // A rename puts the vault among its new namesakes by that same order.
    let rename = Command::RenameVault {
        name: "CASA".to_string(),
    };
    let res = api
        .push(
            &token,
            banca,
            vec![CommandEnvelope::new(banca, "alice", rename)],
        )
        .await;
    applied(&res.json::<PushResponse>().results[0]);
    assert_eq!(listed(&api, &token).await, vec![casa, banca, twin]);
}
