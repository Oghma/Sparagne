//! Transactions section dispatch handling.

use crate::{
    app::{
        App,
        state::{FilterField, TransactionsMode, TransferField},
    },
    error::Result,
    ui::keymap::AppAction,
};

impl App {
    /// Dispatches actions for the Transactions section.
    pub(crate) async fn dispatch_transactions(&mut self, action: AppAction) -> Result<bool> {
        match action {
            AppAction::Submit => {
                self.handle_transactions_submit().await?;
                Ok(true)
            }
            AppAction::Backspace => self.dispatch_transactions_backspace().await,
            AppAction::Up => self.dispatch_transactions_up().await,
            AppAction::Down => self.dispatch_transactions_down().await,
            _ => Ok(false),
        }
    }

    async fn dispatch_transactions_backspace(&mut self) -> Result<bool> {
        // Search backspace takes priority
        if self.handle_search_backspace().await? {
            return Ok(true);
        }

        match self.state.transactions.mode {
            TransactionsMode::Form | TransactionsMode::Edit => {
                self.backspace_transaction_form();
                Ok(true)
            }
            TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                match self.state.transactions.transfer.focus {
                    TransferField::Amount => {
                        self.state.transactions.transfer.amount.pop();
                    }
                    TransferField::Note => {
                        self.state.transactions.transfer.note.pop();
                    }
                    TransferField::OccurredAt => {
                        self.state.transactions.transfer.occurred_at.pop();
                    }
                    _ => {}
                }
                Ok(true)
            }
            TransactionsMode::Filter => {
                match self.state.transactions.filter.focus {
                    FilterField::From => {
                        self.state.transactions.filter.from_input.pop();
                    }
                    FilterField::To => {
                        self.state.transactions.filter.to_input.pop();
                    }
                    FilterField::Kinds | FilterField::Transfers => {}
                }
                Ok(true)
            }
            TransactionsMode::List if self.state.transactions.quick_active => {
                self.state.transactions.quick_input.pop();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn dispatch_transactions_up(&mut self) -> Result<bool> {
        match self.state.transactions.mode {
            TransactionsMode::List | TransactionsMode::Detail => {
                self.state.transactions.select_prev();
                if self.state.transactions.mode == TransactionsMode::Detail {
                    self.open_transaction_detail().await?;
                }
                Ok(true)
            }
            TransactionsMode::PickWallet | TransactionsMode::PickFlow => {
                self.transactions_picker_prev();
                Ok(true)
            }
            TransactionsMode::TransferPicker => {
                self.transfer_picker_prev();
                Ok(true)
            }
            TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                self.transfer_select_prev();
                Ok(true)
            }
            TransactionsMode::Form | TransactionsMode::Edit => {
                self.transaction_form_select_prev();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn dispatch_transactions_down(&mut self) -> Result<bool> {
        match self.state.transactions.mode {
            TransactionsMode::List | TransactionsMode::Detail => {
                self.state.transactions.select_next();
                if self.state.transactions.mode == TransactionsMode::Detail {
                    self.open_transaction_detail().await?;
                }
                Ok(true)
            }
            TransactionsMode::PickWallet | TransactionsMode::PickFlow => {
                self.transactions_picker_next();
                Ok(true)
            }
            TransactionsMode::TransferPicker => {
                self.transfer_picker_next();
                Ok(true)
            }
            TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                self.transfer_select_next();
                Ok(true)
            }
            TransactionsMode::Form | TransactionsMode::Edit => {
                self.transaction_form_select_next();
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
