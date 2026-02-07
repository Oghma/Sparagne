//! Transaction submission actions - split from the monolithic
//! transactions_submit.rs
//!
//! This module handles all transaction submission-related actions:
//! - `form`: Transaction form submission and editing
//! - `transfers`: Wallet and flow transfer submissions
//! - `filter`: Transaction list filtering
//! - `quick_add`: Quick-add parsing and submission
//! - `void`: Transaction voiding and bulk operations

mod filter;
mod form;
mod quick_add;
mod transfers;
mod void;

use crate::{app::App, error::Result};

impl App {
    /// Refresh transactions, wallet/flow balances, and stats after a mutation.
    pub(crate) async fn refresh_after_transaction_mutation(&mut self) -> Result<()> {
        self.load_transactions(true).await?;
        self.refresh_snapshot().await?;
        Ok(())
    }
}
