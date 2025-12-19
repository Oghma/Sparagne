use api_types::{
    transaction::{
        ExpenseNew, IncomeNew, Refund, TransactionCreated, TransactionDetailResponse,
        TransactionGet, TransactionList, TransactionListResponse, TransactionUpdate,
        TransactionVoid, TransferFlowNew, TransferWalletNew,
    },
    vault::{Vault, VaultSnapshot},
};
use reqwest::Url;

use serde::{Deserialize, de::DeserializeOwned};

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

        handle_json(res).await
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

        handle_json(res).await
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

        handle_json(res).await
    }

    pub async fn transaction_detail(
        &self,
        username: &str,
        password: &str,
        payload: TransactionGet,
    ) -> std::result::Result<TransactionDetailResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("transactions/get")
            .map_err(|err| ClientError::Server(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .post(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn transaction_void(
        &self,
        username: &str,
        password: &str,
        transaction_id: uuid::Uuid,
        payload: TransactionVoid,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("transactions/{transaction_id}/void"))
            .map_err(|err| ClientError::Server(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .post(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn transaction_update(
        &self,
        username: &str,
        password: &str,
        transaction_id: uuid::Uuid,
        payload: TransactionUpdate,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("transactions/{transaction_id}"))
            .map_err(|err| ClientError::Server(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .patch(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn income_new(
        &self,
        username: &str,
        password: &str,
        payload: IncomeNew,
    ) -> std::result::Result<TransactionCreated, ClientError> {
        post_create(self, "income", username, password, payload).await
    }

    pub async fn expense_new(
        &self,
        username: &str,
        password: &str,
        payload: ExpenseNew,
    ) -> std::result::Result<TransactionCreated, ClientError> {
        post_create(self, "expense", username, password, payload).await
    }

    pub async fn refund_new(
        &self,
        username: &str,
        password: &str,
        payload: Refund,
    ) -> std::result::Result<TransactionCreated, ClientError> {
        post_create(self, "refund", username, password, payload).await
    }

    pub async fn transfer_wallet_new(
        &self,
        username: &str,
        password: &str,
        payload: TransferWalletNew,
    ) -> std::result::Result<TransactionCreated, ClientError> {
        post_create(self, "transferWallet", username, password, payload).await
    }

    pub async fn transfer_flow_new(
        &self,
        username: &str,
        password: &str,
        payload: TransferFlowNew,
    ) -> std::result::Result<TransactionCreated, ClientError> {
        post_create(self, "transferFlow", username, password, payload).await
    }
}

async fn post_create<T: serde::Serialize>(
    client: &Client,
    path: &str,
    username: &str,
    password: &str,
    payload: T,
) -> std::result::Result<TransactionCreated, ClientError> {
    let endpoint = client
        .base_url
        .join(path)
        .map_err(|err| ClientError::Server(format!("invalid base_url: {err}")))?;

    let res = client
        .http
        .post(endpoint)
        .basic_auth(username, Some(password))
        .json(&payload)
        .send()
        .await
        .map_err(ClientError::Transport)?;

    handle_json(res).await
}

async fn handle_json<T: DeserializeOwned>(
    res: reqwest::Response,
) -> std::result::Result<T, ClientError> {
    if res.status().is_success() {
        return res.json::<T>().await.map_err(ClientError::Transport);
    }

    let status = res.status();
    let body = res
        .json::<ErrorResponse>()
        .await
        .map(|err| err.error)
        .unwrap_or_else(|_| "unknown error".to_string());

    Err(map_error(status.as_u16(), body))
}

async fn handle_empty(res: reqwest::Response) -> std::result::Result<(), ClientError> {
    if res.status().is_success() {
        return Ok(());
    }
    let status = res.status();
    let body = res
        .json::<ErrorResponse>()
        .await
        .map(|err| err.error)
        .unwrap_or_else(|_| "unknown error".to_string());
    Err(map_error(status.as_u16(), body))
}

fn map_error(status: u16, body: String) -> ClientError {
    match status {
        401 => ClientError::Unauthorized,
        403 => ClientError::Forbidden,
        404 => ClientError::NotFound,
        409 => ClientError::Conflict(body),
        422 => ClientError::Validation(body),
        _ => ClientError::Server(body),
    }
}
