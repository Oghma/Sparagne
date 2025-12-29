//! Sparagne budgeting engine core types and operations.
//!
//! This crate exposes the domain model, commands, and the main [`Engine`]
//! facade used by the server and bots.

pub use cash_flows::CashFlow;
pub use commands::{
    ExpenseCmd, IncomeCmd, RefundCmd, TransferFlowCmd, TransferWalletCmd, TxMeta,
    UpdateTransactionCmd,
};
pub use currency::Currency;
pub use ops::{Engine, EngineBuilder, TransactionListFilter};
pub use error::EngineError;
pub use legs::{Leg, LegTarget};
pub use money::Money;
pub use transactions::{Transaction, TransactionKind, TransactionNew};
pub use vault::Vault;
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
