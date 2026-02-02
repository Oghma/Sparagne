use api_types::transaction::TransactionView;

use crate::ui::forms::{AmountField, TextField};

#[derive(Debug)]
pub struct WalletsState {
    pub selected: usize,
    pub mode: WalletsMode,
    pub error: Option<String>,
    pub detail: WalletDetailState,
    pub form: WalletFormState,
    pub search_query: String,
    pub search_active: bool,
    pub show_archived: bool,
}

impl Default for WalletsState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: WalletsMode::List,
            error: None,
            detail: WalletDetailState::default(),
            form: WalletFormState::default(),
            search_query: String::new(),
            search_active: false,
            show_archived: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletsMode {
    List,
    Detail,
    Create,
    Rename,
}

#[derive(Debug, Default)]
pub struct WalletDetailState {
    pub wallet_id: Option<uuid::Uuid>,
    pub transactions: Vec<TransactionView>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WalletFormState {
    pub name: TextField,
    pub opening: AmountField,
    pub focus: WalletFormField,
}

impl Default for WalletFormState {
    fn default() -> Self {
        Self {
            name: TextField::new("Name").required(true).min_length(1),
            opening: AmountField::new("Opening")
                .required(false)
                .require_positive(false),
            focus: WalletFormField::Name,
        }
    }
}

#[allow(dead_code)]
impl WalletFormState {
    /// Updates focus state on all fields based on current focus.
    pub fn update_focus(&mut self) {
        self.name.state.focused = self.focus == WalletFormField::Name;
        self.opening.state.focused = self.focus == WalletFormField::Opening;
    }

    /// Returns true if all fields are valid.
    pub fn is_valid(&self) -> bool {
        self.name.state.validation.is_valid() && self.opening.state.validation.is_valid()
    }

    /// Validates all fields and returns the first error message if any.
    pub fn validate_all(&mut self) -> Option<String> {
        self.name.validate();
        self.opening.validate();

        if let Some(err) = self.name.state.validation.error_message() {
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
        self.opening.clear();
        self.focus = WalletFormField::Name;
        self.update_focus();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletFormField {
    Name,
    Opening,
}
