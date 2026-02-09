//! Recurring template handlers.

use api_types::recurring::{RecurrenceFrequency, RecurringKind};

use super::super::*;

use crate::error::Result;

impl App {
    /// Handles key input when in recurring sub-mode.
    /// Returns `true` if the key was handled.
    pub(crate) async fn handle_recurring_input(&mut self, ch: char) -> Result<bool> {
        match self.state.recurring.mode {
            RecurringMode::List => self.handle_recurring_list_input(ch).await,
            RecurringMode::Create | RecurringMode::Edit => {
                self.handle_recurring_form_input(ch);
                Ok(true)
            }
        }
    }

    async fn handle_recurring_list_input(&mut self, ch: char) -> Result<bool> {
        match ch {
            'c' => {
                self.state.recurring.mode = RecurringMode::Create;
                self.state.recurring.form = RecurringFormState::default();
                Ok(true)
            }
            'e' => {
                self.start_recurring_edit();
                Ok(true)
            }
            'r' => {
                self.load_recurring().await?;
                Ok(true)
            }
            'd' => {
                self.archive_recurring().await?;
                Ok(true)
            }
            ' ' => {
                self.toggle_recurring_enabled().await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn handle_recurring_form_input(&mut self, ch: char) {
        let form = &mut self.state.recurring.form;
        match form.focus {
            RecurringFormField::Kind => {
                // Kind is toggled with Space or Up/Down, not typed characters
            }
            RecurringFormField::Amount => {
                form.amount.push(ch);
            }
            RecurringFormField::Wallet | RecurringFormField::Flow => {
                // Cycled with Up/Down, not typed characters
            }
            RecurringFormField::Category => {
                form.category.push(ch);
            }
            RecurringFormField::Note => {
                form.note.push(ch);
            }
            RecurringFormField::Frequency => {
                // Cycled with Up/Down
            }
            RecurringFormField::DayOfPeriod => {
                form.day_of_period.push(ch);
            }
            RecurringFormField::StartDate => {
                form.start_date.push(ch);
            }
            RecurringFormField::EndDate => {
                form.end_date.push(ch);
            }
        }
    }

    /// Handles submit in recurring mode.
    pub(crate) async fn handle_recurring_submit(&mut self) -> Result<()> {
        match self.state.recurring.mode {
            RecurringMode::List => {
                if self.state.recurring.selected_is_pending() {
                    self.execute_pending_recurring().await?;
                }
            }
            RecurringMode::Create => {
                self.create_recurring().await?;
            }
            RecurringMode::Edit => {
                self.update_recurring().await?;
            }
        }
        Ok(())
    }

    /// Handles cancel in recurring mode.
    pub(crate) fn handle_recurring_cancel(&mut self) {
        match self.state.recurring.mode {
            RecurringMode::Create | RecurringMode::Edit => {
                self.reset_recurring_form();
                self.state.recurring.mode = RecurringMode::List;
            }
            RecurringMode::List => {
                self.state.recurring.reset();
                self.state.transactions.recurring_mode = false;
            }
        }
    }

    /// Handles up in recurring mode.
    pub(crate) fn handle_recurring_up(&mut self) {
        match self.state.recurring.mode {
            RecurringMode::List => {
                self.state.recurring.select_prev();
            }
            RecurringMode::Create | RecurringMode::Edit => {
                self.recurring_form_up();
            }
        }
    }

    /// Handles down in recurring mode.
    pub(crate) fn handle_recurring_down(&mut self) {
        match self.state.recurring.mode {
            RecurringMode::List => {
                self.state.recurring.select_next();
            }
            RecurringMode::Create | RecurringMode::Edit => {
                self.recurring_form_down();
            }
        }
    }

    /// Handles backspace in recurring form.
    pub(crate) fn handle_recurring_backspace(&mut self) {
        let form = &mut self.state.recurring.form;
        match form.focus {
            RecurringFormField::Amount => {
                form.amount.pop();
            }
            RecurringFormField::Category => {
                form.category.pop();
            }
            RecurringFormField::Note => {
                form.note.pop();
            }
            RecurringFormField::DayOfPeriod => {
                form.day_of_period.pop();
            }
            RecurringFormField::StartDate => {
                form.start_date.pop();
            }
            RecurringFormField::EndDate => {
                form.end_date.pop();
            }
            _ => {}
        }
    }

    /// Handles Tab (next field) in recurring form.
    pub(crate) fn recurring_form_next_field(&mut self) {
        self.state.recurring.form.focus = self.state.recurring.form.focus.next();
    }

    fn recurring_form_up(&mut self) {
        let form = &mut self.state.recurring.form;
        match form.focus {
            RecurringFormField::Kind => {
                form.kind = match form.kind {
                    RecurringKind::Income => RecurringKind::Expense,
                    RecurringKind::Expense => RecurringKind::Income,
                };
            }
            RecurringFormField::Wallet => {
                form.wallet_index = form.wallet_index.saturating_sub(1);
            }
            RecurringFormField::Flow => {
                form.flow_index = form.flow_index.saturating_sub(1);
            }
            RecurringFormField::Frequency => {
                form.frequency = match form.frequency {
                    RecurrenceFrequency::Daily => RecurrenceFrequency::Yearly,
                    RecurrenceFrequency::Weekly => RecurrenceFrequency::Daily,
                    RecurrenceFrequency::Monthly => RecurrenceFrequency::Weekly,
                    RecurrenceFrequency::Yearly => RecurrenceFrequency::Monthly,
                };
            }
            _ => {
                form.focus = form.focus.prev();
            }
        }
    }

    fn recurring_form_down(&mut self) {
        let form = &mut self.state.recurring.form;
        match form.focus {
            RecurringFormField::Kind => {
                form.kind = match form.kind {
                    RecurringKind::Income => RecurringKind::Expense,
                    RecurringKind::Expense => RecurringKind::Income,
                };
            }
            RecurringFormField::Wallet => {
                let max = self
                    .state
                    .snapshot
                    .as_ref()
                    .map(|s| s.wallets.len().saturating_sub(1))
                    .unwrap_or(0);
                form.wallet_index = (form.wallet_index + 1).min(max);
            }
            RecurringFormField::Flow => {
                let max = self
                    .state
                    .snapshot
                    .as_ref()
                    .map(|s| s.flows.len().saturating_sub(1))
                    .unwrap_or(0);
                form.flow_index = (form.flow_index + 1).min(max);
            }
            RecurringFormField::Frequency => {
                form.frequency = match form.frequency {
                    RecurrenceFrequency::Daily => RecurrenceFrequency::Weekly,
                    RecurrenceFrequency::Weekly => RecurrenceFrequency::Monthly,
                    RecurrenceFrequency::Monthly => RecurrenceFrequency::Yearly,
                    RecurrenceFrequency::Yearly => RecurrenceFrequency::Daily,
                };
            }
            _ => {
                form.focus = form.focus.next();
            }
        }
    }

    fn start_recurring_edit(&mut self) {
        use crate::app::Resettable;

        // Extract template data before borrowing mutably
        let template_data = {
            let Some(template) = self.state.recurring.selected_template_for_edit() else {
                self.state.recurring.error =
                    Some(t(self.state.locale, TextKey::ErrorResourceNotFound).to_string());
                return;
            };

            // Clone the data we need
            (
                template.kind,
                template.amount_minor,
                template.wallet_id,
                template.flow_id,
                template.note.clone(),
                template.frequency,
                template.day_of_period,
                template.start_date.clone(),
                template.end_date.clone(),
            )
        };

        let (
            kind,
            amount_minor,
            wallet_id,
            flow_id,
            note,
            frequency,
            day_of_period,
            start_date,
            end_date,
        ) = template_data;

        // Reset form to defaults
        self.state.recurring.reset_form();

        // Pre-fill all fields from selected template
        let form = &mut self.state.recurring.form;
        form.kind = kind;

        // Set amount value directly (no set_value method for AmountField)
        form.amount.state.value = format!("{:.2}", amount_minor as f64 / 100.0);
        form.amount.state.touched = true;
        form.amount.validate();

        // Pre-select wallet/flow indices (find in snapshot lists)
        if let Some(ref snapshot) = self.state.snapshot {
            if let Some(wid) = wallet_id {
                form.wallet_index = snapshot
                    .wallets
                    .iter()
                    .position(|w| w.id == wid)
                    .unwrap_or(0);
            }
            if let Some(fid) = flow_id {
                form.flow_index = snapshot.flows.iter().position(|f| f.id == fid).unwrap_or(0);
            }
        }

        // Category is not pre-filled since we only have category_id, not the name
        // User can enter a new category string if needed
        form.note.set_value(note.unwrap_or_default());
        form.frequency = frequency;
        form.day_of_period.set_value(day_of_period.to_string());
        form.start_date.set_value(start_date);
        form.end_date.set_value(end_date.unwrap_or_default());

        // Set focus to first field
        form.focus = RecurringFormField::Kind;
        form.update_focus();

        // Change mode to Edit
        self.state.recurring.mode = RecurringMode::Edit;
    }

    fn reset_recurring_form(&mut self) {
        use crate::app::Resettable;
        self.state.recurring.reset_form();
    }
}
