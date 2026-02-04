//! Visual mode operations for transactions.
//!
//! Contains methods for visual selection mode and bulk operations like delete.

use crate::{
    app::{
        format::map_currency, transactions_visible_indices, App, TransactionsMode, UndoAction,
    },
    error::Result,
    text::{TextKey, format as t_format, t},
};
use engine::Money;

impl App {
    pub(crate) fn toggle_visual_mode(&mut self) {
        if self.state.transactions.visual_mode {
            self.state.transactions.visual_mode = false;
            self.state.transactions.visual_selected.clear();
        } else {
            self.state.transactions.visual_mode = true;
            self.state.transactions.visual_selected.clear();
            self.state.transactions.quick_active = false;
            self.state.transactions.quick_input.clear();
            self.state.transactions.quick_error = None;
        }
    }

    pub(crate) fn exit_visual_mode(&mut self) {
        if self.state.transactions.visual_mode {
            self.state.transactions.visual_mode = false;
            self.state.transactions.visual_selected.clear();
        }
    }

    pub(crate) fn toggle_visual_selection(&mut self) {
        let Some(tx_id) = self.selected_transaction().map(|tx| tx.id) else {
            return;
        };
        if self.state.transactions.visual_selected.contains(&tx_id) {
            self.state.transactions.visual_selected.remove(&tx_id);
        } else {
            self.state.transactions.visual_selected.insert(tx_id);
        }
    }

    pub(crate) async fn queue_transaction_delete(&mut self) -> Result<()> {
        self.finalize_pending_undo().await?;
        let visual_mode = self.state.transactions.visual_mode;
        let visual_ids = if visual_mode {
            self.state
                .transactions
                .visual_selected
                .iter()
                .copied()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let selected_tx_id = self.selected_transaction().map(|tx| tx.id);
        let Some(tx_id) = selected_tx_id else {
            self.state.transactions.error = Some(t(self.state.locale, TextKey::ValidationNoTransactionSelected).to_string());
            return Ok(());
        };

        let ids = if visual_mode && !visual_ids.is_empty() {
            visual_ids
        } else {
            vec![tx_id]
        };

        let single_info = if ids.len() == 1 {
            self.state
                .transactions
                .items
                .iter()
                .find(|tx| tx.id == ids[0])
                .map(|tx| (tx.amount_minor, tx.note.clone()))
        } else {
            None
        };

        if self.state.transactions.mode == TransactionsMode::Detail {
            self.state.transactions.mode = TransactionsMode::List;
            self.state.transactions.detail = None;
        }

        for id in &ids {
            self.state.transactions.pending_delete_ids.insert(*id);
            self.state.transactions.visual_selected.remove(id);
        }
        if visual_mode {
            self.exit_visual_mode();
        }
        let visible_len = transactions_visible_indices(&self.state).len();
        if visible_len == 0 {
            self.state.transactions.selected = 0;
        } else if self.state.transactions.selected >= visible_len {
            self.state.transactions.selected = visible_len - 1;
        }

        let currency = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur);
        let message = if let Some((amount_minor, note)) = single_info {
            let amount = Money::new(amount_minor).format(currency);
            let label = note.as_deref().unwrap_or("Transaction");
            t_format(self.state.locale, TextKey::SuccessDeletedItem, &[("label", label), ("amount", &amount)])
        } else {
            t_format(self.state.locale, TextKey::SuccessDeletedMultiple, &[("count", &ids.len().to_string())])
        };

        self.set_undo_toast(&message, UndoAction::TransactionVoid { ids });
        Ok(())
    }
}
