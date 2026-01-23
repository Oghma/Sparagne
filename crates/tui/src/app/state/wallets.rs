use api_types::transaction::TransactionView;

#[derive(Debug)]
pub struct WalletsState {
    pub selected: usize,
    pub mode: WalletsMode,
    pub error: Option<String>,
    pub detail: WalletDetailState,
    pub form: WalletFormState,
    pub search_query: String,
    pub search_active: bool,
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

#[derive(Debug)]
pub struct WalletFormState {
    pub name: String,
    pub opening: String,
    pub focus: WalletFormField,
    pub error: Option<String>,
}

impl Default for WalletFormState {
    fn default() -> Self {
        Self {
            name: String::new(),
            opening: String::new(),
            focus: WalletFormField::Name,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletFormField {
    Name,
    Opening,
}
