use std::sync::Mutex;

use api_types::{
    category::{
        CategoryList, CategoryListResponse, CategoryMerge, CategoryMergePreview,
        CategoryMergePreviewResponse, CategoryView,
    },
    error::ErrorCode,
    flow::FlowSharedListResponse,
    membership::{MemberUpsert, MembersResponse},
    stats::Statistic,
    transaction::{
        ExpenseNew, IncomeNew, Refund, TransactionCreated, TransactionDetailResponse,
        TransactionGet, TransactionList, TransactionListResponse, TransactionUpdate,
        TransactionVoid,
    },
    vault::{Vault, VaultSnapshot},
};
use async_trait::async_trait;
use reqwest::StatusCode;

use super::{ApiError, ApiGateway};

#[derive(Default)]
pub(crate) struct MockApi {
    pub(crate) pair_user: Mutex<Option<Result<(), ApiError>>>,
    pub(crate) vault_get_main: Mutex<Option<Result<Vault, ApiError>>>,
    pub(crate) vault_snapshot_main: Mutex<Option<Result<VaultSnapshot, ApiError>>>,
    pub(crate) flows_shared_main: Mutex<Option<Result<FlowSharedListResponse, ApiError>>>,
    pub(crate) vault_delete_main: Mutex<Option<Result<(), ApiError>>>,
    pub(crate) stats_get_main: Mutex<Option<Result<Statistic, ApiError>>>,
    pub(crate) vault_members_list: Mutex<Option<Result<MembersResponse, ApiError>>>,
    pub(crate) vault_member_upsert: Mutex<Option<Result<(), ApiError>>>,
    pub(crate) vault_member_remove: Mutex<Option<Result<(), ApiError>>>,
    pub(crate) flow_members_list: Mutex<Option<Result<MembersResponse, ApiError>>>,
    pub(crate) flow_member_upsert: Mutex<Option<Result<(), ApiError>>>,
    pub(crate) flow_member_remove: Mutex<Option<Result<(), ApiError>>>,
    pub(crate) transactions_list: Mutex<Option<Result<TransactionListResponse, ApiError>>>,
    pub(crate) categories_list: Mutex<Option<Result<CategoryListResponse, ApiError>>>,
    pub(crate) categories_merge_preview:
        Mutex<Option<Result<CategoryMergePreviewResponse, ApiError>>>,
    pub(crate) categories_merge: Mutex<Option<Result<CategoryView, ApiError>>>,
    pub(crate) transaction_get_detail: Mutex<Option<Result<TransactionDetailResponse, ApiError>>>,
    pub(crate) create_income: Mutex<Option<Result<TransactionCreated, ApiError>>>,
    pub(crate) create_expense: Mutex<Option<Result<TransactionCreated, ApiError>>>,
    pub(crate) create_refund: Mutex<Option<Result<TransactionCreated, ApiError>>>,
    pub(crate) void_transaction: Mutex<Option<Result<(), ApiError>>>,
    pub(crate) update_transaction: Mutex<Option<Result<(), ApiError>>>,
}

impl MockApi {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn unconfigured_error(name: &'static str) -> ApiError {
        ApiError::Server {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: ErrorCode::Unknown,
            message: format!("mock response not configured: {name}"),
        }
    }

    fn lock_error() -> ApiError {
        ApiError::Server {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: ErrorCode::Unknown,
            message: "mock api lock poisoned".to_string(),
        }
    }

    fn take_or_error<T>(
        slot: &Mutex<Option<Result<T, ApiError>>>,
        name: &'static str,
    ) -> Result<T, ApiError> {
        let mut guard = match slot.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(Self::lock_error()),
        };

        match guard.take() {
            Some(result) => result,
            None => Err(Self::unconfigured_error(name)),
        }
    }
}

#[async_trait]
impl ApiGateway for MockApi {
    async fn pair_user(&self, _telegram_user_id: u64, _code: &str) -> Result<(), ApiError> {
        Self::take_or_error(&self.pair_user, "pair_user")
    }

    async fn vault_get_main(&self, _telegram_user_id: u64) -> Result<Vault, ApiError> {
        Self::take_or_error(&self.vault_get_main, "vault_get_main")
    }

    async fn vault_snapshot_main(&self, _telegram_user_id: u64) -> Result<VaultSnapshot, ApiError> {
        Self::take_or_error(&self.vault_snapshot_main, "vault_snapshot_main")
    }

    async fn flows_shared_main(
        &self,
        _telegram_user_id: u64,
    ) -> Result<FlowSharedListResponse, ApiError> {
        Self::take_or_error(&self.flows_shared_main, "flows_shared_main")
    }

