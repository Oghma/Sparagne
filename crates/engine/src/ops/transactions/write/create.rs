use uuid::Uuid;

use crate::{ExpenseCmd, IncomeCmd, RefundCmd, ResultEngine, TransactionKind};

use super::{super::super::Engine, common::FlowWalletCmd};

/// Generates a flow+wallet transaction method (income, expense, refund).
macro_rules! impl_flow_wallet_tx {
    ($(#[$meta:meta])* $fn_name:ident, $cmd_type:ty, $kind:expr) => {
        $(#[$meta])*
        pub async fn $fn_name(&self, cmd: $cmd_type) -> ResultEngine<Uuid> {
            self.create_flow_wallet_transaction_cmd(FlowWalletCmd {
                vault_id: cmd.vault_id,
                amount_minor: cmd.amount_minor,
                flow_id: cmd.flow_id,
                wallet_id: cmd.wallet_id,
                meta: cmd.meta,
                user_id: cmd.user_id,
                kind: $kind,
            })
            .await
        }
    };
}

impl Engine {
    impl_flow_wallet_tx!(
        /// Create an income transaction (increases both wallet and flow).
        income,
        IncomeCmd,
        TransactionKind::Income
    );

    impl_flow_wallet_tx!(
        /// Create an expense transaction (decreases both wallet and flow).
        expense,
        ExpenseCmd,
        TransactionKind::Expense
    );

    impl_flow_wallet_tx!(
        /// Create a refund transaction (increases both wallet and flow).
        ///
        /// A refund is modeled as its own `TransactionKind::Refund` instead of a
        /// negative expense, to keep reporting correct and explicit.
        refund,
        RefundCmd,
        TransactionKind::Refund
    );
}
