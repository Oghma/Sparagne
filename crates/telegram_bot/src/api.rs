use api_types::{
    category::{
        CategoryList, CategoryListResponse, CategoryMerge, CategoryMergePreview,
        CategoryMergePreviewResponse, CategoryView,
    },
    error::{ErrorCode, ErrorEnvelope, ErrorPayload},
    flow::{FlowSharedList, FlowSharedListResponse},
    membership::{MemberUpsert, MembersResponse},
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

    async fn get_json<TResp: for<'de> serde::Deserialize<'de>>(
        &self,
        telegram_user_id: Option<u64>,
        path: &str,
    ) -> Result<TResp, ApiError> {
        let mut req = self.client.get(self.url(path));
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

    async fn delete_unit(&self, telegram_user_id: Option<u64>, path: &str) -> Result<(), ApiError> {
        let mut req = self.client.delete(self.url(path));
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

    pub(crate) async fn vault_get_main(&self, telegram_user_id: u64) -> Result<Vault, ApiError> {
        self.post_json(
            Some(telegram_user_id),
            "/vault/get",
            &Vault {
                id: None,
                name: Some("Main".to_string()),
                currency: None,
                owner: None,
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
                owner: None,
            },
        )
        .await
    }

    pub(crate) async fn flows_shared_main(
        &self,
        telegram_user_id: u64,
    ) -> Result<FlowSharedListResponse, ApiError> {
        let vault = self.vault_get_main(telegram_user_id).await?;
        let vault_id = vault.id.ok_or(ApiError::Server {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: ErrorCode::Unknown,
            message: "vault id missing".to_string(),
        })?;

        self.post_json(
            Some(telegram_user_id),
            "/flows/shared",
            &FlowSharedList {
                vault_id,
                include_archived: Some(true),
            },
        )
        .await
    }

    pub(crate) async fn vault_delete_main(&self, telegram_user_id: u64) -> Result<(), ApiError> {
        let vault = self.vault_get_main(telegram_user_id).await?;
        let vault_id = vault.id.ok_or(ApiError::Server {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: ErrorCode::Unknown,
            message: "vault id missing".to_string(),
        })?;

        self.delete_unit(Some(telegram_user_id), &format!("/vault/{vault_id}"))
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
                owner: None,
            },
        )
        .await
    }

    pub(crate) async fn vault_members_list(
        &self,
        telegram_user_id: u64,
        vault_id: &str,
    ) -> Result<MembersResponse, ApiError> {
        self.get_json(
            Some(telegram_user_id),
            &format!("/vault/{vault_id}/members"),
        )
        .await
    }

    pub(crate) async fn vault_member_upsert(
        &self,
        telegram_user_id: u64,
        vault_id: &str,
        payload: &MemberUpsert,
    ) -> Result<(), ApiError> {
        self.post_json_unit(
            Some(telegram_user_id),
            &format!("/vault/{vault_id}/members"),
            payload,
        )
        .await
    }

    pub(crate) async fn vault_member_remove(
        &self,
        telegram_user_id: u64,
        vault_id: &str,
        username: &str,
    ) -> Result<(), ApiError> {
        self.delete_unit(
            Some(telegram_user_id),
            &format!("/vault/{vault_id}/members/{username}"),
        )
        .await
    }

    pub(crate) async fn flow_members_list(
        &self,
        telegram_user_id: u64,
        vault_id: &str,
        flow_id: uuid::Uuid,
    ) -> Result<MembersResponse, ApiError> {
        self.get_json(
            Some(telegram_user_id),
            &format!("/vault/{vault_id}/flows/{flow_id}/members"),
        )
        .await
    }

    pub(crate) async fn flow_member_upsert(
        &self,
        telegram_user_id: u64,
        vault_id: &str,
        flow_id: uuid::Uuid,
        payload: &MemberUpsert,
    ) -> Result<(), ApiError> {
        self.post_json_unit(
            Some(telegram_user_id),
            &format!("/vault/{vault_id}/flows/{flow_id}/members"),
            payload,
        )
        .await
    }

    pub(crate) async fn flow_member_remove(
        &self,
        telegram_user_id: u64,
        vault_id: &str,
        flow_id: uuid::Uuid,
        username: &str,
    ) -> Result<(), ApiError> {
        self.delete_unit(
            Some(telegram_user_id),
            &format!("/vault/{vault_id}/flows/{flow_id}/members/{username}"),
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

    pub(crate) async fn categories_list(
        &self,
        telegram_user_id: u64,
        payload: &CategoryList,
    ) -> Result<CategoryListResponse, ApiError> {
        self.post_json(Some(telegram_user_id), "/categories/list", payload)
            .await
    }

    pub(crate) async fn categories_merge_preview(
        &self,
        telegram_user_id: u64,
        category_id: uuid::Uuid,
        payload: &CategoryMergePreview,
    ) -> Result<CategoryMergePreviewResponse, ApiError> {
        self.post_json(
            Some(telegram_user_id),
            &format!("/categories/{category_id}/merge/preview"),
            payload,
        )
        .await
    }

    pub(crate) async fn categories_merge(
        &self,
        telegram_user_id: u64,
        category_id: uuid::Uuid,
        payload: &CategoryMerge,
    ) -> Result<CategoryView, ApiError> {
        self.post_json(
            Some(telegram_user_id),
            &format!("/categories/{category_id}/merge"),
            payload,
        )
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
