//! Sparagne budgeting engine core types and operations.
//!
//! This crate exposes the domain model, commands, and the main [`Engine`]
//! facade used by the server and bots.

/// Cash flow aggregate model.
pub use cash_flows::CashFlow;
/// Command inputs for transaction operations.
pub use commands::{
    ExpenseCmd, IncomeCmd, RefundCmd, TransferFlowCmd, TransferWalletCmd, TxMeta,
    UpdateTransactionCmd,
};
/// Currency codes and helpers.
pub use currency::Currency;
/// Engine facade, builder, and transaction listing filters.
pub use ops::{Engine, EngineBuilder, TransactionListFilter};
/// Engine error type.
pub use error::EngineError;
/// Transaction leg primitives.
pub use legs::{Leg, LegTarget};
/// Money parsing and formatting helper.
pub use money::Money;
/// Transaction models and kinds.
pub use transactions::{Transaction, TransactionKind, TransactionNew};
/// Vault aggregate model.
pub use vault::Vault;
/// Wallet aggregate model.
pub use wallets::Wallet;

mod cash_flows;
mod commands;
mod currency;
mod ops;
mod error;
mod flow_memberships;
mod legs;
mod money;
mod transactions;
mod users;
mod util;
mod vault;
mod vault_memberships;
mod wallets;

type ResultEngine<T> = Result<T, EngineError>;
