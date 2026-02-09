use api_types::recurring::{
    PendingRecurringView, RecurrenceFrequency, RecurringKind, RecurringTemplateView,
};
use uuid::Uuid;

use super::search::ListSearchState;
use crate::ui::forms::{AmountField, TextField};

#[derive(Debug)]
pub struct RecurringState {
    pub templates: Vec<RecurringTemplateView>,
    pub pending: Vec<PendingRecurringView>,
    pub pending_count: usize,
    pub selected: usize,
    pub mode: RecurringMode,
    pub form: RecurringFormState,
    pub search: ListSearchState,
    pub error: Option<String>,
}

impl Default for RecurringState {
    fn default() -> Self {
        Self {
            templates: Vec::new(),
            pending: Vec::new(),
            pending_count: 0,
            selected: 0,
            mode: RecurringMode::List,
            form: RecurringFormState::default(),
            search: ListSearchState::default(),
            error: None,
        }
    }
}

impl RecurringState {
    pub(crate) fn reset(&mut self) {
        self.templates.clear();
        self.pending.clear();
        self.pending_count = 0;
        self.selected = 0;
        self.mode = RecurringMode::List;
        self.form = RecurringFormState::default();
        self.search = ListSearchState::default();
        self.error = None;
    }

    pub(crate) fn select_next(&mut self) {
        let total = self.pending.len() + self.templates.len();
        if total == 0 {
            return;
        }
        self.selected = (self.selected + 1).min(total - 1);
    }

    pub(crate) fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Returns whether the selected index points to a pending item.
    pub(crate) fn selected_is_pending(&self) -> bool {
        self.selected < self.pending.len()
    }

    /// Returns the pending item at the current selection, if applicable.
    pub(crate) fn selected_pending(&self) -> Option<&PendingRecurringView> {
        if self.selected_is_pending() {
            self.pending.get(self.selected)
        } else {
            None
        }
    }

    /// Returns the template at the current selection (from the templates list).
    pub(crate) fn selected_template(&self) -> Option<&RecurringTemplateView> {
        if self.selected_is_pending() {
            None
        } else {
            let idx = self.selected - self.pending.len();
            self.templates.get(idx)
        }
    }

    /// Returns the template for editing - either from pending item or templates list.
    pub(crate) fn selected_template_for_edit(&self) -> Option<&RecurringTemplateView> {
        if self.selected_is_pending() {
            self.pending.get(self.selected).map(|p| &p.template)
        } else {
            let idx = self.selected - self.pending.len();
            self.templates.get(idx)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringMode {
    List,
    Create,
    Edit,
}

#[derive(Debug, Clone)]
pub struct RecurringFormState {
    pub kind: RecurringKind,
    pub amount: AmountField,
    pub wallet_id: Option<Uuid>,
    pub wallet_index: usize,
    pub flow_id: Option<Uuid>,
    pub flow_index: usize,
    pub category: TextField,
    pub note: TextField,
    pub frequency: RecurrenceFrequency,
    pub day_of_period: TextField,
    pub start_date: TextField,
    pub end_date: TextField,
    pub focus: RecurringFormField,
    pub error: Option<String>,
}

impl Default for RecurringFormState {
    fn default() -> Self {
        Self {
            kind: RecurringKind::Expense,
            amount: AmountField::new("Amount"),
            wallet_id: None,
            wallet_index: 0,
            flow_id: None,
            flow_index: 0,
            category: TextField::new("Category"),
            note: TextField::new("Note"),
            frequency: RecurrenceFrequency::Monthly,
            day_of_period: TextField::new("Day"),
            start_date: TextField::new("Start"),
            end_date: TextField::new("End"),
            focus: RecurringFormField::Kind,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringFormField {
    Kind,
    Amount,
    Wallet,
    Flow,
    Category,
    Note,
    Frequency,
    DayOfPeriod,
    StartDate,
    EndDate,
}

impl RecurringFormField {
    pub fn next(self) -> Self {
        match self {
            Self::Kind => Self::Amount,
            Self::Amount => Self::Wallet,
            Self::Wallet => Self::Flow,
            Self::Flow => Self::Category,
            Self::Category => Self::Note,
            Self::Note => Self::Frequency,
            Self::Frequency => Self::DayOfPeriod,
            Self::DayOfPeriod => Self::StartDate,
            Self::StartDate => Self::EndDate,
            Self::EndDate => Self::Kind,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Kind => Self::EndDate,
            Self::Amount => Self::Kind,
            Self::Wallet => Self::Amount,
            Self::Flow => Self::Wallet,
            Self::Category => Self::Flow,
            Self::Note => Self::Category,
            Self::Frequency => Self::Note,
            Self::DayOfPeriod => Self::Frequency,
            Self::StartDate => Self::DayOfPeriod,
            Self::EndDate => Self::StartDate,
        }
    }
}

use super::selectable::{Resettable, UpdateFocus};

impl UpdateFocus for RecurringFormState {
    fn update_focus(&mut self) {
        self.amount.state.focused = self.focus == RecurringFormField::Amount;
        self.category.state.focused = self.focus == RecurringFormField::Category;
        self.note.state.focused = self.focus == RecurringFormField::Note;
        self.day_of_period.state.focused = self.focus == RecurringFormField::DayOfPeriod;
        self.start_date.state.focused = self.focus == RecurringFormField::StartDate;
        self.end_date.state.focused = self.focus == RecurringFormField::EndDate;
    }
}

impl Resettable for RecurringState {
    type Form = RecurringFormState;

    fn form_mut(&mut self) -> &mut Self::Form {
        &mut self.form
    }

    fn error_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }
}
