use api_types::{
    transaction::{TransactionList, TransactionListResponse},
    vault::{Vault, VaultSnapshot},
};
use reqwest::Url;

use serde::Deserialize;

use crate::error::{AppError, Result};

#[derive(Debug)]
pub enum ClientError {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict(String),
    Validation(String),
    Server(String),
    Transport(reqwest::Error),
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone)]
pub struct Client {
    base_url: Url,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|err| AppError::Terminal(format!("invalid base_url: {err}")))?;
        Ok(Self {
            base_url,
            http: reqwest::Client::new(),
        })
    }

    pub async fn vault_get(
        &self,
        username: &str,
        password: &str,
        vault_name: &str,
    ) -> std::result::Result<Vault, ClientError> {
        let endpoint = self
            .base_url
            .join("vault/get")
            .map_err(|err| ClientError::Server(format!("invalid base_url: {err}")))?;

        let payload = Vault {
            id: None,
            name: Some(vault_name.to_string()),
            currency: None,
        };

        let res = self
            .http
            .post(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        if res.status().is_success() {
            return res.json::<Vault>().await.map_err(ClientError::Transport);
        }

        let status = res.status();
        let body = res
            .json::<ErrorResponse>()
            .await
            .map(|err| err.error)
            .unwrap_or_else(|_| "unknown error".to_string());

        let err = match status.as_u16() {
            401 => ClientError::Unauthorized,
            403 => ClientError::Forbidden,
            404 => ClientError::NotFound,
            409 => ClientError::Conflict(body),
            422 => ClientError::Validation(body),
            _ => ClientError::Server(body),
        };
        Err(err)
    }

    pub async fn vault_snapshot(
        &self,
        username: &str,
        password: &str,
        vault_name: &str,
    ) -> std::result::Result<VaultSnapshot, ClientError> {
        let endpoint = self
            .base_url
            .join("vault/snapshot")
            .map_err(|err| ClientError::Server(format!("invalid base_url: {err}")))?;

        let payload = Vault {
            id: None,
            name: Some(vault_name.to_string()),
            currency: None,
        };

        let res = self
            .http
            .post(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        if res.status().is_success() {
            return res
                .json::<VaultSnapshot>()
                .await
                .map_err(ClientError::Transport);
        }

        let status = res.status();
        let body = res
            .json::<ErrorResponse>()
            .await
            .map(|err| err.error)
            .unwrap_or_else(|_| "unknown error".to_string());

        let err = match status.as_u16() {
            401 => ClientError::Unauthorized,
            403 => ClientError::Forbidden,
            404 => ClientError::NotFound,
            409 => ClientError::Conflict(body),
            422 => ClientError::Validation(body),
            _ => ClientError::Server(body),
        };
        Err(err)
    }

    pub async fn transactions_list(
        &self,
        username: &str,
        password: &str,
        payload: TransactionList,
    ) -> std::result::Result<TransactionListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("transactions")
            .map_err(|err| ClientError::Server(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .post(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        if res.status().is_success() {
            return res
                .json::<TransactionListResponse>()
                .await
                .map_err(ClientError::Transport);
        }

        let status = res.status();
        let body = res
            .json::<ErrorResponse>()
            .await
            .map(|err| err.error)
            .unwrap_or_else(|_| "unknown error".to_string());

        let err = match status.as_u16() {
            401 => ClientError::Unauthorized,
            403 => ClientError::Forbidden,
            404 => ClientError::NotFound,
            409 => ClientError::Conflict(body),
            422 => ClientError::Validation(body),
            _ => ClientError::Server(body),
        };
        Err(err)
    }
}
