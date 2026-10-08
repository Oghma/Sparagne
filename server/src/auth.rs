//! Accounts, password hashing and bearer tokens.
//!
//! Passwords are argon2id PHC strings; a token is 32 random bytes shown once
//! as base64url and stored as its sha256 digest. Neither is ever logged.
//! Every password check and every registration first goes through
//! [`crate::ratelimit`].

use std::{net::IpAddr, time::Instant};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier},
};
use axum::{
    extract::{FromRequestParts, State},
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sparagne_core::sync::{Credentials, TokenResponse};
use uuid::Uuid;

use crate::{
    client_ip::ClientIp,
    error::{ApiError, ApiResult},
    extract::Json,
    state::AppState,
};

const USERNAME_MIN: usize = 3;
const USERNAME_MAX: usize = 32;
const PASSWORD_MIN: usize = 8;
const TOKEN_BYTES: usize = 32;

/// The account behind the `Authorization: Bearer` header of the request.
#[derive(Clone, Debug)]
pub struct CurrentUser {
    pub id: Uuid,
    pub username: String,
    /// Digest of the presented token, so logout can revoke it.
    pub token_hash: String,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or_else(ApiError::unauthorized)?;
        let token_hash = digest(token);
        let lookup = token_hash.clone();
        let now = Utc::now().timestamp();
        let user = state
            .run(move |state| state.db().user_for_token(&lookup, now))
            .await?
            .ok_or_else(ApiError::unauthorized)?;
        Ok(Self {
            id: user.id,
            username: user.username,
            token_hash,
        })
    }
}

/// Body of `GET /me`.
#[derive(Clone, Debug, Serialize)]
pub struct MeResponse {
    pub username: String,
}

