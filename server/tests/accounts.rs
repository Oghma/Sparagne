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
async fn an_unknown_user_gets_the_same_401_as_a_wrong_password() {
    let api = Api::new();
    api.register("alice").await;

    let wrong = api.login("alice", "not my password").await;
    let unknown = api.login("nobody", "not my password").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.body, unknown.body);

    // The unknown user is checked against a real hash made with the same
    // parameters as the accounts', so both answers cost one verification.
    let dummy = api.state.dummy_hash();
    assert!(dummy.starts_with("$argon2id$"), "{dummy}");
    let (_, stored) = api
        .state
        .db()
        .user_with_hash("alice")
        .unwrap()
        .expect("alice exists");
    let params = |phc: &str| phc.split('$').nth(3).map(str::to_string);
    assert_eq!(params(dummy), params(&stored));
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

// ---------------------------------------------------------------------------
// Changing the password
// ---------------------------------------------------------------------------

const NEW_PASSWORD: &str = "a much longer passphrase";

fn change(current: &str, new: &str) -> serde_json::Value {
    json!({ "current_password": current, "new_password": new })
}

#[tokio::test]
async fn changing_the_password_needs_the_current_one() {
    let api = Api::new();
    let token = api.register("alice").await;

    let res = api
        .post(
            "/auth/password",
            &token,
            change("not my password", NEW_PASSWORD),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.code(), "unauthorized");
    assert_eq!(api.login("alice", PASSWORD).await.status, StatusCode::OK);
    assert_eq!(
        api.login("alice", NEW_PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );

    // And a token: without one there is nobody to change the password of.
    let anonymous = api
        .call(
            Method::POST,
            "/auth/password",
            None,
            Some(change(PASSWORD, NEW_PASSWORD)),
        )
        .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn changing_the_password_validates_the_new_one() {
    let api = Api::new();
    let token = api.register("alice").await;
    let res = api
        .post("/auth/password", &token, change(PASSWORD, "short"))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "invalid_request");
    assert_eq!(api.login("alice", PASSWORD).await.status, StatusCode::OK);

    let malformed = api
        .post(
            "/auth/password",
            &token,
            json!({ "new_password": NEW_PASSWORD }),
        )
        .await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn changing_the_password_revokes_the_other_tokens_and_keeps_this_one() {
    let api = Api::new();
    let laptop = api.register("alice").await;
    let phone = api
        .login("alice", PASSWORD)
        .await
        .json::<TokenResponse>()
        .token;
    let bob = api.register("bob").await;

    let res = api
        .post("/auth/password", &laptop, change(PASSWORD, NEW_PASSWORD))
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{:?}", res.body);

    assert_eq!(api.get("/me", &laptop).await.status, StatusCode::OK);
    let revoked = api.get("/me", &phone).await;
    assert_eq!(revoked.status, StatusCode::UNAUTHORIZED);
    assert_eq!(revoked.code(), "unauthorized");
    // Somebody else's session is not touched.
    assert_eq!(api.get("/me", &bob).await.status, StatusCode::OK);

    assert_eq!(
        api.login("alice", PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.login("alice", NEW_PASSWORD).await.status,
        StatusCode::OK
    );
}

// ---------------------------------------------------------------------------
// Leaving a vault
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_editor_or_a_viewer_can_leave_a_vault() {
    let api = Api::new();
    let (alice, bob, vault, _) = common::basics(&api).await;
    let carol = api.register("carol").await;
    let uri = format!("/vaults/{vault}/members");
    let set = api
        .put(
            &uri,
            &alice.token,
            json!({ "username": "carol", "role": "viewer" }),
        )
        .await;
    assert_eq!(set.status, StatusCode::NO_CONTENT);

    // bob (editor) leaves, carol (viewer) leaves by a differently spelled name.
    let left = api.delete(&format!("{uri}/bob"), &bob.token).await;
    assert_eq!(left.status, StatusCode::NO_CONTENT, "{:?}", left.body);
    let left = api.delete(&format!("{uri}/Carol"), &carol).await;
    assert_eq!(left.status, StatusCode::NO_CONTENT, "{:?}", left.body);

    let members: Vec<MemberEntry> = api.get(&uri, &alice.token).await.json();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].username, "alice");
    // The vault is gone for bob: blind 404s, and no longer listed.
    let pull = api
        .get(&format!("/vaults/{vault}/pull?since=0"), &bob.token)
        .await;
    assert_eq!(pull.status, StatusCode::NOT_FOUND);
    let listed: Vec<serde_json::Value> = api.get("/vaults", &bob.token).await.json();
    assert!(listed.is_empty());
    // Leaving twice is leaving a vault one is not a member of.
    let again = api.delete(&format!("{uri}/bob"), &bob.token).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_owner_cannot_leave_a_vault() {
    let api = Api::new();
    let (alice, _bob, vault, _) = common::basics(&api).await;
    let res = api
        .delete(&format!("/vaults/{vault}/members/alice"), &alice.token)
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "forbidden");
}

#[tokio::test]
async fn a_member_cannot_remove_another_member() {
    let api = Api::new();
    let (alice, bob, vault, _) = common::basics(&api).await;
    api.register("carol").await;
    let uri = format!("/vaults/{vault}/members");
    api.put(
        &uri,
        &alice.token,
        json!({ "username": "carol", "role": "viewer" }),
    )
    .await;

    for target in ["carol", "alice"] {
        let res = api.delete(&format!("{uri}/{target}"), &bob.token).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{target}");
        assert_eq!(res.code(), "forbidden");
    }
    let members: Vec<MemberEntry> = api.get(&uri, &alice.token).await.json();
    assert_eq!(members.len(), 3);
}
