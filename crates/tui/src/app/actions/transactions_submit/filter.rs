use crate::{
    app::{App, TransactionsFilterState, TransactionsMode},
    error::Result,
};

impl App {
    pub(crate) async fn apply_filter(&mut self) -> Result<()> {
        let (from_input, to_input, kind_income, kind_expense, kind_refund, kind_tw, kind_tf) = {
            let filter = &self.state.transactions.filter;
            (
                filter.from_input.clone(),
                filter.to_input.clone(),
                filter.kind_income,
                filter.kind_expense,
                filter.kind_refund,
                filter.kind_transfer_wallet,
                filter.kind_transfer_flow,
            )
        };

        let from = if from_input.trim().is_empty() {
            None
        } else {
            match self.parse_local_datetime(&from_input) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.filter.error = Some(message);
                    return Ok(());
                }
            }
        };

        let to = if to_input.trim().is_empty() {
            None
        } else {
            match self.parse_local_datetime(&to_input) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.filter.error = Some(message);
                    return Ok(());
                }
            }
        };

        let mut kinds = Vec::new();
        if kind_income {
            kinds.push(api_types::transaction::TransactionKind::Income);
        }
        if kind_expense {
            kinds.push(api_types::transaction::TransactionKind::Expense);
        }
        if kind_refund {
            kinds.push(api_types::transaction::TransactionKind::Refund);
        }
        if kind_tw {
            kinds.push(api_types::transaction::TransactionKind::TransferWallet);
        }
        if kind_tf {
            kinds.push(api_types::transaction::TransactionKind::TransferFlow);
        }

        self.state.transactions.filter_from = from;
        self.state.transactions.filter_to = to;
        self.state.transactions.filter_kinds = if kinds.is_empty() { None } else { Some(kinds) };

        self.state.transactions.filter.error = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.load_transactions(true).await?;
        Ok(())
    }

    pub(crate) async fn clear_filters(&mut self) -> Result<()> {
        self.state.transactions.scope_wallet_id = None;
        self.state.transactions.scope_flow_id = None;
        self.state.transactions.filter_from = None;
        self.state.transactions.filter_to = None;
        self.state.transactions.filter_kinds = None;
        self.state.transactions.filter = TransactionsFilterState::default();
        self.load_transactions(true).await?;
        Ok(())
    }
}
