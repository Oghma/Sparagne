use api_types::transaction::TransactionView;
use engine::CashFlow;

use super::search::ListSearchState;
use super::selectable::{EntityListMode, HasArchiveToggle, HasSelection, Resettable, UpdateFocus};
use crate::ui::forms::{AmountField, TextField};

#[derive(Debug)]
pub struct FlowsState {
    pub selected: usize,
    pub mode: EntityListMode,
    pub error: Option<String>,
    pub detail: FlowDetailState,
    pub form: FlowFormState,
    pub search: ListSearchState,
    pub show_archived: bool,
}

impl HasSelection for FlowsState {
    fn selected(&self) -> usize {
        self.selected
    }

    fn set_selected(&mut self, idx: usize) {
        self.selected = idx;
    }
}

impl HasArchiveToggle for FlowsState {
    fn show_archived(&self) -> bool {
        self.show_archived
    }

    fn set_show_archived(&mut self, show: bool) {
        self.show_archived = show;
    }
}

impl Default for FlowsState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: EntityListMode::List,
            error: None,
            detail: FlowDetailState::default(),
            form: FlowFormState::default(),
            search: ListSearchState::default(),
            show_archived: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct FlowDetailState {
    pub flow_id: Option<uuid::Uuid>,
    pub transactions: Vec<TransactionView>,
    pub detail: Option<CashFlow>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FlowFormState {
    pub name: TextField,
    pub mode: FlowModeChoice,
    pub cap: AmountField,
    pub opening: AmountField,
    pub focus: FlowFormField,
}

impl Default for FlowFormState {
    fn default() -> Self {
        Self {
            name: TextField::new("Name").required(true).min_length(1),
            mode: FlowModeChoice::Unlimited,
            cap: AmountField::new("Cap")
                .required(false)
                .require_positive(true),
            opening: AmountField::new("Opening")
                .required(false)
                .require_positive(false),
            focus: FlowFormField::Name,
        }
    }
}

impl UpdateFocus for FlowFormState {
    fn update_focus(&mut self) {
        self.name.state.focused = self.focus == FlowFormField::Name;
        self.cap.state.focused = self.focus == FlowFormField::Cap;
        self.opening.state.focused = self.focus == FlowFormField::Opening;
    }
}

impl Resettable for FlowsState {
    type Form = FlowFormState;

    fn form_mut(&mut self) -> &mut Self::Form {
        &mut self.form
    }

    fn error_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }
}

impl FlowFormState {

    /// Validates all fields and returns the first error message if any.
    pub(crate) fn validate_all(&mut self) -> Option<String> {
        self.name.validate();
        // Only validate cap if mode requires it
        if !matches!(self.mode, FlowModeChoice::Unlimited) {
            self.cap.validate();
        }
        self.opening.validate();

        if let Some(err) = self.name.state.validation.error_message() {
            return Some(err.to_string());
        }
        if !matches!(self.mode, FlowModeChoice::Unlimited)
            && let Some(err) = self.cap.state.validation.error_message()
        {
            return Some(err.to_string());
        }
        if let Some(err) = self.opening.state.validation.error_message() {
            return Some(err.to_string());
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowFormField {
    Name,
    Mode,
    Cap,
    Opening,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowModeChoice {
    Unlimited,
    NetCapped,
    IncomeCapped,
}

impl FlowModeChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unlimited => "Unlimited",
            Self::NetCapped => "Net capped",
            Self::IncomeCapped => "Income capped",
        }
    }
}
