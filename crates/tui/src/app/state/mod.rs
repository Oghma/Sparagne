//! Application state types for the TUI.

mod categories;
mod common;
mod flows;
mod members;
mod stats;
mod transactions;
mod vault;
mod wallets;

pub use categories::*;
pub use common::{MRU_LIMIT, *};
pub use flows::*;
pub use members::*;
pub use stats::*;
pub use transactions::*;
pub use vault::*;
pub use wallets::*;

use api_types::vault::{Vault, VaultSnapshot};
use chrono::{DateTime, FixedOffset};
use uuid::Uuid;

use crate::{config::Density, text::Locale};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    Home,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Home,
    Transactions,
    Accounts,
    Analytics,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Categories,
    Vault,
    Members,
}

impl SettingsTab {
    pub const ALL: [Self; 3] = [Self::Categories, Self::Vault, Self::Members];

    pub fn index(self) -> usize {
        match self {
            Self::Categories => 0,
            Self::Vault => 1,
            Self::Members => 2,
        }
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Vault,
            2 => Self::Members,
            _ => Self::Categories,
        }
    }

    pub fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }

    pub fn prev(self) -> Self {
        let len = Self::ALL.len();
        Self::from_index((self.index() + len - 1) % len)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountsTab {
    Sources,
    Envelopes,
    Goals,
}

impl AccountsTab {
    pub const ALL: [Self; 3] = [Self::Sources, Self::Envelopes, Self::Goals];

    pub fn index(self) -> usize {
        match self {
            Self::Sources => 0,
            Self::Envelopes => 1,
            Self::Goals => 2,
        }
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Envelopes,
            2 => Self::Goals,
            _ => Self::Sources,
        }
    }

    pub fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }

    pub fn prev(self) -> Self {
        let len = Self::ALL.len();
        Self::from_index((self.index() + len - 1) % len)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    Username,
    Password,
}

#[derive(Debug)]
pub struct LoginState {
    pub username: String,
    pub password: String,
    pub focus: LoginField,
    pub message: Option<String>,
}

#[derive(Debug)]
pub struct AppState {
    pub screen: Screen,
    pub login: LoginState,
    pub vault: Option<Vault>,
    pub snapshot: Option<VaultSnapshot>,
    pub section: Section,
    pub accounts_tab: AccountsTab,
    pub settings_tab: SettingsTab,
    pub home_feed_selected: usize,
    pub home_low_balance_minor: i64,
    pub undo_toast_secs: u64,
    /// Enable emoji icons in the UI.
    pub emoji_mode: bool,
    /// UI density setting.
    pub density: Density,
    /// UI locale for translations.
    pub locale: Locale,
    pub transactions: TransactionsState,
    pub wallets: WalletsState,
    pub flows: FlowsState,
    pub vault_ui: VaultState,
    pub categories: CategoriesState,
    pub members: MembersState,
    pub stats: StatsState,
    pub palette: CommandPaletteState,
    pub global_search: GlobalSearchState,
    pub help: HelpState,
    pub toast: Option<ToastState>,
    pub overlays: OverlayState,
    pub connection: ConnectionState,
    pub spinner: SpinnerState,
    pub last_refresh: Option<DateTime<FixedOffset>>,
    pub last_flow_id: Option<Uuid>,
    pub default_wallet_id: Option<Uuid>,
    pub default_flow_id: Option<Uuid>,
}
