use api_types::vault::VaultView;

#[derive(Debug)]
pub struct VaultState {
    pub mode: VaultMode,
    pub form: VaultFormState,
    pub defaults: DefaultsFormState,
    pub list: VaultListState,
    pub error: Option<String>,
    pub confirm_delete: bool,
}

impl Default for VaultState {
    fn default() -> Self {
        Self {
            mode: VaultMode::View,
            form: VaultFormState::default(),
            defaults: DefaultsFormState::default(),
            list: VaultListState::default(),
            error: None,
            confirm_delete: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultMode {
    View,
    Create,
    Defaults,
    Select,
}

#[derive(Debug, Default)]
pub struct VaultFormState {
    pub name: String,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct DefaultsFormState {
    pub wallet_index: usize,
    pub flow_index: usize,
    pub focus: DefaultsField,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct VaultListState {
    pub items: Vec<VaultView>,
    pub selected: usize,
    pub error: Option<String>,
}

impl Default for DefaultsFormState {
    fn default() -> Self {
        Self {
            wallet_index: 0,
            flow_index: 0,
            focus: DefaultsField::Wallet,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultsField {
    Wallet,
    Flow,
}
