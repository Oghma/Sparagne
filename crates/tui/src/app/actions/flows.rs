use super::super::*;

use crate::{error::Result, text::format as t_format};
use api_types::{
    flow::{FlowMode, FlowNew, FlowUpdate},
    transaction::TransactionList,
};
use engine::Money;

impl App {
    pub(crate) async fn open_flow_detail(&mut self) -> Result<()> {
        let Some(flow_id) = self.selected_flow().map(|flow| flow.id) else {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::ValidationNoFlowSelected).to_string());
            return Ok(());
        };
        self.state.flows.detail.flow_id = Some(flow_id);
        self.state.flows.mode = EntityListMode::Detail;
        self.load_flow_transactions(flow_id).await?;
        self.load_flow_detail(flow_id).await?;
        Ok(())
    }
    pub(crate) async fn load_flow_transactions(&mut self, flow_id: uuid::Uuid) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let payload = TransactionList {
            vault_id,
            flow_id: Some(flow_id),
            wallet_id: None,
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
                self.state.flows.detail.transactions = list.transactions;
                self.state.flows.detail.error = None;
                self.connection_ok(None);
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_connection(err) else {
                    return Ok(());
                };
                self.state.flows.detail.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn load_flow_detail(&mut self, flow_id: uuid::Uuid) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let res = self
            .client
            .cash_flow_get(api_types::cash_flow::CashFlowGet {
                vault_id,
                id: Some(flow_id),
                name: None,
            })
            .await;

        match res {
            Ok(flow) => {
                self.state.flows.detail.detail = Some(flow);
                self.connection_ok(None);
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_connection(err) else {
                    return Ok(());
                };
                self.state.flows.detail.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn submit_flow_create(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;

        // Validate the form
        if let Some(err) = self.state.flows.form.validate_all() {
            self.state.flows.error = Some(err);
            return Ok(());
        }

        let name = self.state.flows.form.name.value().trim().to_string();
        if name.is_empty() {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::PromptEnterName).to_string());
            return Ok(());
        }

        let currency = self.current_currency();
        let opening_raw = self.state.flows.form.opening.value().trim();
        let opening_raw = if opening_raw.is_empty() {
            "0"
        } else {
            opening_raw
        };
        let opening = match Money::parse_major(opening_raw, currency) {
            Ok(money) => money.minor(),
            Err(_) => {
                self.state.flows.error = Some(
                    t(self.state.locale, TextKey::ValidationOpeningBalanceInvalid).to_string(),
                );
                return Ok(());
            }
        };
        if opening < 0 {
            self.state.flows.error = Some(
                t(
                    self.state.locale,
                    TextKey::ValidationOpeningBalanceNonNegative,
                )
                .to_string(),
            );
            return Ok(());
        }

        let mode = match self.state.flows.form.mode {
            FlowModeChoice::Unlimited => FlowMode::Unlimited,
            FlowModeChoice::NetCapped => {
                let cap = match self.parse_flow_cap(currency) {
                    Some(cap) => cap,
                    None => return Ok(()),
                };
                FlowMode::NetCapped { cap_minor: cap }
            }
            FlowModeChoice::IncomeCapped => {
                let cap = match self.parse_flow_cap(currency) {
                    Some(cap) => cap,
                    None => return Ok(()),
                };
                FlowMode::IncomeCapped { cap_minor: cap }
            }
        };

        let res = self
            .client
            .flow_new(FlowNew {
                vault_id,
                name,
                mode,
                opening_balance_minor: opening,
                occurred_at: self.now_in_timezone(),
                allow_negative: self.state.flows.form.allow_negative,
            })
            .await;

        match res {
            Ok(created) => {
                self.reset_flow_form();
                self.state.flows.mode = EntityListMode::List;
                self.refresh_snapshot().await?;
                self.select_flow_by_id(created.id);
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessFlowCreated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.client_error_message(err) else {
                    return Ok(());
                };
                self.set_toast(&msg, ToastLevel::Error);
                self.state.flows.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn submit_flow_rename(&mut self) -> Result<()> {
        let Some((flow_id, is_unallocated)) =
            self.selected_flow().map(|f| (f.id, f.is_unallocated))
        else {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::ValidationNoFlowSelected).to_string());
            return Ok(());
        };
        if is_unallocated {
            self.state.flows.error = Some(
                t(
                    self.state.locale,
                    TextKey::ValidationUnallocatedCannotRename,
                )
                .to_string(),
            );
            return Ok(());
        }

        // Validate the form
        if let Some(err) = self.state.flows.form.validate_all() {
            self.state.flows.error = Some(err);
            return Ok(());
        }

        let name = self.state.flows.form.name.value().trim();
        if name.is_empty() {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::PromptEnterName).to_string());
            return Ok(());
        }

        let res = self
            .client
            .flow_update(
                flow_id,
                FlowUpdate {
                    vault_id: self.current_vault_id()?,
                    name: Some(name.to_string()),
                    archived: None,
                    mode: None,
                    allow_negative: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.reset_flow_form();
                self.state.flows.mode = EntityListMode::List;
                self.refresh_snapshot().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessFlowUpdated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.client_error_message(err) else {
                    return Ok(());
                };
                self.set_toast(&msg, ToastLevel::Error);
                self.state.flows.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) async fn toggle_flow_archive(&mut self) -> Result<()> {
        let Some(flow) = self.selected_flow() else {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::ValidationNoFlowSelected).to_string());
            return Ok(());
        };
        if flow.is_unallocated {
            self.state.flows.error = Some(
                t(
                    self.state.locale,
                    TextKey::ValidationUnallocatedCannotArchive,
                )
                .to_string(),
            );
            return Ok(());
        }
        let res = self
            .client
            .flow_update(
                flow.id,
                FlowUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(!flow.archived),
                    mode: None,
                    allow_negative: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.refresh_snapshot().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessFlowUpdated),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorArchiveFlow) else {
                    return Ok(());
                };
                self.state.flows.error = Some(msg);
            }
        }

        Ok(())
    }

    pub(crate) async fn archive_flow_with_undo(&mut self) -> Result<()> {
        self.finalize_pending_undo().await?;
        let Some(flow) = self.selected_flow() else {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::ValidationNoFlowSelected).to_string());
            return Ok(());
        };
        let flow_id = flow.id;
        let flow_name = flow.name.clone();
        let is_unallocated = flow.is_unallocated;
        let is_archived = flow.archived;

        if is_unallocated {
            self.state.flows.error = Some(
                t(
                    self.state.locale,
                    TextKey::ValidationUnallocatedCannotArchive,
                )
                .to_string(),
            );
            return Ok(());
        }
        if is_archived {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::ValidationAlreadyArchived).to_string());
            return Ok(());
        }

        let res = self
            .client
            .flow_update(
                flow_id,
                FlowUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(true),
                    mode: None,
                    allow_negative: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.refresh_snapshot().await?;
                let message = t_format(
                    self.state.locale,
                    TextKey::SuccessDeletedFlow,
                    &[("name", &flow_name)],
                );
                self.set_undo_toast(&message, UndoAction::FlowArchive { id: flow_id });
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorArchiveFlow) else {
                    return Ok(());
                };
                self.state.flows.error = Some(msg);
            }
        }

        Ok(())
    }

    pub(crate) async fn undo_flow_archive(&mut self, flow_id: uuid::Uuid) -> Result<()> {
        let res = self
            .client
            .flow_update(
                flow_id,
                FlowUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(false),
                    mode: None,
                    allow_negative: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.refresh_snapshot().await?;
                self.set_toast(
                    t(self.state.locale, TextKey::SuccessFlowRestored),
                    ToastLevel::Success,
                );
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorRestoreFlow) else {
                    return Ok(());
                };
                self.state.flows.error = Some(msg);
            }
        }

        Ok(())
    }
    pub(crate) fn parse_flow_cap(&mut self, currency: engine::Currency) -> Option<i64> {
        let cap_raw = self.state.flows.form.cap.value().trim();
        if cap_raw.is_empty() {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::PromptEnterCap).to_string());
            return None;
        }
        let cap = match Money::parse_major(cap_raw, currency) {
            Ok(money) => money.minor().abs(),
            Err(_) => {
                self.state.flows.error =
                    Some(t(self.state.locale, TextKey::ValidationCapInvalid).to_string());
                return None;
            }
        };
        if cap <= 0 {
            self.state.flows.error =
                Some(t(self.state.locale, TextKey::ValidationCapMustBePositive).to_string());
            return None;
        }
        Some(cap)
    }
}
