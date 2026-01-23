use api_types::membership::{MemberView, MembershipRole};

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

#[derive(Debug)]
pub struct MemberFormState {
    pub username: String,
    pub role: MembershipRole,
    pub focus: MemberFormField,
    pub editing: bool,
    pub error: Option<String>,
}

impl Default for MemberFormState {
    fn default() -> Self {
        Self {
            username: String::new(),
            role: MembershipRole::Viewer,
            focus: MemberFormField::Username,
            editing: false,
            error: None,
        }
    }
}
