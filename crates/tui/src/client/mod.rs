use api_types::{
    cash_flow::CashFlowGet,
    category::{
        CategoryAliasCreate, CategoryAliasCreated, CategoryAliasDelete, CategoryAliasList,
        CategoryAliasListResponse, CategoryCreate, CategoryCreated, CategoryList,
        CategoryListResponse, CategoryMerge, CategoryMergePreview, CategoryMergePreviewResponse,
        CategoryUpdate, CategoryView,
    },
    error::{ErrorCode, ErrorEnvelope, ErrorPayload},
    flow::{FlowCreated, FlowNew, FlowSharedList, FlowSharedListResponse, FlowUpdate},
    membership::{MemberUpsert, MembersResponse},
    stats::Statistic,
    transaction::{
        ExpenseNew, IncomeNew, Refund, TransactionCreated, TransactionDetailResponse,
        TransactionGet, TransactionList, TransactionListResponse, TransactionUpdate,
        TransactionVoid, TransferFlowNew, TransferWalletNew,
    },
    vault::{Vault, VaultNew, VaultSnapshot},
    wallet::{WalletCreated, WalletNew, WalletUpdate},
};
use reqwest::Url;

use serde::de::DeserializeOwned;

use crate::error::{AppError, Result};

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
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

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
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

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

    pub async fn vault_new(
        &self,
        username: &str,
        password: &str,
        payload: VaultNew,
    ) -> std::result::Result<Vault, ClientError> {
        let endpoint = self
            .base_url
            .join("vault/new")
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

    pub async fn vault_delete(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .delete(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn stats_get(
        &self,
        username: &str,
        password: &str,
        payload: Vault,
    ) -> std::result::Result<Statistic, ClientError> {
        let endpoint = self
            .base_url
            .join("stats/get")
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

    pub async fn categories_list(
        &self,
        username: &str,
        password: &str,
        payload: CategoryList,
    ) -> std::result::Result<CategoryListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("categories/list")
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

    pub async fn categories_create(
        &self,
        username: &str,
        password: &str,
        payload: CategoryCreate,
    ) -> std::result::Result<CategoryCreated, ClientError> {
        let endpoint = self
            .base_url
            .join("categories")
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

    pub async fn categories_update(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryUpdate,
    ) -> std::result::Result<CategoryView, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .patch(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn category_aliases_list(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryAliasList,
    ) -> std::result::Result<CategoryAliasListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/aliases/list"))
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

    pub async fn category_alias_create(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryAliasCreate,
    ) -> std::result::Result<CategoryAliasCreated, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/aliases"))
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

    pub async fn category_alias_delete(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        alias_id: uuid::Uuid,
        payload: CategoryAliasDelete,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/aliases/{alias_id}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .delete(endpoint)
            .basic_auth(username, Some(password))
            .json(&payload)
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn categories_merge_preview(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryMergePreview,
    ) -> std::result::Result<CategoryMergePreviewResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/merge/preview"))
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

    pub async fn categories_merge(
        &self,
        username: &str,
        password: &str,
        category_id: uuid::Uuid,
        payload: CategoryMerge,
    ) -> std::result::Result<CategoryView, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("categories/{category_id}/merge"))
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

    pub async fn vault_members_list(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
    ) -> std::result::Result<MembersResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/members"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .get(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn vault_member_upsert(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        payload: MemberUpsert,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/members"))
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

    pub async fn vault_member_remove(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        member_username: &str,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/members/{member_username}"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .delete(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
    }

    pub async fn flow_members_list(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        flow_id: uuid::Uuid,
    ) -> std::result::Result<MembersResponse, ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/flows/{flow_id}/members"))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .get(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_json(res).await
    }

    pub async fn flow_member_upsert(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        flow_id: uuid::Uuid,
        payload: MemberUpsert,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("vault/{vault_id}/flows/{flow_id}/members"))
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

    pub async fn flow_member_remove(
        &self,
        username: &str,
        password: &str,
        vault_id: &str,
        flow_id: uuid::Uuid,
        member_username: &str,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!(
                "vault/{vault_id}/flows/{flow_id}/members/{member_username}"
            ))
            .map_err(|err| ClientError::Client(format!("invalid base_url: {err}")))?;

        let res = self
            .http
            .delete(endpoint)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(ClientError::Transport)?;

        handle_empty(res).await
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

    pub async fn flow_new(
        &self,
        username: &str,
        password: &str,
        payload: FlowNew,
    ) -> std::result::Result<FlowCreated, ClientError> {
        let endpoint = self
            .base_url
            .join("flows")
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

    pub async fn flows_shared_list(
        &self,
        username: &str,
        password: &str,
        payload: FlowSharedList,
    ) -> std::result::Result<FlowSharedListResponse, ClientError> {
        let endpoint = self
            .base_url
            .join("flows/shared")
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

    pub async fn flow_update(
        &self,
        username: &str,
        password: &str,
        flow_id: uuid::Uuid,
        payload: FlowUpdate,
    ) -> std::result::Result<(), ClientError> {
        let endpoint = self
            .base_url
            .join(&format!("flows/{flow_id}"))
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

    pub async fn cash_flow_get(
        &self,
        username: &str,
        password: &str,
        payload: CashFlowGet,
    ) -> std::result::Result<engine::CashFlow, ClientError> {
        let endpoint = self
            .base_url
            .join("cashFlow/get")
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
