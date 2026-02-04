//! Transaction operations (list, create, update, void)

use api_types::transaction::{
    ExpenseNew, IncomeNew, Refund, TransactionCreated, TransactionDetailResponse, TransactionGet,
    TransactionList, TransactionListResponse, TransactionUpdate, TransactionVoid, TransferFlowNew,
    TransferWalletNew,
};

use super::{Client, ClientError, handle_empty, handle_json};

impl Client {
    pub async fn transactions_list(
        &self,
        username: &str,
        password: &str,
        payload: TransactionList,
    ) -> std::result::Result<TransactionListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("transactions")
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

    pub async fn transaction_detail(
        &self,
        username: &str,
        password: &str,
        payload: TransactionGet,
    ) -> std::result::Result<TransactionDetailResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("transactions/get")
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
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

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
        .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

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
