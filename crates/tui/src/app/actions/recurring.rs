use super::super::*;

use crate::error::{AppError, Result};
use api_types::recurring::{
    PendingRecurringList, RecurringExecute, RecurringTemplateArchive, RecurringTemplateList,
    RecurringTemplateNew, RecurringTemplateUpdate,
};

impl App {
    pub(crate) async fn load_recurring(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;

        let list_res = self
            .client
            .recurring_list(RecurringTemplateList {
                vault_id: vault_id.clone(),
                include_archived: false,
            })
            .await;

        let pending_res = self
            .client
            .recurring_pending(PendingRecurringList {
                vault_id: vault_id.clone(),
            })
            .await;

        match (list_res, pending_res) {
            (Ok(list), Ok(pending)) => {
                self.state.recurring.templates = list.templates;
                self.state.recurring.pending = pending.pending;
                self.state.recurring.pending_count = self.state.recurring.pending.len();
                self.state.recurring.error = None;
                self.state.recurring.selected = 0;
                self.connection_ok(None);
            }
            (Err(err), _) | (_, Err(err)) => {
                let Some(msg) = self.on_api_error_connection(err) else {
                    return Ok(());
                };
                self.state.recurring.error = Some(msg);
            }
        }

        Ok(())
    }


    pub(crate) async fn create_recurring(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let form = &self.state.recurring.form;

        let amount_str = form.amount.value().trim().to_string();
        let amount_f: f64 = amount_str
            .parse()
            .map_err(|_| AppError::Terminal("invalid amount".to_string()))?;
        let amount_minor = (amount_f * 100.0) as i64;

        if amount_minor <= 0 {
            self.state.recurring.form.error =
                Some(t(self.state.locale, TextKey::ValidationAmountPositive).to_string());
            return Ok(());
        }

        let day_str = form.day_of_period.value().trim().to_string();
        let day: i32 = day_str.parse().unwrap_or(1);

        let start_date = form.start_date.value().trim().to_string();
        if start_date.is_empty() {
            self.state.recurring.form.error =
                Some(t(self.state.locale, TextKey::ValidationDateRequired).to_string());
            return Ok(());
        }

        let end_date_raw = form.end_date.value().trim().to_string();
        let end_date = if end_date_raw.is_empty() {
            None
        } else {
            Some(end_date_raw)
        };

        let category = form.category.value().trim().to_string();
        let note_raw = form.note.value().trim().to_string();
        let note = if note_raw.is_empty() {
            None
        } else {
            Some(note_raw)
        };

        let wallet_id = self.resolve_recurring_wallet_id();
        let flow_id = self.resolve_recurring_flow_id();

        let payload = RecurringTemplateNew {
            vault_id,
            kind: form.kind,
            amount_minor,
            wallet_id,
            flow_id,
            category_id: None,
            category: if category.is_empty() {
                None
            } else {
                Some(category)
            },
            note,
            frequency: form.frequency,
            day_of_period: day,
            start_date,
            end_date,
        };

        let res = self.client.recurring_create(payload).await;

        match res {
            Ok(_created) => {
                self.state.recurring.form = RecurringFormState::default();
                self.state.recurring.mode = RecurringMode::List;
                self.set_toast(
                    t(self.state.locale, TextKey::RecurringCreated),
                    ToastLevel::Success,
                );
                self.load_recurring().await?;
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorSaving) else {
                    return Ok(());
                };
                self.state.recurring.form.error = Some(msg);
            }
        }

