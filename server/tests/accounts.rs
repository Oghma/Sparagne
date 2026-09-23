//! Accounts on the server: case-insensitive usernames, login failures and
//! their limits, changing the password, leaving a vault, and the `user`
//! commands of the admin CLI (`docs/v2/SYNC.md` §3, `docs/v2/DEPLOY.md`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, Client, PASSWORD, Res};
use serde_json::json;
use sparagne_core::sync::{MemberEntry, TokenResponse};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `POST /auth/register` with any credentials, whatever the outcome.
async fn register_as(api: &Api, username: &str, password: &str) -> Res {
    api.call(
        Method::POST,
        "/auth/register",
        None,
        Some(json!({ "username": username, "password": password })),
    )
    .await
}

// ---------------------------------------------------------------------------
// Usernames
// ---------------------------------------------------------------------------

#[tokio::test]
async fn usernames_are_trimmed_and_lowercased() {
    let api = Api::new();
    api.register("alice").await;

    let res = api.login("Alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.json::<TokenResponse>().username, "alice");
    let res = api.login("  ALICE ", PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);

    // Registering is normalized the same way: " Bob" is bob, and "BOB" is
    // the same account again.
    let bob = register_as(&api, " Bob", PASSWORD).await;
    assert_eq!(bob.status, StatusCode::CREATED, "{:?}", bob.body);
    assert_eq!(bob.json::<TokenResponse>().username, "bob");
    let again = register_as(&api, "BOB", PASSWORD).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.code(), "already_exists");
}

#[tokio::test]
async fn members_are_named_case_insensitively() {
    let api = Api::new();
    let mut alice = Client::register(&api, "alice").await;
    api.register("bob").await;
    let vault = alice.create_vault("Casa");
    alice.sync(&api, vault).await;
    let uri = format!("/vaults/{vault}/members");

    let set = api
        .put(
            &uri,
            &alice.token,
            json!({ "username": "Bob", "role": "viewer" }),
        )
        .await;
    assert_eq!(set.status, StatusCode::NO_CONTENT, "{:?}", set.body);
    let members: Vec<MemberEntry> = api.get(&uri, &alice.token).await.json();
    assert_eq!(members.len(), 2);
    assert_eq!(members[1].username, "bob");

    let removed = api.delete(&format!("{uri}/BOB"), &alice.token).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{:?}", removed.body);
    let members: Vec<MemberEntry> = api.get(&uri, &alice.token).await.json();
    assert_eq!(members.len(), 1);
}
