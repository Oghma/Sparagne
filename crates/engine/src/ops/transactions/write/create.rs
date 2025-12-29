use uuid::Uuid;

use crate::{ExpenseCmd, IncomeCmd, RefundCmd, ResultEngine, TransactionKind};

use super::common::FlowWalletCmd;
use super::super::super::Engine;

impl Engine {
    /// Create an income transaction (increases both wallet and flow).
    pub async fn income(&self, cmd: IncomeCmd) -> ResultEngine<Uuid> {
        let IncomeCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
        } = cmd;
        self.create_flow_wallet_transaction_cmd(FlowWalletCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
            kind: TransactionKind::Income,
        })
        .await
    }

    /// Create an expense transaction (decreases both wallet and flow).
    pub async fn expense(&self, cmd: ExpenseCmd) -> ResultEngine<Uuid> {
        let ExpenseCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
        } = cmd;
        self.create_flow_wallet_transaction_cmd(FlowWalletCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
            kind: TransactionKind::Expense,
        })
        .await
    }

    /// Create a refund transaction (increases both wallet and flow).
    ///
    /// A refund is modeled as its own `TransactionKind::Refund` instead of a
    /// negative expense, to keep reporting correct and explicit.
    pub async fn refund(&self, cmd: RefundCmd) -> ResultEngine<Uuid> {
        let RefundCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
        } = cmd;
        self.create_flow_wallet_transaction_cmd(FlowWalletCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
            kind: TransactionKind::Refund,
        })
        .await
    }
}
