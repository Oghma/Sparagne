//! Command palette state.

use super::selectable::SelectableList;

/// Maximum number of MRU commands to track.
pub const MRU_LIMIT: usize = 5;

#[derive(Debug, Default)]
pub struct CommandPaletteState {
    pub active: bool,
    pub query: String,
    pub selected: usize,
    /// Most recently used commands (most recent first).
    pub mru: Vec<PaletteCommand>,
    /// Cached filtered command count for selection bounds.
    pub(crate) filtered_count: usize,
}

impl SelectableList for CommandPaletteState {
    fn visible_count(&self) -> usize {
        self.filtered_count
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn set_selected(&mut self, idx: usize) {
        self.selected = idx;
    }
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

impl std::str::FromStr for PaletteCommand {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "new_expense" => Ok(Self::NewExpense),
            "new_income" => Ok(Self::NewIncome),
            "new_refund" => Ok(Self::NewRefund),
            "new_transfer_wallet" => Ok(Self::NewTransferWallet),
            "new_transfer_flow" => Ok(Self::NewTransferFlow),
            "categories" => Ok(Self::Categories),
            "category_aliases" => Ok(Self::CategoryAliases),
            "members" => Ok(Self::Members),
            "wallet_new" => Ok(Self::WalletNew),
            "flow_new" => Ok(Self::FlowNew),
            "vault_create" => Ok(Self::VaultCreate),
            "refresh" => Ok(Self::Refresh),
            "toggle_voided" => Ok(Self::ToggleVoided),
            _ => Err(()),
        }
    }
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
            Self::FlowNew => "Accounts: New Envelope",
            Self::VaultCreate => "Vault: Create",
            Self::Refresh => "Refresh",
            Self::ToggleVoided => "Transactions: Toggle voided",
        }
    }

    /// Returns a unique string identifier for persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewExpense => "new_expense",
            Self::NewIncome => "new_income",
            Self::NewRefund => "new_refund",
            Self::NewTransferWallet => "new_transfer_wallet",
            Self::NewTransferFlow => "new_transfer_flow",
            Self::Categories => "categories",
            Self::CategoryAliases => "category_aliases",
            Self::Members => "members",
            Self::WalletNew => "wallet_new",
            Self::FlowNew => "flow_new",
            Self::VaultCreate => "vault_create",
            Self::Refresh => "refresh",
            Self::ToggleVoided => "toggle_voided",
        }
    }
}
