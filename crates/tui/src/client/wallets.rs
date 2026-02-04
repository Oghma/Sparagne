//! Wallet CRUD operations

use api_types::wallet::{WalletCreated, WalletNew, WalletUpdate};

use super::{Client, ClientError, handle_empty, handle_json};

impl Client {
    pub async fn wallet_new(
        &self,
        username: &str,
        password: &str,
        payload: WalletNew,
    ) -> std::result::Result<WalletCreated, ClientError> {
        let endpoint = self
            .base_url
            .join("wallets")
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

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

    pub async fn wallet_update(
        &self,
        username: &str,
        password: &str,
        wallet_id: uuid::Uuid,
        payload: WalletUpdate,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("wallets/{wallet_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

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
}
