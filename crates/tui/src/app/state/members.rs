use api_types::membership::{MemberView, MembershipRole};

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

#[allow(dead_code)]
impl MemberFormState {
    /// Updates focus state on all fields based on current focus.
    pub fn update_focus(&mut self) {
        self.username.state.focused = self.focus == MemberFormField::Username;
    }

    /// Returns true if all fields are valid.
    pub fn is_valid(&self) -> bool {
        self.username.state.validation.is_valid()
    }

    /// Validates all fields and returns the first error message if any.
    pub fn validate_all(&mut self) -> Option<String> {
        self.username.validate();
        self.username
            .state
            .validation
            .error_message()
            .map(String::from)
    }

    /// Clears the form and resets to default state.
    pub fn clear(&mut self) {
        self.username.clear();
        self.role = MembershipRole::Viewer;
        self.focus = MemberFormField::Username;
        self.editing = false;
        self.update_focus();
    }
}
