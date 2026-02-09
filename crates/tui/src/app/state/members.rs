use api_types::membership::{MemberView, MembershipRole};

use super::selectable::{Resettable, SelectableList, UpdateFocus};
use crate::ui::forms::TextField;

#[derive(Debug)]
pub struct MembersState {
    pub scope: MembersScope,
    pub mode: MembersMode,
    pub items: Vec<MemberView>,
    pub selected: usize,
    pub flow_index: usize,
    pub form: MemberFormState,
    pub error: Option<String>,
}

impl SelectableList for MembersState {
    fn visible_count(&self) -> usize {
        self.items.len()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn set_selected(&mut self, idx: usize) {
        self.selected = idx;
    }
}

impl Default for MembersState {
    fn default() -> Self {
        Self {
            scope: MembersScope::Vault,
            mode: MembersMode::List,
            items: Vec::new(),
            selected: 0,
            flow_index: 0,
            form: MemberFormState::default(),
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembersScope {
    Vault,
    Flow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembersMode {
    List,
    Form,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberFormField {
    Username,
    Role,
}

#[derive(Debug, Clone)]
pub struct MemberFormState {
    pub username: TextField,
    pub role: MembershipRole,
    pub focus: MemberFormField,
    pub editing: bool,
}

impl Default for MemberFormState {
    fn default() -> Self {
        Self {
            username: TextField::new("Username").required(true).min_length(1),
            role: MembershipRole::Viewer,
            focus: MemberFormField::Username,
            editing: false,
        }
    }
}

impl UpdateFocus for MemberFormState {
    fn update_focus(&mut self) {
        self.username.state.focused = self.focus == MemberFormField::Username;
    }
}

impl Resettable for MembersState {
    type Form = MemberFormState;

    fn form_mut(&mut self) -> &mut Self::Form {
        &mut self.form
    }

    fn error_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }
}

impl MemberFormState {
    /// Validates all fields and returns the first error message if any.
    pub(crate) fn validate_all(&mut self) -> Option<String> {
        self.username.validate();
        self.username
            .state
            .validation
            .error_message()
            .map(String::from)
    }
}
