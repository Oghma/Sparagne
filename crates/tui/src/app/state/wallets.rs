use api_types::transaction::TransactionView;

use super::search::ListSearchState;
use super::selectable::{HasArchiveToggle, HasSelection, Resettable, UpdateFocus};
use crate::ui::forms::{AmountField, TextField};

#[derive(Debug)]
pub struct WalletsState {
    pub selected: usize,
    pub mode: WalletsMode,
    pub error: Option<String>,
    pub detail: WalletDetailState,
    pub form: WalletFormState,
    pub search: ListSearchState,
    pub show_archived: bool,
}

impl HasSelection for WalletsState {
    fn selected(&self) -> usize {
        self.selected
    }

    fn set_selected(&mut self, idx: usize) {
        self.selected = idx;
    }
}

impl HasArchiveToggle for WalletsState {
    fn show_archived(&self) -> bool {
        self.show_archived
    }

    fn set_show_archived(&mut self, show: bool) {
        self.show_archived = show;
    }
}

impl Default for WalletsState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: WalletsMode::List,
            error: None,
            detail: WalletDetailState::default(),
            form: WalletFormState::default(),
            search: ListSearchState::default(),
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

impl UpdateFocus for WalletFormState {
    fn update_focus(&mut self) {
        self.name.state.focused = self.focus == WalletFormField::Name;
        self.opening.state.focused = self.focus == WalletFormField::Opening;
    }
}

impl Resettable for WalletsState {
    type Form = WalletFormState;

    fn form_mut(&mut self) -> &mut Self::Form {
        &mut self.form
    }

    fn error_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }
}

impl WalletFormState {

    /// Validates all fields and returns the first error message if any.
    pub(crate) fn validate_all(&mut self) -> Option<String> {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletFormField {
    Name,
    Opening,
}
