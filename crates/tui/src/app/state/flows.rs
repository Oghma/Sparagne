use api_types::transaction::TransactionView;
use engine::CashFlow;

use crate::ui::forms::{AmountField, TextField};

#[derive(Debug)]
pub struct FlowsState {
    pub selected: usize,
    pub mode: FlowsMode,
    pub error: Option<String>,
    pub detail: FlowDetailState,
    pub form: FlowFormState,
    pub search_query: String,
    pub search_active: bool,
    pub show_archived: bool,
}

impl Default for FlowsState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: FlowsMode::List,
            error: None,
            detail: FlowDetailState::default(),
            form: FlowFormState::default(),
            search_query: String::new(),
            search_active: false,
            show_archived: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowsMode {
    List,
    Detail,
    Create,
    Rename,
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

#[allow(dead_code)]
impl FlowFormState {
    /// Updates focus state on all fields based on current focus.
    pub fn update_focus(&mut self) {
        self.name.state.focused = self.focus == FlowFormField::Name;
        self.cap.state.focused = self.focus == FlowFormField::Cap;
        self.opening.state.focused = self.focus == FlowFormField::Opening;
    }

    /// Returns true if all fields are valid.
    pub fn is_valid(&self) -> bool {
        self.name.state.validation.is_valid()
            && self.cap.state.validation.is_valid()
            && self.opening.state.validation.is_valid()
    }

    /// Validates all fields and returns the first error message if any.
    pub fn validate_all(&mut self) -> Option<String> {
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

    /// Clears the form and resets to default state.
    pub fn clear(&mut self) {
        self.name.clear();
        self.mode = FlowModeChoice::Unlimited;
        self.cap.clear();
        self.opening.clear();
        self.focus = FlowFormField::Name;
        self.update_focus();
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
