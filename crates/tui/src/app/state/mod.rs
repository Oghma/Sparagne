//! Application state types for the TUI.

#[macro_use]
mod cyclic;
mod categories;
mod flows;
mod members;
mod overlays;
mod palette;
pub(crate) mod recurring;
mod search;
mod selectable;
mod stats;
mod toast;
mod transactions;
mod ui;
mod vault;
mod wallets;

pub use categories::*;
pub use flows::*;
pub use members::*;
pub use overlays::{
    BulkCategoryDialogState, ConfirmAction, ConfirmDialogKind, ConfirmDialogState, ErrorAction,
    ErrorDialogKind, ErrorDialogState, GroupingDialogState, OverlayState,
};
pub use palette::{CommandPaletteState, MRU_LIMIT, PaletteCommand};
pub use recurring::{RecurringFormField, RecurringFormState, RecurringMode, RecurringState};
pub use search::{GlobalSearchState, SearchResult, SearchResultKind};
pub use selectable::EntityListMode;
pub(crate) use selectable::{
    HasArchiveToggle, Resettable, SelectableList, SelectableWithCount, UpdateFocus,
};
pub use stats::*;
pub use toast::{ToastLevel, ToastState, UndoAction};
pub use transactions::*;
pub use ui::{ConnectionState, HelpState, SpinnerState};
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
    Preferences,
}

cyclic_enum!(SettingsTab { Categories => 0, Vault => 1, Members => 2, Preferences => 3 });

/// Focus state for the Preferences settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesField {
    #[default]
    EmojiMode,
    Density,
}

impl PreferencesField {
    pub fn next(self) -> Self {
        match self {
            Self::EmojiMode => Self::Density,
            Self::Density => Self::EmojiMode,
        }
    }

    pub fn prev(self) -> Self {
        self.next()
    }
}

/// State for the Preferences settings screen.
#[derive(Debug, Clone, Default)]
pub struct PreferencesState {
    pub focus: PreferencesField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountsTab {
    Wallets,
    Budget,
}

cyclic_enum!(AccountsTab { Wallets => 0, Budget => 1 });

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
    pub preferences: PreferencesState,
    pub recurring: RecurringState,
}
