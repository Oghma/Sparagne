//! Picker-related transaction handlers.
//!
//! Contains methods for wallet picker, flow picker, and transfer picker
//! operations.

use crate::{
    app::{App, TransactionsMode, TransferField, TransferFormState},
    error::Result,
    text::{TextKey, t},
    validation::DateField,
};

impl App {
    pub(crate) fn transactions_picker_next(&mut self) {
        let len = self.transactions_picker_len();
        if len == 0 {
            return;
        }
        self.state.transactions.picker_index =
            (self.state.transactions.picker_index + 1).min(len - 1);
    }

    pub(crate) fn transactions_picker_prev(&mut self) {
        let len = self.transactions_picker_len();
        if len == 0 {
            return;
        }
        self.state.transactions.picker_index =
            self.state.transactions.picker_index.saturating_sub(1);
    }

    pub(crate) fn transactions_picker_len(&self) -> usize {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            return 0;
        };
        match self.state.transactions.mode {
            TransactionsMode::PickWallet => snapshot.wallets.len() + 1,
            TransactionsMode::PickFlow => snapshot.flows.len() + 1,
            _ => 0,
        }
    }

    pub(crate) fn open_wallet_picker(&mut self) {
        self.state.transactions.quick_active = false;
        self.state.transactions.picker_index = self
            .state
            .transactions
            .scope_wallet_id
            .and_then(|wallet_id| {
                self.state.snapshot.as_ref().and_then(|snap| {
                    snap.wallets
                        .iter()
                        .position(|wallet| wallet.id == wallet_id)
                })
            })
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.state.transactions.mode = TransactionsMode::PickWallet;
    }

    pub(crate) fn open_flow_picker(&mut self) {
        self.state.transactions.quick_active = false;
        self.state.transactions.picker_index = self
            .state
            .transactions
            .scope_flow_id
            .and_then(|flow_id| {
                self.state
                    .snapshot
                    .as_ref()
                    .and_then(|snap| snap.flows.iter().position(|flow| flow.id == flow_id))
            })
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.state.transactions.mode = TransactionsMode::PickFlow;
    }

    pub(crate) async fn apply_wallet_picker(&mut self) -> Result<()> {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            self.state.transactions.error =
                Some(t(self.state.locale, TextKey::ValidationSnapshotUnavailable).to_string());
            self.state.transactions.mode = TransactionsMode::List;
            return Ok(());
        };

        if self.state.transactions.picker_index == 0 {
            self.state.transactions.scope_wallet_id = None;
        } else {
            let index = self.state.transactions.picker_index - 1;
            if let Some(wallet) = snapshot.wallets.get(index) {
                self.state.transactions.scope_wallet_id = Some(wallet.id);
            }
        }

        self.state.transactions.scope_flow_id = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.state.transactions.picker_index = 0;
        self.load_transactions(true).await?;
        Ok(())
    }

    pub(crate) async fn apply_flow_picker(&mut self) -> Result<()> {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            self.state.transactions.error =
                Some(t(self.state.locale, TextKey::ValidationSnapshotUnavailable).to_string());
            self.state.transactions.mode = TransactionsMode::List;
            return Ok(());
        };

        if self.state.transactions.picker_index == 0 {
            self.state.transactions.scope_flow_id = None;
        } else {
            let index = self.state.transactions.picker_index - 1;
            if let Some(flow) = snapshot.flows.get(index) {
                self.state.transactions.scope_flow_id = Some(flow.id);
                self.state.last_flow_id = Some(flow.id);
            }
        }

        self.state.transactions.scope_wallet_id = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.state.transactions.picker_index = 0;
        self.load_transactions(true).await?;
        Ok(())
    }

    pub(crate) fn open_transfer_picker(&mut self) {
        self.state.transactions.quick_active = false;
        self.state.transactions.picker_index = 0;
        self.state.transactions.mode = TransactionsMode::TransferPicker;
    }

    pub(crate) fn apply_transfer_picker(&mut self) -> Result<()> {
        match self.state.transactions.picker_index {
            0 => self.start_transfer_wallet(),
            1 => self.start_transfer_flow(),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn transfer_picker_next(&mut self) {
        self.state.transactions.picker_index = (self.state.transactions.picker_index + 1) % 2;
    }

    pub(crate) fn transfer_picker_prev(&mut self) {
        self.state.transactions.picker_index = (self.state.transactions.picker_index + 1) % 2;
    }

    pub(crate) fn start_transfer_wallet(&mut self) {
        let occurred_at_str = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.transfer = TransferFormState {
            occurred_at: DateField::with_value(occurred_at_str),
            ..TransferFormState::default()
        };
        self.state.transactions.mode = TransactionsMode::TransferWallet;
        self.init_transfer_indices();
    }

    pub(crate) fn start_transfer_flow(&mut self) {
        let occurred_at_str = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.transfer = TransferFormState {
            occurred_at: DateField::with_value(occurred_at_str),
            ..TransferFormState::default()
        };
        self.state.transactions.mode = TransactionsMode::TransferFlow;
        self.init_transfer_indices();
    }

    pub(crate) fn init_transfer_indices(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            self.state.transactions.transfer.error =
                Some(t(self.state.locale, TextKey::ValidationNoElementAvailable).to_string());
            return;
        }
        self.state.transactions.transfer.from_index = 0;
        self.state.transactions.transfer.to_index = if len > 1 { 1 } else { 0 };
    }

    pub(crate) fn transfer_select_next(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            return;
        }
        match self.state.transactions.transfer.focus {
            TransferField::From => {
                self.state.transactions.transfer.from_index =
                    (self.state.transactions.transfer.from_index + 1) % len;
            }
            TransferField::To => {
                self.state.transactions.transfer.to_index =
                    (self.state.transactions.transfer.to_index + 1) % len;
            }
            _ => {}
        }
    }

    pub(crate) fn transfer_select_prev(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            return;
        }
        match self.state.transactions.transfer.focus {
            TransferField::From => {
                self.state.transactions.transfer.from_index =
                    (self.state.transactions.transfer.from_index + len - 1) % len;
            }
            TransferField::To => {
                self.state.transactions.transfer.to_index =
                    (self.state.transactions.transfer.to_index + len - 1) % len;
            }
            _ => {}
        }
    }
}