        Ok(())
    }

    pub(crate) async fn archive_recurring(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let Some(template) = self.state.recurring.selected_template() else {
            return Ok(());
        };
        let id = template.id;

        let res = self
            .client
            .recurring_archive(id, RecurringTemplateArchive { vault_id })
            .await;

        match res {
            Ok(()) => {
                self.set_toast(
                    t(self.state.locale, TextKey::RecurringArchived),
                    ToastLevel::Success,
                );
                self.load_recurring().await?;
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::HintDelete) else {
                    return Ok(());
                };
                self.state.recurring.error = Some(msg);
            }
        }

        Ok(())
    }

    pub(crate) async fn toggle_recurring_enabled(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let Some(template) = self.state.recurring.selected_template() else {
            return Ok(());
        };
        let id = template.id;
        let new_enabled = !template.enabled;

        let res = self
            .client
            .recurring_update(
                id,
                RecurringTemplateUpdate {
                    vault_id,
                    amount_minor: None,
                    wallet_id: None,
                    flow_id: None,
                    category_id: None,
                    category: None,
                    note: None,
                    frequency: None,
                    day_of_period: None,
                    end_date: None,
                    enabled: Some(new_enabled),
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.load_recurring().await?;
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorUpdating) else {
                    return Ok(());
                };
                self.state.recurring.error = Some(msg);
            }
        }

        Ok(())
    }

    pub(crate) async fn execute_pending_recurring(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let Some(pending) = self.state.recurring.selected_pending() else {
            return Ok(());
        };
        let id = pending.template.id;

        let res = self
            .client
            .recurring_execute(id, RecurringExecute { vault_id })
            .await;

        match res {
            Ok(_response) => {
                self.set_toast(
                    t(self.state.locale, TextKey::RecurringExecuted),
                    ToastLevel::Success,
                );
                self.load_recurring().await?;
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorSaving) else {
                    return Ok(());
                };
                self.state.recurring.error = Some(msg);
            }
        }

        Ok(())
    }

    fn resolve_recurring_wallet_id(&self) -> Option<uuid::Uuid> {
        let snapshot = self.state.snapshot.as_ref()?;
        let idx = self.state.recurring.form.wallet_index;
        snapshot.wallets.get(idx).map(|w| w.id)
    }

    fn resolve_recurring_flow_id(&self) -> Option<uuid::Uuid> {
        let snapshot = self.state.snapshot.as_ref()?;
        let idx = self.state.recurring.form.flow_index;
        snapshot.flows.get(idx).map(|f| f.id)
    }

    pub(crate) async fn update_recurring(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;

        // Get selected template ID
        let template_id = self
            .state
            .recurring
            .selected_template_for_edit()
            .map(|t| t.id)
            .ok_or_else(|| AppError::Terminal("no template selected".to_string()))?;

        // Extract form data
        let form = &self.state.recurring.form;
        let amount_str = form.amount.value().trim().to_string();
        let day_str = form.day_of_period.value().trim().to_string();
        let start_date = form.start_date.value().trim().to_string();
        let end_date_raw = form.end_date.value().trim().to_string();
        let category_raw = form.category.value().trim().to_string();
        let note_raw = form.note.value().trim().to_string();
        let frequency = form.frequency;

        // Parse amount
        let amount_f: f64 = match amount_str.parse() {
            Ok(v) => v,
            Err(_) => {
                self.state.recurring.form.error =
                    Some(t(self.state.locale, TextKey::ValidationAmountInvalid).to_string());
                return Ok(());
            }
        };
        let amount_minor = (amount_f * 100.0) as i64;

        if amount_minor <= 0 {
            self.state.recurring.form.error =
                Some(t(self.state.locale, TextKey::ValidationAmountPositive).to_string());
            return Ok(());
        }

        // Validate start_date
        if start_date.is_empty() {
            self.state.recurring.form.error =
                Some(t(self.state.locale, TextKey::ValidationDateRequired).to_string());
            return Ok(());
        }

        // Get wallet/flow IDs from form indices
        let wallet_id = self.resolve_recurring_wallet_id();
        let flow_id = self.resolve_recurring_flow_id();

        // Parse day_of_period
        let day: i32 = day_str.parse().unwrap_or(1);

        // Parse end_date (optional)
        // Double Option: Some(Some(date)) = set date, Some(None) = clear date, None = no change
        let end_date = if end_date_raw.is_empty() {
            Some(None) // Clear the end date
        } else {
            Some(Some(end_date_raw))
        };

        // Get category string
        let category = if category_raw.is_empty() {
            None
        } else {
            Some(category_raw)
        };

        // Get note
        let note = if note_raw.is_empty() {
            None
        } else {
            Some(note_raw)
        };

        // Build update payload
        let payload = RecurringTemplateUpdate {
            vault_id,
            amount_minor: Some(amount_minor),
            wallet_id,
            flow_id,
            category_id: None,
            category,
            note,
            frequency: Some(frequency),
            day_of_period: Some(day),
            end_date,
            enabled: None, // Don't change enabled status during edit
        };

        // API call
        let res = self.client.recurring_update(template_id, payload).await;

        match res {
            Ok(()) => {
                self.state.recurring.form = RecurringFormState::default();
                self.state.recurring.mode = RecurringMode::List;
                self.set_toast(
                    t(self.state.locale, TextKey::RecurringUpdated),
                    ToastLevel::Success,
                );
                self.load_recurring().await?;
            }
            Err(err) => {
                let Some(msg) = self.on_api_error_toast(err, TextKey::ErrorUpdating) else {
                    return Ok(());
                };
                self.state.recurring.form.error = Some(msg);
            }
        }

        Ok(())
    }
}
