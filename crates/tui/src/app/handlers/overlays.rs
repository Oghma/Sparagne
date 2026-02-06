use super::super::*;

use crate::{error::Result, text::{TextKey, format as t_format, t}, ui::keymap::AppAction};
use engine::Money;

impl App {
    pub(crate) fn maybe_open_discard_dialog(&mut self) -> bool {
        if self.state.section != Section::Transactions {
            return false;
        }

        match self.state.transactions.mode {
            TransactionsMode::Form | TransactionsMode::Edit => {
                if !self.state.transactions.form.is_dirty() {
                    return false;
                }
                let locale = self.state.locale;
                self.state.overlays.confirm = Some(ConfirmDialogState::discard_changes(
                    t(locale, TextKey::DialogUnsavedChangesTitle),
                    t(locale, TextKey::DialogUnsavedChangesMessage),
                    t(locale, TextKey::DialogSave),
                    t(locale, TextKey::DialogDiscard),
                    ConfirmAction::SubmitTransactionForm,
                    ConfirmAction::DiscardTransactionForm,
                ));
                true
            }
            TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                if !self.state.transactions.transfer.is_dirty() {
                    return false;
                }
                let locale = self.state.locale;
                self.state.overlays.confirm = Some(ConfirmDialogState::discard_changes(
                    t(locale, TextKey::DialogUnsavedChangesTitle),
                    t(locale, TextKey::DialogUnsavedChangesMessage),
                    t(locale, TextKey::DialogSave),
                    t(locale, TextKey::DialogDiscard),
                    ConfirmAction::SubmitTransferForm,
                    ConfirmAction::DiscardTransferForm,
                ));
                true
            }
            _ => false,
        }
    }

    pub(crate) fn open_vault_delete_dialog(&mut self) {
        let locale = self.state.locale;
        let name = self
            .state
            .vault
            .as_ref()
            .and_then(|vault| vault.name.as_deref())
            .unwrap_or(t(locale, TextKey::DialogThisVault));
        let preview = vec![format!("🏦 {name}")];
        self.state.overlays.confirm = Some(ConfirmDialogState::delete(
            t(locale, TextKey::DialogDeleteVaultTitle),
            format!("Delete \"{name}\"?"),
            t(locale, TextKey::DialogDeleteVaultWarning),
            preview,
            t(locale, TextKey::DialogDelete),
            ConfirmAction::DeleteVault,
        ));
    }

    pub(crate) fn open_transaction_delete_dialog(&mut self) {
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

        let locale = self.state.locale;
        if visual_mode && !visual_ids.is_empty() {
            let count = visual_ids.len();
            let count_str = count.to_string();
            let preview = vec![format!("🧾 {}", t_format(locale, TextKey::SuccessDeletedMultiple, &[("count", &count_str)]))];
            self.state.overlays.confirm = Some(ConfirmDialogState::delete(
                t(locale, TextKey::DialogDeleteTransactionsTitle),
                format!("Delete {count} transactions?"),
                t(locale, TextKey::DialogDeleteUndoHint),
                preview,
                t(locale, TextKey::DialogDelete),
                ConfirmAction::DeleteTransaction,
            ));
            return;
        }

        let Some((amount_minor, note)) = self
            .selected_transaction()
            .map(|tx| (tx.amount_minor, tx.note.clone()))
        else {
            self.state.transactions.error = Some(t(self.state.locale, TextKey::ValidationNoTransactionSelected).to_string());
            return;
        };

        let amount = Money::new(amount_minor).format(self.current_currency());
        let note = note.as_deref().unwrap_or(t(locale, TextKey::DialogTransaction));
        let preview = vec![format!("🧾 {note}  {amount}")];
        self.state.overlays.confirm = Some(ConfirmDialogState::delete(
            t(locale, TextKey::DialogDeleteTransactionTitle),
            format!("Delete \"{note}\"?"),
            t(locale, TextKey::DialogDeleteUndoHint),
            preview,
            t(locale, TextKey::DialogDelete),
            ConfirmAction::DeleteTransaction,
        ));
    }

    pub(crate) fn open_bulk_category_dialog(&mut self) {
        let visual_mode = self.state.transactions.visual_mode;
        let ids = if visual_mode && !self.state.transactions.visual_selected.is_empty() {
            self.state
                .transactions
                .visual_selected
                .iter()
                .copied()
                .collect::<Vec<_>>()
        } else if let Some(id) = self.selected_transaction().map(|tx| tx.id) {
            vec![id]
        } else {
            Vec::new()
        };

        if ids.is_empty() {
            self.state.transactions.error = Some(t(self.state.locale, TextKey::ValidationNoTransactionSelected).to_string());
            return;
        }

        let count = ids.len();
        self.state.overlays.bulk_category = Some(BulkCategoryDialogState {
            transaction_ids: ids,
            count,
            input: String::new(),
            error: None,
        });
    }

    pub(crate) fn open_grouping_dialog(&mut self) {
        if self.state.section != Section::Transactions
            || self.state.transactions.mode != TransactionsMode::List
        {
            return;
        }
        self.state.overlays.grouping = Some(GroupingDialogState {
            selected: self.state.transactions.grouping_mode.index(),
        });
    }

    pub(crate) fn open_wallet_archive_dialog(&mut self) {
        let locale = self.state.locale;
        let Some(wallet) = self.selected_wallet() else {
            self.state.wallets.error = Some(t(locale, TextKey::ValidationNoWalletSelected).to_string());
            return;
        };
        let name = wallet.name.as_str();
        let preview = vec![format!("💰 {name}")];
        self.state.overlays.confirm = Some(ConfirmDialogState::archive(
            t(locale, TextKey::DialogDeleteWalletTitle),
            format!("Delete \"{name}\"?"),
            t(locale, TextKey::DialogDeleteWalletHint),
            preview,
            t(locale, TextKey::DialogDelete),
            ConfirmAction::ArchiveWalletWithUndo,
        ));
    }

    pub(crate) fn open_flow_archive_dialog(&mut self) {
        let locale = self.state.locale;
        let Some(flow) = self.selected_flow() else {
            self.state.flows.error = Some(t(locale, TextKey::ValidationNoFlowSelected).to_string());
            return;
        };
        let name = flow.name.as_str();
        let preview = vec![format!("📦 {name}")];
        self.state.overlays.confirm = Some(ConfirmDialogState::archive(
            t(locale, TextKey::DialogDeleteFlowTitle),
            format!("Delete \"{name}\"?"),
            t(locale, TextKey::DialogDeleteFlowHint),
            preview,
            t(locale, TextKey::DialogDelete),
            ConfirmAction::ArchiveFlowWithUndo,
        ));
    }

    pub(crate) fn open_category_archive_dialog(&mut self) {
        let locale = self.state.locale;
        let Some(category) = self.selected_category() else {
            self.state.categories.error = Some(t(locale, TextKey::PromptNoCategorySelected).to_string());
            return;
        };
        let name = category.name.as_str();
        let preview = vec![format!("🏷️ {name}")];
        self.state.overlays.confirm = Some(ConfirmDialogState::archive(
            t(locale, TextKey::DialogDeleteCategoryTitle),
            format!("Delete \"{name}\"?"),
            t(locale, TextKey::DialogDeleteCategoryHint),
            preview,
            t(locale, TextKey::DialogDelete),
            ConfirmAction::ToggleCategoryArchive,
        ));
    }

    pub(crate) async fn handle_confirm_action(&mut self, action: AppAction) -> Result<()> {
        let Some(dialog) = self.state.overlays.confirm.take() else {
            return Ok(());
        };

        match action {
            AppAction::Cancel => {}
            AppAction::Submit => {
                let action = dialog.confirm_action;
                self.run_confirm_action(action).await?;
            }
            AppAction::Input('d' | 'D') if dialog.kind == ConfirmDialogKind::DiscardChanges => {
                if let Some(action) = dialog.extra_action {
                    self.run_confirm_action(action).await?;
                } else {
                    self.state.overlays.confirm = Some(dialog);
                }
            }
            _ => {
                self.state.overlays.confirm = Some(dialog);
            }
        }

        Ok(())
    }

    pub(crate) async fn handle_error_action(&mut self, action: AppAction) -> Result<()> {
        let Some(dialog) = self.state.overlays.error.take() else {
            return Ok(());
        };

        match action {
            AppAction::Cancel => {}
            AppAction::Submit => {
                if let Some(action) = dialog.retry_action {
                    self.run_error_action(action).await?;
                }
            }
            AppAction::Input('r' | 'R') => {
                if let Some(action) = dialog.retry_action {
                    self.run_error_action(action).await?;
                } else {
                    self.state.overlays.error = Some(dialog);
                }
            }
            _ => {
                self.state.overlays.error = Some(dialog);
            }
        }

        Ok(())
    }

    pub(crate) async fn handle_bulk_category_action(&mut self, action: AppAction) -> Result<()> {
        let Some(mut dialog) = self.state.overlays.bulk_category.take() else {
            return Ok(());
        };

        match action {
            AppAction::Cancel => {}
            AppAction::Backspace => {
                dialog.input.pop();
                dialog.error = None;
                self.state.overlays.bulk_category = Some(dialog);
            }
            AppAction::Input(ch) => {
                dialog.input.push(ch);
                dialog.error = None;
                self.state.overlays.bulk_category = Some(dialog);
            }
            AppAction::Submit => {
                self.state.overlays.bulk_category = Some(dialog);
                self.apply_bulk_category().await?;
            }
            _ => {
                self.state.overlays.bulk_category = Some(dialog);
            }
        }

        Ok(())
    }

    pub(crate) async fn handle_grouping_action(&mut self, action: AppAction) -> Result<()> {
        let Some(mut dialog) = self.state.overlays.grouping.take() else {
            return Ok(());
        };

        match action {
            AppAction::Cancel => {}
            AppAction::Up | AppAction::Left => {
                dialog.selected = GroupingMode::from_index(dialog.selected).prev().index();
                self.state.overlays.grouping = Some(dialog);
            }
            AppAction::Down | AppAction::Right => {
                dialog.selected = GroupingMode::from_index(dialog.selected).next().index();
                self.state.overlays.grouping = Some(dialog);
            }
            AppAction::Submit => {
                self.apply_grouping_mode(GroupingMode::from_index(dialog.selected));
            }
            AppAction::Input(ch) => {
                let mode = match ch {
                    'd' | 'D' | '1' => Some(GroupingMode::Date),
                    'c' | 'C' | '2' => Some(GroupingMode::Category),
                    'w' | 'W' | '3' => Some(GroupingMode::Wallet),
                    'e' | 'E' | '4' => Some(GroupingMode::Envelope),
                    _ => None,
                };
                if let Some(mode) = mode {
                    self.apply_grouping_mode(mode);
                } else {
                    self.state.overlays.grouping = Some(dialog);
                }
            }
            AppAction::Backspace
            | AppAction::None
            | AppAction::NextField
            | AppAction::CycleAmbiguous => {
                self.state.overlays.grouping = Some(dialog);
            }
            AppAction::Search | AppAction::TogglePalette | AppAction::Quit => {}
        }

        Ok(())
    }

    async fn apply_bulk_category(&mut self) -> Result<()> {
        let Some(mut dialog) = self.state.overlays.bulk_category.take() else {
            return Ok(());
        };

        let category = dialog.input.trim().trim_start_matches('#').trim();
        if category.is_empty() {
            dialog.error = Some(t(self.state.locale, TextKey::PromptEnterCategory).to_string());
            self.state.overlays.bulk_category = Some(dialog);
            return Ok(());
        }

        let category = category.to_string();
        self.finalize_pending_undo().await?;
        self.bulk_categorize_transactions(&dialog.transaction_ids, &category)
            .await?;
        Ok(())
    }

    fn apply_grouping_mode(&mut self, mode: GroupingMode) {
        self.state.transactions.grouping_mode = mode;
        self.state.overlays.grouping = None;

        // Clamp selection after regrouping.
        let len = transactions_visible_indices(&self.state).len();
        if len == 0 {
            self.state.transactions.selected = 0;
        } else if self.state.transactions.selected >= len {
            self.state.transactions.selected = len - 1;
        }
    }

    async fn run_confirm_action(&mut self, action: ConfirmAction) -> Result<()> {
        match action {
            ConfirmAction::DeleteTransaction => {
                self.queue_transaction_delete().await?;
            }
            ConfirmAction::DeleteVault => {
                self.delete_vault().await?;
            }
            ConfirmAction::ArchiveWalletWithUndo => {
                self.archive_wallet_with_undo().await?;
            }
            ConfirmAction::ArchiveFlowWithUndo => {
                self.archive_flow_with_undo().await?;
            }
            ConfirmAction::ToggleCategoryArchive => {
                self.toggle_category_archive().await?;
            }
            ConfirmAction::DiscardTransactionForm => {
                self.discard_transaction_form();
            }
            ConfirmAction::DiscardTransferForm => {
                self.discard_transfer_form();
            }
            ConfirmAction::SubmitTransactionForm | ConfirmAction::SubmitTransferForm => {
                self.handle_transactions_submit().await?;
            }
        }

        Ok(())
    }

    async fn run_error_action(&mut self, action: ErrorAction) -> Result<()> {
        match action {
            ErrorAction::RetrySnapshot => {
                self.refresh_snapshot().await?;
                if self.state.section == Section::Transactions {
                    self.load_transactions(true).await?;
                }
            }
        }

        Ok(())
    }

    fn discard_transaction_form(&mut self) {
        if self.state.transactions.form.editing_id.is_some() {
            self.state.transactions.mode = TransactionsMode::Detail;
        } else {
            self.state.transactions.mode = TransactionsMode::List;
        }
        self.state.transactions.form = TransactionFormState::default();
    }

    fn discard_transfer_form(&mut self) {
        if self.state.transactions.transfer.editing_id.is_some() {
            self.state.transactions.mode = TransactionsMode::Detail;
        } else {
            self.state.transactions.mode = TransactionsMode::List;
        }
        self.state.transactions.transfer = TransferFormState::default();
    }
}
