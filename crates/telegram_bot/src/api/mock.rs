use std::sync::Mutex;

use api_types::{
    category::{CategoryList, CategoryListResponse},
    error::ErrorCode,
    stats::Statistic,
    transaction::{
        ExpenseNew, IncomeNew, TransactionCreated, TransactionDetailResponse, TransactionGet,
        TransactionList, TransactionListResponse, TransactionUpdate, TransactionVoid,
    },
    vault::{Vault, VaultList, VaultListResponse, VaultSnapshot},
};
use async_trait::async_trait;
use reqwest::StatusCode;

use super::{ApiError, ApiGateway};

#[derive(Default)]
pub(crate) struct MockApi {
    pub(crate) pair_user: Mutex<Option<Result<(), ApiError>>>,
    pub(crate) vault_get: Mutex<Option<Result<Vault, ApiError>>>,
    pub(crate) vault_list: Mutex<Option<Result<VaultListResponse, ApiError>>>,
    pub(crate) vault_snapshot: Mutex<Option<Result<VaultSnapshot, ApiError>>>,
    pub(crate) stats_get: Mutex<Option<Result<Statistic, ApiError>>>,
    pub(crate) transactions_list: Mutex<Option<Result<TransactionListResponse, ApiError>>>,
    pub(crate) categories_list: Mutex<Option<Result<CategoryListResponse, ApiError>>>,
    pub(crate) transaction_get_detail: Mutex<Option<Result<TransactionDetailResponse, ApiError>>>,
    pub(crate) create_income: Mutex<Option<Result<TransactionCreated, ApiError>>>,
    pub(crate) create_expense: Mutex<Option<Result<TransactionCreated, ApiError>>>,
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

    async fn vault_get(&self, _telegram_user_id: u64, _payload: &Vault) -> Result<Vault, ApiError> {
        Self::take_or_error(&self.vault_get, "vault_get")
    }

    async fn vault_list(
        &self,
        _telegram_user_id: u64,
        _payload: &VaultList,
    ) -> Result<VaultListResponse, ApiError> {
        Self::take_or_error(&self.vault_list, "vault_list")
    }

    async fn vault_snapshot(
        &self,
        _telegram_user_id: u64,
        _payload: &Vault,
    ) -> Result<VaultSnapshot, ApiError> {
        Self::take_or_error(&self.vault_snapshot, "vault_snapshot")
    }

    async fn stats_get(
        &self,
        _telegram_user_id: u64,
        _payload: &Vault,
    ) -> Result<Statistic, ApiError> {
        Self::take_or_error(&self.stats_get, "stats_get")
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
