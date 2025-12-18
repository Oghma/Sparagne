use api_types::{
    stats::Statistic,
    transaction::{
        ExpenseNew, IncomeNew, Refund, TransactionCreated, TransactionDetailResponse,
        TransactionGet, TransactionList, TransactionListResponse, TransactionUpdate,
        TransactionVoid,
    },
    user::PairUser,
    vault::{Vault, VaultSnapshot},
};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub(crate) struct ApiClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ApiError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("{status}: {message}")]
    Server { status: StatusCode, message: String },
}

impl ApiClient {
    pub(crate) fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    async fn post_json<TReq: serde::Serialize + ?Sized, TResp: for<'de> serde::Deserialize<'de>>(
        &self,
        telegram_user_id: Option<u64>,
        path: &str,
        body: &TReq,
    ) -> Result<TResp, ApiError> {
        let mut req = self.client.post(self.url(path)).json(body);
        if let Some(id) = telegram_user_id {
            req = req.header("telegram-user-id", id.to_string());
        }

        let resp = req.send().await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp.json::<TResp>().await?);
        }

        let message = match resp.json::<ErrorBody>().await {
            Ok(err) => err.error,
            Err(_) => "server error".to_string(),
        };
        Err(ApiError::Server { status, message })
    }

    async fn post_json_unit<TReq: serde::Serialize + ?Sized>(
        &self,
        telegram_user_id: Option<u64>,
        path: &str,
        body: &TReq,
    ) -> Result<(), ApiError> {
        let mut req = self.client.post(self.url(path)).json(body);
        if let Some(id) = telegram_user_id {
            req = req.header("telegram-user-id", id.to_string());
        }

        let resp = req.send().await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let message = match resp.json::<ErrorBody>().await {
            Ok(err) => err.error,
            Err(_) => "server error".to_string(),
        };
        Err(ApiError::Server { status, message })
    }

    pub(crate) async fn pair_user(
        &self,
        telegram_user_id: u64,
        code: &str,
    ) -> Result<(), ApiError> {
        self.post_json_unit(
            None,
            "/user/pair",
            &PairUser {
                code: code.to_string(),
                telegram_id: telegram_user_id.to_string(),
            },
        )
        .await
    }

    pub(crate) async fn vault_get_main(&self, telegram_user_id: u64) -> Result<Vault, ApiError> {
        self.post_json(
            Some(telegram_user_id),
            "/vault/get",
            &Vault {
                id: None,
                name: Some("Main".to_string()),
                currency: None,
            },
        )
        .await
    }

    pub(crate) async fn vault_snapshot_main(
        &self,
        telegram_user_id: u64,
    ) -> Result<VaultSnapshot, ApiError> {
        self.post_json(
            Some(telegram_user_id),
            "/vault/snapshot",
            &Vault {
                id: None,
                name: Some("Main".to_string()),
                currency: None,
            },
        )
        .await
    }

    pub(crate) async fn stats_get_main(
        &self,
        telegram_user_id: u64,
    ) -> Result<Statistic, ApiError> {
        self.post_json(
            Some(telegram_user_id),
            "/stats/get",
            &Vault {
                id: None,
                name: Some("Main".to_string()),
                currency: None,
            },
        )
        .await
    }

    pub(crate) async fn transactions_list(
        &self,
        telegram_user_id: u64,
        payload: &TransactionList,
    ) -> Result<TransactionListResponse, ApiError> {
        self.post_json(Some(telegram_user_id), "/transactions", payload)
            .await
    }

    pub(crate) async fn transaction_get_detail(
        &self,
        telegram_user_id: u64,
        payload: &TransactionGet,
    ) -> Result<TransactionDetailResponse, ApiError> {
        self.post_json(Some(telegram_user_id), "/transactions/get", payload)
            .await
    }

    pub(crate) async fn create_income(
        &self,
        telegram_user_id: u64,
        payload: &IncomeNew,
    ) -> Result<TransactionCreated, ApiError> {
        self.post_json(Some(telegram_user_id), "/income", payload)
            .await
    }

    pub(crate) async fn create_expense(
        &self,
        telegram_user_id: u64,
        payload: &ExpenseNew,
    ) -> Result<TransactionCreated, ApiError> {
        self.post_json(Some(telegram_user_id), "/expense", payload)
            .await
    }

    pub(crate) async fn create_refund(
        &self,
        telegram_user_id: u64,
        payload: &Refund,
    ) -> Result<TransactionCreated, ApiError> {
        self.post_json(Some(telegram_user_id), "/refund", payload)
            .await
    }

    pub(crate) async fn void_transaction(
        &self,
        telegram_user_id: u64,
        tx_id: uuid::Uuid,
        payload: &TransactionVoid,
    ) -> Result<(), ApiError> {
        self.post_json_unit(
            Some(telegram_user_id),
            &format!("/transactions/{tx_id}/void"),
            payload,
        )
        .await
    }

    pub(crate) async fn update_transaction(
        &self,
        telegram_user_id: u64,
        tx_id: uuid::Uuid,
        payload: &TransactionUpdate,
    ) -> Result<(), ApiError> {
        let req = self
            .client
            .patch(self.url(&format!("/transactions/{tx_id}")))
            .header("telegram-user-id", telegram_user_id.to_string())
            .json(payload);

        let resp = req.send().await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let message = match resp.json::<ErrorBody>().await {
            Ok(err) => err.error,
            Err(_) => "server error".to_string(),
        };
        Err(ApiError::Server { status, message })
    }
}
