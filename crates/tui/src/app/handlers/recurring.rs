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
            RecurringMode::Create => {
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
        }
        Ok(())
    }

    /// Handles cancel in recurring mode.
    pub(crate) fn handle_recurring_cancel(&mut self) {
        match self.state.recurring.mode {
            RecurringMode::Create => {
                self.state.recurring.mode = RecurringMode::List;
                self.state.recurring.form = RecurringFormState::default();
            }
            RecurringMode::List => {
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
            RecurringMode::Create => {
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
            RecurringMode::Create => {
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
}