    async fn vault_delete_main(&self, _telegram_user_id: u64) -> Result<(), ApiError> {
        Self::take_or_error(&self.vault_delete_main, "vault_delete_main")
    }

    async fn stats_get_main(&self, _telegram_user_id: u64) -> Result<Statistic, ApiError> {
        Self::take_or_error(&self.stats_get_main, "stats_get_main")
    }

    async fn vault_members_list(
        &self,
        _telegram_user_id: u64,
        _vault_id: &str,
    ) -> Result<MembersResponse, ApiError> {
        Self::take_or_error(&self.vault_members_list, "vault_members_list")
    }

    async fn vault_member_upsert(
        &self,
        _telegram_user_id: u64,
        _vault_id: &str,
        _payload: &MemberUpsert,
    ) -> Result<(), ApiError> {
        Self::take_or_error(&self.vault_member_upsert, "vault_member_upsert")
    }

    async fn vault_member_remove(
        &self,
        _telegram_user_id: u64,
        _vault_id: &str,
        _username: &str,
    ) -> Result<(), ApiError> {
        Self::take_or_error(&self.vault_member_remove, "vault_member_remove")
    }

    async fn flow_members_list(
        &self,
        _telegram_user_id: u64,
        _vault_id: &str,
        _flow_id: uuid::Uuid,
    ) -> Result<MembersResponse, ApiError> {
        Self::take_or_error(&self.flow_members_list, "flow_members_list")
    }

    async fn flow_member_upsert(
        &self,
        _telegram_user_id: u64,
        _vault_id: &str,
        _flow_id: uuid::Uuid,
        _payload: &MemberUpsert,
    ) -> Result<(), ApiError> {
        Self::take_or_error(&self.flow_member_upsert, "flow_member_upsert")
    }

    async fn flow_member_remove(
        &self,
        _telegram_user_id: u64,
        _vault_id: &str,
        _flow_id: uuid::Uuid,
        _username: &str,
    ) -> Result<(), ApiError> {
        Self::take_or_error(&self.flow_member_remove, "flow_member_remove")
    }

    async fn transactions_list(
        &self,
        _telegram_user_id: u64,
        _payload: &TransactionList,
    ) -> Result<TransactionListResponse, ApiError> {
        Self::take_or_error(&self.transactions_list, "transactions_list")
    }

    async fn categories_list(
        &self,
        _telegram_user_id: u64,
        _payload: &CategoryList,
    ) -> Result<CategoryListResponse, ApiError> {
        Self::take_or_error(&self.categories_list, "categories_list")
    }

    async fn categories_merge_preview(
        &self,
        _telegram_user_id: u64,
        _category_id: uuid::Uuid,
        _payload: &CategoryMergePreview,
    ) -> Result<CategoryMergePreviewResponse, ApiError> {
        Self::take_or_error(&self.categories_merge_preview, "categories_merge_preview")
    }

    async fn categories_merge(
        &self,
        _telegram_user_id: u64,
        _category_id: uuid::Uuid,
        _payload: &CategoryMerge,
    ) -> Result<CategoryView, ApiError> {
        Self::take_or_error(&self.categories_merge, "categories_merge")
    }

    async fn transaction_get_detail(
        &self,
        _telegram_user_id: u64,
        _payload: &TransactionGet,
    ) -> Result<TransactionDetailResponse, ApiError> {
        Self::take_or_error(&self.transaction_get_detail, "transaction_get_detail")
    }

    async fn create_income(
        &self,
        _telegram_user_id: u64,
        _payload: &IncomeNew,
    ) -> Result<TransactionCreated, ApiError> {
        Self::take_or_error(&self.create_income, "create_income")
    }

    async fn create_expense(
        &self,
        _telegram_user_id: u64,
        _payload: &ExpenseNew,
    ) -> Result<TransactionCreated, ApiError> {
        Self::take_or_error(&self.create_expense, "create_expense")
    }

    async fn create_refund(
        &self,
        _telegram_user_id: u64,
        _payload: &Refund,
    ) -> Result<TransactionCreated, ApiError> {
        Self::take_or_error(&self.create_refund, "create_refund")
    }

    async fn void_transaction(
        &self,
        _telegram_user_id: u64,
        _tx_id: uuid::Uuid,
        _payload: &TransactionVoid,
    ) -> Result<(), ApiError> {
        Self::take_or_error(&self.void_transaction, "void_transaction")
    }

    async fn update_transaction(
        &self,
        _telegram_user_id: u64,
        _tx_id: uuid::Uuid,
        _payload: &TransactionUpdate,
    ) -> Result<(), ApiError> {
        Self::take_or_error(&self.update_transaction, "update_transaction")
    }
}