/// `POST /auth/register`. Every attempt counts against the caller's address
/// (`ratelimit`), valid or not.
pub async fn register(
    State(state): State<AppState>,
    ClientIp(from): ClientIp,
    Json(credentials): Json<Credentials>,
) -> ApiResult<(StatusCode, Json<TokenResponse>)> {
    if !state.config().allow_registration {
        return Err(ApiError::registration_disabled());
    }
    state.limiter().begin_register(from, Instant::now())?;
    let username = normalize_username(&credentials.username);
    validate_username(&username)?;
    validate_password(&credentials.password)?;
    let response = state
        .run(move |state| {
            let hash = hash_password(state, &credentials.password)?;
            let now = Utc::now().timestamp();
            let db = state.db();
            let user = db.insert_user(&username, &hash, now)?;
            issue_token(state, &db, &user.id, &user.username, now)
        })
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// `POST /auth/login`. The limits are checked before any hashing: a locked
/// username or address is `429` even with the right password.
pub async fn login(
    State(state): State<AppState>,
    ClientIp(from): ClientIp,
    Json(credentials): Json<Credentials>,
) -> ApiResult<Json<TokenResponse>> {
    let username = normalize_username(&credentials.username);
    begin_login(&state, &username, from)?;
    let response = state
        .run(move |state| {
            let found = state.db().user_with_hash(&username)?;
            // An unknown username still pays for one verification, against
            // the dummy hash: the 401 takes as long as a wrong password's.
            let (user, hash) = match found {
                Some((user, hash)) => (Some(user), hash),
                None => (None, state.dummy_hash().to_string()),
            };
            let verified = verify_password(state, &credentials.password, &hash);
            let Some(user) = user.filter(|_| verified) else {
                return Err(ApiError::unauthorized());
            };
            state.limiter().login_succeeded(&user.username, from);
            let now = Utc::now().timestamp();
            issue_token(state, &state.db(), &user.id, &user.username, now)
        })
        .await?;
    Ok(Json(response))
}

/// Counts a password check for `username` from `from`, or refuses it. A name
/// that fails [`validate_username`] cannot be an account, so it only counts
/// against the address and never grows the limiter by a bucket of its own.
fn begin_login(state: &AppState, username: &str, from: IpAddr) -> ApiResult<()> {
    let key = validate_username(username).is_ok().then_some(username);
    state.limiter().begin_login(key, from, Instant::now())
}

/// `POST /auth/logout`.
pub async fn logout(State(state): State<AppState>, user: CurrentUser) -> ApiResult<StatusCode> {
    state
        .run(move |state| state.db().delete_token(&user.token_hash))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Body of `POST /auth/password`. No `Debug`: it holds two passwords.
#[derive(Clone, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// `POST /auth/password`: needs the current password, validates the new one
/// like register does, and revokes every other token of the account. The
/// token that made the request stays valid. The current password is checked
/// under the same limits as a login, so a stolen token cannot guess it.
pub async fn change_password(
    State(state): State<AppState>,
    ClientIp(from): ClientIp,
    user: CurrentUser,
    Json(request): Json<ChangePasswordRequest>,
) -> ApiResult<StatusCode> {
    validate_password(&request.new_password)?;
    begin_login(&state, &user.username, from)?;
    state
        .run(move |state| {
            let Some((_, hash)) = state.db().user_with_hash(&user.username)? else {
                return Err(ApiError::unauthorized());
            };
            if !verify_password(state, &request.current_password, &hash) {
                return Err(ApiError::unauthorized());
            }
            state.limiter().login_succeeded(&user.username, from);
            let new_hash = hash_password(state, &request.new_password)?;
            let db = state.db();
            db.set_password_hash(user.id, &new_hash)?;
            db.delete_tokens_of_user(user.id, Some(&user.token_hash))?;
            Ok(())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /me`.
pub async fn me(user: CurrentUser) -> Json<MeResponse> {
    Json(MeResponse {
        username: user.username,
    })
}

fn issue_token(
    state: &AppState,
    db: &crate::db::ServerDb,
    user_id: &Uuid,
    username: &str,
    now: i64,
) -> ApiResult<TokenResponse> {
    db.purge_expired_tokens(now)?;
    let token = new_token();
    let expires_at = now.saturating_add(state.config().token_ttl_seconds());
    db.insert_token(&digest(&token), *user_id, now, expires_at)?;
    Ok(TokenResponse {
        token,
        expires_at,
        username: username.to_string(),
    })
}

/// Usernames are compared trimmed and lowercased everywhere (register, login,
/// members, the admin CLI): "Alice " and "alice" are the same account.
#[must_use]
pub fn normalize_username(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Username rules, after [`normalize_username`]: 3 to 32 characters of
/// `[a-z0-9_.-]`.
pub fn validate_username(username: &str) -> ApiResult<()> {
    let length = username.chars().count();
    if !(USERNAME_MIN..=USERNAME_MAX).contains(&length) {
        return Err(ApiError::invalid_request(format!(
            "username must be {USERNAME_MIN} to {USERNAME_MAX} characters"
        )));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
    {
        return Err(ApiError::invalid_request(
            "username may only contain a-z, 0-9, '_', '.' and '-'",
        ));
    }
    Ok(())
}

/// Passwords are at least 8 characters.
pub fn validate_password(password: &str) -> ApiResult<()> {
    if password.chars().count() < PASSWORD_MIN {
        return Err(ApiError::invalid_request(format!(
            "password must be at least {PASSWORD_MIN} characters"
        )));
    }
    Ok(())
}

fn hash_password(state: &AppState, password: &str) -> ApiResult<String> {
    hash_with(state.argon(), password)
}

/// An argon2id PHC string for `password`, salted afresh.
pub fn hash_with(argon: &Argon2<'_>, password: &str) -> ApiResult<String> {
    argon
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|err| ApiError::internal(err.to_string()))
}

fn verify_password(state: &AppState, password: &str, hash: &str) -> bool {
    state
        .argon()
        .verify_password(password.as_bytes(), hash)
        .is_ok()
}

/// 32 random bytes as base64url without padding.
fn new_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Lowercase hex sha256, the shape tokens are stored in.
fn digest(token: &str) -> String {
    let hash = Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(hash.len() * 2);
    for byte in hash {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    out
}

const fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'a' + nibble - 10) as char,
    }
}
