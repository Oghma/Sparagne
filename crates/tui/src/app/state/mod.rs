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
pub use common::*;
pub use flows::*;
pub use members::*;
pub use stats::*;
pub use transactions::*;
pub use vault::*;
pub use wallets::*;

use api_types::vault::{Vault, VaultSnapshot};
use chrono::{DateTime, FixedOffset};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    Home,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Home,
    Transactions,
    Wallets,
    Flows,
    Categories,
    Members,
    Vault,
    Stats,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Transactions => "Transactions",
            Self::Wallets => "Wallets",
            Self::Flows => "Flows",
            Self::Categories => "Categories",
            Self::Members => "Members",
            Self::Vault => "Vault",
            Self::Stats => "Stats",
        }
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
    pub transactions: TransactionsState,
    pub wallets: WalletsState,
    pub flows: FlowsState,
    pub vault_ui: VaultState,
    pub categories: CategoriesState,
    pub members: MembersState,
    pub stats: StatsState,
    pub palette: CommandPaletteState,
    pub help: HelpState,
    pub toast: Option<ToastState>,
    pub connection: ConnectionState,
    pub last_refresh: Option<DateTime<FixedOffset>>,
    pub last_flow_id: Option<Uuid>,
    pub default_wallet_id: Option<Uuid>,
    pub default_flow_id: Option<Uuid>,
}
