//! HTTP client for the Sparagne API
//!
//! Domain-focused modules:
//! - [`vaults`]: Vault CRUD operations
//! - [`wallets`]: Wallet CRUD operations
//! - [`flows`]: Flow CRUD operations
//! - [`transactions`]: Transaction operations (list, create, update, void)
//! - [`categories`]: Category CRUD operations
//! - [`members`]: Membership operations
//! - [`stats`]: Statistics endpoints

mod categories;
mod flows;
mod members;
mod stats;
mod transactions;
mod vaults;
mod wallets;

use api_types::error::{ErrorCode, ErrorEnvelope, ErrorPayload};
use reqwest::Url;
use serde::de::DeserializeOwned;

use crate::error::{AppError, Result};

/// HTTP client errors
#[derive(Debug)]
pub enum ClientError {
    Unauthorized,
    Forbidden(ErrorPayload),
    NotFound(ErrorPayload),
    Conflict(ErrorPayload),
    Validation(ErrorPayload),
    BadRequest(ErrorPayload),
    Server(ErrorPayload),
    Client(String),
    Transport(reqwest::Error),
}

/// HTTP client for the Sparagne API
#[derive(Debug, Clone)]
pub struct Client {
    base_url: Url,
    http: reqwest::Client,
    credentials: Option<(String, String)>,
}

impl Client {
    /// Create a new client with the given base URL
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|err| AppError::Terminal(format!("invalid base_url: {err}")))?;
        Ok(Self {
            base_url,
            http: reqwest::Client::new(),
            credentials: None,
        })
    }

    /// Store credentials for subsequent requests
    pub(crate) fn set_credentials(&mut self, username: String, password: String) {
        self.credentials = Some((username, password));
    }

    /// Apply stored basic-auth credentials to a request builder
    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some((username, password)) = &self.credentials {
            req.basic_auth(username, Some(password))
        } else {
            req
        }
    }
}

// --- Internal helper functions for response handling ---

async fn handle_json<T: DeserializeOwned>(
    res: reqwest::Response,
) -> std::result::Result<T, ClientError> {
    if res.status().is_success() {
        return res.json::<T>().await.map_err(ClientError::Transport);
    }

    let status = res.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ClientError::Unauthorized);
    }

    let payload = res
        .json::<ErrorEnvelope>()
        .await
        .map(|err| err.error)
        .unwrap_or_else(|_| ErrorPayload {
            code: ErrorCode::Unknown,
            message: "unknown error".to_string(),
            details: None,
        });

    Err(map_error(status.as_u16(), payload))
}

async fn handle_empty(res: reqwest::Response) -> std::result::Result<(), ClientError> {
    if res.status().is_success() {
        return Ok(());
    }
    let status = res.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ClientError::Unauthorized);
    }
    let payload = res
        .json::<ErrorEnvelope>()
        .await
        .map(|err| err.error)
        .unwrap_or_else(|_| ErrorPayload {
            code: ErrorCode::Unknown,
            message: "unknown error".to_string(),
            details: None,
        });
    Err(map_error(status.as_u16(), payload))
}

fn map_error(status: u16, payload: ErrorPayload) -> ClientError {
    match status {
        403 => ClientError::Forbidden(payload),
        404 => ClientError::NotFound(payload),
        409 => ClientError::Conflict(payload),
        422 => ClientError::Validation(payload),
        400 => ClientError::BadRequest(payload),
        _ => ClientError::Server(payload),
    }
}
