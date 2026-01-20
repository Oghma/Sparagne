use api_types::{
    category::{CategoryList, CategoryListResponse},
    error::{ErrorCode, ErrorEnvelope, ErrorPayload},
    stats::Statistic,
    transaction::{
        ExpenseNew, IncomeNew, TransactionCreated, TransactionDetailResponse, TransactionGet,
        TransactionList, TransactionListResponse, TransactionUpdate, TransactionVoid,
    },
    user::PairUser,
    vault::{Vault, VaultSnapshot},
};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};

#[derive(Clone, Debug)]
pub(crate) struct ApiClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ApiError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("{status}: {code:?} {message}")]
    Server {
        status: StatusCode,
        code: ErrorCode,
        message: String,
    },
}

#[async_trait]
pub(crate) trait ApiGateway: Send + Sync {
    async fn pair_user(&self, telegram_user_id: u64, code: &str) -> Result<(), ApiError>;
    async fn vault_get(&self, telegram_user_id: u64, payload: &Vault) -> Result<Vault, ApiError>;
    async fn vault_snapshot(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<VaultSnapshot, ApiError>;
    async fn stats_get(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<Statistic, ApiError>;
    async fn transactions_list(
        &self,
        telegram_user_id: u64,
        payload: &TransactionList,
    ) -> Result<TransactionListResponse, ApiError>;
    async fn categories_list(
        &self,
        telegram_user_id: u64,
        payload: &CategoryList,
    ) -> Result<CategoryListResponse, ApiError>;
    async fn transaction_get_detail(
        &self,
        telegram_user_id: u64,
        payload: &TransactionGet,
    ) -> Result<TransactionDetailResponse, ApiError>;
    async fn create_income(
        &self,
        telegram_user_id: u64,
        payload: &IncomeNew,
    ) -> Result<TransactionCreated, ApiError>;
    async fn create_expense(
        &self,
        telegram_user_id: u64,
        payload: &ExpenseNew,
    ) -> Result<TransactionCreated, ApiError>;
    async fn void_transaction(
        &self,
        telegram_user_id: u64,
        tx_id: uuid::Uuid,
        payload: &TransactionVoid,
    ) -> Result<(), ApiError>;
    async fn update_transaction(
        &self,
        telegram_user_id: u64,
        tx_id: uuid::Uuid,
        payload: &TransactionUpdate,
    ) -> Result<(), ApiError>;
}

#[async_trait]
impl<T> ApiGateway for std::sync::Arc<T>
where
    T: ApiGateway + ?Sized,
{
    async fn pair_user(&self, telegram_user_id: u64, code: &str) -> Result<(), ApiError> {
        self.as_ref().pair_user(telegram_user_id, code).await
    }

    async fn vault_get(&self, telegram_user_id: u64, payload: &Vault) -> Result<Vault, ApiError> {
        self.as_ref().vault_get(telegram_user_id, payload).await
    }

    async fn vault_snapshot(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<VaultSnapshot, ApiError> {
        self.as_ref()
            .vault_snapshot(telegram_user_id, payload)
            .await
    }

    async fn stats_get(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<Statistic, ApiError> {
        self.as_ref().stats_get(telegram_user_id, payload).await
    }

    async fn transactions_list(
        &self,
        telegram_user_id: u64,
        payload: &TransactionList,
    ) -> Result<TransactionListResponse, ApiError> {
        self.as_ref()
            .transactions_list(telegram_user_id, payload)
            .await
    }

    async fn categories_list(
        &self,
        telegram_user_id: u64,
        payload: &CategoryList,
    ) -> Result<CategoryListResponse, ApiError> {
        self.as_ref()
            .categories_list(telegram_user_id, payload)
            .await
    }

    async fn transaction_get_detail(
        &self,
        telegram_user_id: u64,
        payload: &TransactionGet,
    ) -> Result<TransactionDetailResponse, ApiError> {
        self.as_ref()
            .transaction_get_detail(telegram_user_id, payload)
            .await
    }

    async fn create_income(
        &self,
        telegram_user_id: u64,
        payload: &IncomeNew,
    ) -> Result<TransactionCreated, ApiError> {
        self.as_ref().create_income(telegram_user_id, payload).await
    }

    async fn create_expense(
        &self,
        telegram_user_id: u64,
        payload: &ExpenseNew,
    ) -> Result<TransactionCreated, ApiError> {
        self.as_ref()
            .create_expense(telegram_user_id, payload)
            .await
    }

    async fn void_transaction(
        &self,
        telegram_user_id: u64,
        tx_id: uuid::Uuid,
        payload: &TransactionVoid,
    ) -> Result<(), ApiError> {
        self.as_ref()
            .void_transaction(telegram_user_id, tx_id, payload)
            .await
    }

    async fn update_transaction(
        &self,
        telegram_user_id: u64,
        tx_id: uuid::Uuid,
        payload: &TransactionUpdate,
    ) -> Result<(), ApiError> {
        self.as_ref()
            .update_transaction(telegram_user_id, tx_id, payload)
            .await
    }
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

        let payload = match resp.json::<ErrorEnvelope>().await {
            Ok(err) => err.error,
            Err(_) => ErrorPayload {
                code: ErrorCode::Unknown,
                message: "server error".to_string(),
                details: None,
            },
        };
        Err(ApiError::Server {
            status,
            code: payload.code,
            message: payload.message,
        })
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
        let payload = match resp.json::<ErrorEnvelope>().await {
            Ok(err) => err.error,
            Err(_) => ErrorPayload {
                code: ErrorCode::Unknown,
                message: "server error".to_string(),
                details: None,
            },
        };
        Err(ApiError::Server {
            status,
            code: payload.code,
            message: payload.message,
        })
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

    pub(crate) async fn vault_get(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<Vault, ApiError> {
        self.post_json(Some(telegram_user_id), "/vault/get", payload)
            .await
    }

    pub(crate) async fn vault_snapshot(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<VaultSnapshot, ApiError> {
        self.post_json(Some(telegram_user_id), "/vault/snapshot", payload)
            .await
    }

    pub(crate) async fn stats_get(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<Statistic, ApiError> {
        self.post_json(Some(telegram_user_id), "/stats/get", payload)
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

    pub(crate) async fn categories_list(
        &self,
        telegram_user_id: u64,
        payload: &CategoryList,
    ) -> Result<CategoryListResponse, ApiError> {
        self.post_json(Some(telegram_user_id), "/categories/list", payload)
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
        let payload = match resp.json::<ErrorEnvelope>().await {
            Ok(err) => err.error,
            Err(_) => ErrorPayload {
                code: ErrorCode::Unknown,
                message: "server error".to_string(),
                details: None,
            },
        };
        Err(ApiError::Server {
            status,
            code: payload.code,
            message: payload.message,
        })
    }
}

#[async_trait]
impl ApiGateway for ApiClient {
    async fn pair_user(&self, telegram_user_id: u64, code: &str) -> Result<(), ApiError> {
        self.pair_user(telegram_user_id, code).await
    }

    async fn vault_get(&self, telegram_user_id: u64, payload: &Vault) -> Result<Vault, ApiError> {
        self.vault_get(telegram_user_id, payload).await
    }

    async fn vault_snapshot(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<VaultSnapshot, ApiError> {
        self.vault_snapshot(telegram_user_id, payload).await
    }

    async fn stats_get(
        &self,
        telegram_user_id: u64,
        payload: &Vault,
    ) -> Result<Statistic, ApiError> {
        self.stats_get(telegram_user_id, payload).await
    }

    async fn transactions_list(
        &self,
        telegram_user_id: u64,
        payload: &TransactionList,
    ) -> Result<TransactionListResponse, ApiError> {
        self.transactions_list(telegram_user_id, payload).await
    }

    async fn categories_list(
        &self,
        telegram_user_id: u64,
        payload: &CategoryList,
    ) -> Result<CategoryListResponse, ApiError> {
        self.categories_list(telegram_user_id, payload).await
    }

    async fn transaction_get_detail(
        &self,
        telegram_user_id: u64,
        payload: &TransactionGet,
    ) -> Result<TransactionDetailResponse, ApiError> {
        self.transaction_get_detail(telegram_user_id, payload).await
    }

    async fn create_income(
        &self,
        telegram_user_id: u64,
        payload: &IncomeNew,
    ) -> Result<TransactionCreated, ApiError> {
        self.create_income(telegram_user_id, payload).await
    }

    async fn create_expense(
        &self,
        telegram_user_id: u64,
        payload: &ExpenseNew,
    ) -> Result<TransactionCreated, ApiError> {
        self.create_expense(telegram_user_id, payload).await
    }

    async fn void_transaction(
        &self,
        telegram_user_id: u64,
        tx_id: uuid::Uuid,
        payload: &TransactionVoid,
    ) -> Result<(), ApiError> {
        self.void_transaction(telegram_user_id, tx_id, payload)
            .await
    }

    async fn update_transaction(
        &self,
        telegram_user_id: u64,
        tx_id: uuid::Uuid,
        payload: &TransactionUpdate,
    ) -> Result<(), ApiError> {
        self.update_transaction(telegram_user_id, tx_id, payload)
            .await
    }
}

#[cfg(test)]
pub(crate) mod mock;
