use super::super::*;

use crate::{error::Result, text::format as t_format};
use api_types::{
    transaction::TransactionList,
    wallet::{WalletNew, WalletUpdate},
};
use engine::Money;

impl App {
    pub(crate) async fn open_wallet_detail(&mut self) -> Result<()> {
        let Some(wallet_id) = self.selected_wallet().map(|wallet| wallet.id) else {
            self.state.wallets.error =
                Some(t(self.state.locale, TextKey::ValidationNoWalletSelected).to_string());
            return Ok(());
        };
        self.state.wallets.detail.wallet_id = Some(wallet_id);
        self.state.wallets.mode = EntityListMode::Detail;
        self.load_wallet_transactions(wallet_id).await?;
        Ok(())
    }
    pub(crate) async fn load_wallet_transactions(&mut self, wallet_id: uuid::Uuid) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let payload = TransactionList {
            vault_id,
            flow_id: None,
            wallet_id: Some(wallet_id),
            limit: Some(10),
            cursor: None,
            from: None,
            to: None,
            kinds: None,
            include_voided: Some(false),
            include_transfers: Some(false),
        };
        let res = self.client.transactions_list(payload).await;

        match res {
            Ok(list) => {
                self.state.wallets.detail.transactions = list.transactions;
                self.state.wallets.detail.error = None;
                self.connection_ok(None);
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_connection(err) else {
                    return Ok(());
                };
                self.state.wallets.detail.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn submit_wallet_create(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;

        // Validate the form
        if let Some(err) = self.state.wallets.form.validate_all() {
            self.state.wallets.error = Some(err);
            return Ok(());
        }

        let name = self.state.wallets.form.name.value().trim();
        if name.is_empty() {
            self.state.wallets.error =
                Some(t(self.state.locale, TextKey::PromptEnterName).to_string());
            return Ok(());
        }

        let currency = self.current_currency();
        let opening_raw = self.state.wallets.form.opening.value().trim();
        let opening_raw = if opening_raw.is_empty() {
            "0"
        } else {
            opening_raw
        };
        let opening = match Money::parse_major(opening_raw, currency) {
            Ok(money) => money.minor(),
            Err(_) => {
                self.state.wallets.error = Some(
                    t(self.state.locale, TextKey::ValidationOpeningBalanceInvalid).to_string(),
                );
                return Ok(());
            }
        };

        let res = self
            .client
            .wallet_new(WalletNew {
                vault_id,
                name: name.to_string(),
                opening_balance_minor: opening,
                occurred_at: self.now_in_timezone(),
            })
            .await;

        match res {
            Ok(created) => {
                self.reset_wallet_form();
                self.state.wallets.mode = EntityListMode::List;
                self.refresh_snapshot().await?;
                self.select_wallet_by_id(created.id);
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessWalletCreated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorCreateWallet) else {
                    return Ok(());
                };
                self.state.wallets.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn submit_wallet_rename(&mut self) -> Result<()> {
        let Some(wallet_id) = self.selected_wallet().map(|w| w.id) else {
            self.state.wallets.error =
                Some(t(self.state.locale, TextKey::ValidationNoWalletSelected).to_string());
            return Ok(());
        };

        // Validate the form
        if let Some(err) = self.state.wallets.form.validate_all() {
            self.state.wallets.error = Some(err);
            return Ok(());
        }

        let name = self.state.wallets.form.name.value().trim();
        if name.is_empty() {
            self.state.wallets.error =
                Some(t(self.state.locale, TextKey::PromptEnterName).to_string());
            return Ok(());
        }

        let res = self
            .client
            .wallet_update(
                wallet_id,
                WalletUpdate {
                    vault_id: self.current_vault_id()?,
                    name: Some(name.to_string()),
                    archived: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.reset_wallet_form();
                self.state.wallets.mode = EntityListMode::List;
                self.refresh_snapshot().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessWalletUpdated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorUpdateWallet) else {
                    return Ok(());
                };
                self.state.wallets.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn toggle_wallet_archive(&mut self) -> Result<()> {
        let Some(wallet) = self.selected_wallet() else {
            self.state.wallets.error =
                Some(t(self.state.locale, TextKey::ValidationNoWalletSelected).to_string());
            return Ok(());
        };
        let res = self
            .client
            .wallet_update(
                wallet.id,
                WalletUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(!wallet.archived),
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.refresh_snapshot().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessWalletUpdated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorArchiveWallet) else {
                    return Ok(());
                };
                self.state.wallets.error = Some(msg);
            }
        }

        Ok(())
    }

    pub(crate) async fn archive_wallet_with_undo(&mut self) -> Result<()> {
        self.finalize_pending_undo().await?;
        let Some(wallet) = self.selected_wallet() else {
            self.state.wallets.error =
                Some(t(self.state.locale, TextKey::ValidationNoWalletSelected).to_string());
            return Ok(());
        };
        let wallet_id = wallet.id;
        let wallet_name = wallet.name.clone();
        let is_archived = wallet.archived;

        if is_archived {
            self.state.wallets.error =
                Some(t(self.state.locale, TextKey::ValidationAlreadyArchived).to_string());
            return Ok(());
        }

        let res = self
            .client
            .wallet_update(
                wallet_id,
                WalletUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(true),
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.refresh_snapshot().await?;
                let message = t_format(
                    self.state.locale,
                    TextKey::SuccessDeletedWallet,
                    &[("name", &wallet_name)],
                );
                self.set_undo_toast(&message, UndoAction::WalletArchive { id: wallet_id });
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorArchiveWallet) else {
                    return Ok(());
                };
                self.state.wallets.error = Some(msg);
            }
        }

        Ok(())
    }

    pub(crate) async fn undo_wallet_archive(&mut self, wallet_id: uuid::Uuid) -> Result<()> {
        let res = self
            .client
            .wallet_update(
                wallet_id,
                WalletUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(false),
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.refresh_snapshot().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessWalletRestored),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorRestoreWallet) else {
                    return Ok(());
                };
                self.state.wallets.error = Some(msg);
            }
        }

        Ok(())
    }
}
