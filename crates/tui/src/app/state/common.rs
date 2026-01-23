use std::time::Instant;

#[derive(Debug, Default)]
pub struct HelpState {
    pub active: bool,
}

#[derive(Debug)]
pub struct ToastState {
    pub message: String,
    pub level: ToastLevel,
    pub expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Error,
}

#[derive(Debug, Default)]
pub struct ConnectionState {
    pub ok: bool,
    pub message: Option<String>,
}

#[derive(Debug, Default)]
pub struct CommandPaletteState {
    pub active: bool,
    pub query: String,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    NewExpense,
    NewIncome,
    NewRefund,
    NewTransferWallet,
    NewTransferFlow,
    Categories,
    CategoryAliases,
    Members,
    WalletNew,
    FlowNew,
    VaultCreate,
    Refresh,
    ToggleVoided,
}

impl PaletteCommand {
    pub fn all() -> Vec<Self> {
        vec![
            Self::NewExpense,
            Self::NewIncome,
            Self::NewRefund,
            Self::NewTransferWallet,
            Self::NewTransferFlow,
            Self::Categories,
            Self::CategoryAliases,
            Self::Members,
            Self::WalletNew,
            Self::FlowNew,
            Self::VaultCreate,
            Self::Refresh,
            Self::ToggleVoided,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NewExpense => "Transactions: New Expense",
            Self::NewIncome => "Transactions: New Income",
            Self::NewRefund => "Transactions: New Refund",
            Self::NewTransferWallet => "Transactions: New Transfer Wallet",
            Self::NewTransferFlow => "Transactions: New Transfer Flow",
            Self::Categories => "Categories: Open",
            Self::CategoryAliases => "Categories: Aliases",
            Self::Members => "Members: Open",
            Self::WalletNew => "Wallets: New",
            Self::FlowNew => "Flows: New",
            Self::VaultCreate => "Vault: Create",
            Self::Refresh => "Refresh",
            Self::ToggleVoided => "Transactions: Toggle voided",
        }
    }
}
