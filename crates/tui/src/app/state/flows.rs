use api_types::transaction::TransactionView;
use engine::CashFlow;

#[derive(Debug)]
pub struct FlowsState {
    pub selected: usize,
    pub mode: FlowsMode,
    pub error: Option<String>,
    pub detail: FlowDetailState,
    pub form: FlowFormState,
    pub search_query: String,
    pub search_active: bool,
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

#[derive(Debug)]
pub struct FlowFormState {
    pub name: String,
    pub mode: FlowModeChoice,
    pub cap: String,
    pub opening: String,
    pub focus: FlowFormField,
    pub error: Option<String>,
}

impl Default for FlowFormState {
    fn default() -> Self {
        Self {
            name: String::new(),
            mode: FlowModeChoice::Unlimited,
            cap: String::new(),
            opening: String::new(),
            focus: FlowFormField::Name,
            error: None,
        }
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
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Unlimited => "Unlimited",
            Self::NetCapped => "Net capped",
            Self::IncomeCapped => "Income capped",
        }
    }
}
