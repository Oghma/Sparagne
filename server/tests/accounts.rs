//! Accounts on the server: case-insensitive usernames, login failures and
//! their limits, changing the password, leaving a vault, and the `user`
//! commands of the admin CLI (`docs/v2/SYNC.md` §3, `docs/v2/DEPLOY.md`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::net::SocketAddr;

use axum::{
    extract::ConnectInfo,
    http::{Method, StatusCode, header::RETRY_AFTER},
};
use common::{Api, Client, PASSWORD, Res};
use serde_json::json;
use sparagne_core::sync::{MemberEntry, TokenResponse};
use sparagne_server::Config;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const WRONG: &str = "not my password";

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

/// Where a request claims to come from: the TCP peer axum records when
/// serving for real, and an `X-Forwarded-For` header.
#[derive(Clone, Copy, Default)]
struct Origin {
    peer: Option<&'static str>,
    forwarded_for: Option<&'static str>,
}

impl Origin {
    fn peer(addr: &'static str) -> Self {
        Self {
            peer: Some(addr),
            forwarded_for: None,
        }
    }

    /// Through the proxy: the peer is always the proxy itself.
    fn forwarded(value: &'static str) -> Self {
        Self {
            peer: Some("10.0.0.2:41000"),
            forwarded_for: Some(value),
        }
    }
}

async fn post_from(api: &Api, from: Origin, uri: &str, body: serde_json::Value) -> Res {
    let mut request = Api::request(Method::POST, uri, None, Some(body));
    if let Some(peer) = from.peer {
        let addr: SocketAddr = peer.parse().expect("socket address");
        request.extensions_mut().insert(ConnectInfo(addr));
    }
    if let Some(value) = from.forwarded_for {
        request
            .headers_mut()
            .insert("x-forwarded-for", value.parse().expect("header value"));
    }
    api.send(request).await
}

async fn login_from(api: &Api, from: Origin, username: &str, password: &str) -> Res {
    let body = json!({ "username": username, "password": password });
    post_from(api, from, "/auth/login", body).await
}

/// A `429` with a `Retry-After` of at most `max` seconds.
fn assert_limited(res: &Res, max: u64) {
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS, "{:?}", res.body);
    assert_eq!(res.code(), "too_many_requests");
    let seconds: u64 = res
        .headers
        .get(RETRY_AFTER)
        .expect("Retry-After")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=max).contains(&seconds), "{seconds}");
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
// Rate limits
// ---------------------------------------------------------------------------

#[tokio::test]
async fn failed_logins_lock_the_username_with_retry_after() {
    let api = Api::new();
    api.register("alice").await;
    for _ in 0..5 {
        let res = api.login("alice", WRONG).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    }
    let locked = api.login("alice", WRONG).await;
    assert_limited(&locked, 900);
    // The lock is on the account, not on how its name is spelled.
    assert_limited(&api.login(" Alice", WRONG).await, 900);
}

#[tokio::test]
async fn a_locked_username_refuses_even_the_right_password() {
    let api = Api::new();
    api.register("alice").await;
    api.register("bob").await;
    for _ in 0..5 {
        api.login("alice", WRONG).await;
    }
    assert_limited(&api.login("alice", PASSWORD).await, 900);
    // Nobody else is locked out, from the same address either.
    assert_eq!(api.login("bob", PASSWORD).await.status, StatusCode::OK);
}

#[tokio::test]
async fn unknown_usernames_lock_like_real_ones() {
    let api = Api::new();
    for _ in 0..5 {
        let res = api.login("nobody", WRONG).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    }
    // Otherwise the lock would tell which accounts exist.
    assert_limited(&api.login("nobody", WRONG).await, 900);
}

#[tokio::test]
async fn a_successful_login_resets_the_failures() {
    let api = Api::new();
    api.register("alice").await;
    for round in 0..3 {
        for _ in 0..4 {
            let res = api.login("alice", WRONG).await;
            assert_eq!(res.status, StatusCode::UNAUTHORIZED, "round {round}");
        }
        let res = api.login("alice", PASSWORD).await;
        assert_eq!(res.status, StatusCode::OK, "round {round}");
    }
}

#[tokio::test]
async fn wrong_current_passwords_count_as_failed_logins() {
    let api = Api::new();
    let token = api.register("alice").await;
    for _ in 0..5 {
        let res = api
            .post("/auth/password", &token, change(WRONG, NEW_PASSWORD))
            .await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    }
    let res = api
        .post("/auth/password", &token, change(PASSWORD, NEW_PASSWORD))
        .await;
    assert_limited(&res, 900);
    assert_limited(&api.login("alice", PASSWORD).await, 900);
}

#[tokio::test]
async fn the_address_limit_uses_the_rightmost_forwarded_for_when_trusted() {
    let api = Api::with_config(Config {
        trust_forwarded_for: true,
        ip_max_failures: 3,
        ..Config::default()
    });
    api.register("alice").await;
    // Three different usernames, one client: whatever it puts in front of
    // the header, the proxy appends the real address last.
    let client = Origin::forwarded("203.0.113.7");
    let spoofing = Origin::forwarded("198.51.100.1, 203.0.113.7");
    for (i, from) in [client, spoofing, client].into_iter().enumerate() {
        let res = login_from(&api, from, &format!("user{i}"), WRONG).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    }
    assert_limited(&login_from(&api, client, "alice", PASSWORD).await, 900);
    assert_limited(&login_from(&api, spoofing, "alice", PASSWORD).await, 900);

    // Another client behind the same proxy is not affected.
    let neighbour = Origin::forwarded("203.0.113.8");
    let res = login_from(&api, neighbour, "alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
}

#[tokio::test]
async fn an_untrusted_forwarded_for_is_ignored() {
    let api = Api::with_config(Config {
        ip_max_failures: 3,
        ..Config::default()
    });
    api.register("alice").await;
    // A client rotating the header gets nowhere: the peer is the key.
    for (i, forged) in ["203.0.113.1", "203.0.113.2", "203.0.113.3"]
        .into_iter()
        .enumerate()
    {
        let from = Origin {
            peer: Some("192.0.2.10:50000"),
            forwarded_for: Some(forged),
        };
        let res = login_from(&api, from, &format!("user{i}"), WRONG).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    }
    let from = Origin {
        peer: Some("192.0.2.10:50001"),
        forwarded_for: Some("203.0.113.4"),
    };
    assert_limited(&login_from(&api, from, "alice", PASSWORD).await, 900);

    // Another peer is another client.
    let other = Origin::peer("192.0.2.11:50000");
    let res = login_from(&api, other, "alice", PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
}

#[tokio::test]
async fn registration_is_limited_per_address() {
    let api = Api::new();
    // Ten attempts an hour, valid or not.
    for i in 0..9 {
        let res = register_as(&api, &format!("user{i}"), PASSWORD).await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    }
    let invalid = register_as(&api, "x", PASSWORD).await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
    assert_limited(&register_as(&api, "user10", PASSWORD).await, 3_600);

    let elsewhere = Origin::peer("192.0.2.20:50000");
    let body = json!({ "username": "user10", "password": PASSWORD });
    let res = post_from(&api, elsewhere, "/auth/register", body).await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
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
