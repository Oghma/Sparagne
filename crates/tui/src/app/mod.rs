use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};

use crate::{
    client::{Client, ClientError},
    config::AppConfig,
    error::{AppError, Result},
    ui,
};
use crate::quick_add::QuickAddKind;

use api_types::{
    flow::{FlowMode, FlowNew, FlowUpdate},
    stats::Statistic,
    transaction::{
        ExpenseNew, IncomeNew, Refund, TransactionDetailResponse, TransactionGet, TransactionList,
        TransactionListResponse, TransactionUpdate, TransactionView, TransactionVoid,
        TransferFlowNew, TransferWalletNew,
    },
    vault::{Vault, VaultNew, VaultSnapshot},
    wallet::{WalletNew, WalletUpdate},
};
use chrono::{DateTime, FixedOffset, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use engine::Money;
use std::str::FromStr;

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
    pub stats: StatsState,
    pub base_url: String,
    pub last_flow_id: Option<uuid::Uuid>,
}

pub struct App {
    config: AppConfig,
    client: Client,
    pub state: AppState,
    should_quit: bool,
}

impl App {
    pub fn new(config: AppConfig) -> Result<Self> {
        let client = Client::new(&config.base_url)?;
        let state = AppState {
            screen: Screen::Login,
            login: LoginState {
                username: config.username.clone(),
                password: String::new(),
                focus: LoginField::Username,
                message: None,
            },
            vault: None,
            snapshot: None,
            section: Section::Home,
            transactions: TransactionsState::default(),
            wallets: WalletsState::default(),
            flows: FlowsState::default(),
            vault_ui: VaultState::default(),
            stats: StatsState::default(),
            base_url: config.base_url.clone(),
            last_flow_id: None,
        };

        Ok(Self {
            config,
            client,
            state,
            should_quit: false,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = ui::setup_terminal()?;
        let result = self.event_loop(&mut terminal).await;
        ui::restore_terminal(&mut terminal)?;
        result
    }

    async fn event_loop(&mut self, terminal: &mut ui::Terminal) -> Result<()> {
        let tick_rate = Duration::from_millis(200);

        while !self.should_quit {
            terminal
                .draw(|frame| ui::render(frame, &self.state))
                .map_err(|err| AppError::Terminal(err.to_string()))?;

            if event::poll(tick_rate)? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key).await?,
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
        }

        Ok(())
    }

    async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match crate::ui::keymap::map_key(key) {
            crate::ui::keymap::AppAction::Quit => {
                if self.state.screen == Screen::Login {
                    self.should_quit = true;
                } else {
                    self.should_quit = true;
                }
            }
            crate::ui::keymap::AppAction::Cancel => {
                if self.state.screen == Screen::Login {
                    self.should_quit = true;
                } else if self.state.section == Section::Transactions {
                    match self.state.transactions.mode {
                        TransactionsMode::Edit => {
                            self.state.transactions.mode = TransactionsMode::Detail;
                            self.state.transactions.edit_input.clear();
                            self.state.transactions.edit_error = None;
                        }
                        TransactionsMode::Detail => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.detail = None;
                            self.state.transactions.edit_input.clear();
                            self.state.transactions.edit_error = None;
                        }
                        TransactionsMode::List => {
                            if self.state.transactions.quick_active {
                                self.state.transactions.quick_active = false;
                                self.state.transactions.quick_input.clear();
                                self.state.transactions.quick_error = None;
                            } else {
                                self.state.section = Section::Home;
                            }
                        }
                    }
                } else if self.state.section == Section::Wallets {
                    match self.state.wallets.mode {
                        WalletsMode::Create | WalletsMode::Rename => {
                            self.reset_wallet_form();
                            self.state.wallets.mode = WalletsMode::List;
                        }
                        WalletsMode::Detail => {
                            self.state.wallets.mode = WalletsMode::List;
                            self.state.wallets.detail = WalletDetailState::default();
                        }
                        WalletsMode::List => {
                            self.state.section = Section::Home;
                        }
                    }
                } else if self.state.section == Section::Flows {
                    match self.state.flows.mode {
                        FlowsMode::Create | FlowsMode::Rename => {
                            self.reset_flow_form();
                            self.state.flows.mode = FlowsMode::List;
                        }
                        FlowsMode::Detail => {
                            self.state.flows.mode = FlowsMode::List;
                            self.state.flows.detail = FlowDetailState::default();
                        }
                        FlowsMode::List => {
                            self.state.section = Section::Home;
                        }
                    }
                } else if self.state.section == Section::Vault {
                    if self.state.vault_ui.mode == VaultMode::Create {
                        self.reset_vault_form();
                        self.state.vault_ui.mode = VaultMode::View;
                    } else {
                        self.state.section = Section::Home;
                    }
                } else if self.state.section == Section::Stats {
                    self.state.section = Section::Home;
                }
            }
            crate::ui::keymap::AppAction::NextField => {
                self.advance_focus();
            }
            crate::ui::keymap::AppAction::Submit => {
                if self.state.screen == Screen::Login {
                    self.attempt_login().await?;
                } else if self.state.section == Section::Transactions {
                    self.handle_transactions_submit().await?;
                } else if self.state.section == Section::Wallets {
                    self.handle_wallets_submit().await?;
                } else if self.state.section == Section::Flows {
                    self.handle_flows_submit().await?;
                } else if self.state.section == Section::Vault {
                    self.handle_vault_submit().await?;
                } else if self.state.section == Section::Stats {
                    self.load_stats().await?;
                }
            }
            crate::ui::keymap::AppAction::Backspace => {
                if self.state.screen == Screen::Login {
                    let field = self.active_field_mut();
                    field.pop();
                } else if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Edit
                {
                    self.state.transactions.edit_input.pop();
                } else if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                    && self.state.transactions.quick_active
                {
                    self.state.transactions.quick_input.pop();
                } else if self.state.section == Section::Wallets {
                    self.backspace_wallet_form();
                } else if self.state.section == Section::Flows {
                    self.backspace_flow_form();
                } else if self.state.section == Section::Vault {
                    self.backspace_vault_form();
                }
            }
            crate::ui::keymap::AppAction::Up => {
                if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.select_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Wallets
                    && self.state.wallets.mode == WalletsMode::List
                {
                    self.wallets_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::List
                {
                    self.flows_select_prev();
                }
            }
            crate::ui::keymap::AppAction::Down => {
                if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.select_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Wallets
                    && self.state.wallets.mode == WalletsMode::List
                {
                    self.wallets_select_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::List
                {
                    self.flows_select_next();
                }
            }
            crate::ui::keymap::AppAction::Input(ch) => {
                if self.state.screen == Screen::Login {
                    let field = self.active_field_mut();
                    field.push(ch);
                } else {
                    if self.state.section == Section::Transactions
                        && self.state.transactions.mode == TransactionsMode::Edit
                    {
                        self.state.transactions.edit_input.push(ch);
                        return Ok(());
                    } else if self.state.section == Section::Transactions
                        && self.state.transactions.mode == TransactionsMode::List
                        && self.state.transactions.quick_active
                    {
                        self.state.transactions.quick_input.push(ch);
                        return Ok(());
                    } else if self.handle_form_input(ch) {
                        return Ok(());
                    } else {
                        self.handle_non_login_key(ch).await?;
                    }
                }
            }
            crate::ui::keymap::AppAction::None => {}
        }

        Ok(())
    }

    fn advance_focus(&mut self) {
        if self.state.screen == Screen::Login {
            self.state.login.focus = match self.state.login.focus {
                LoginField::Username => LoginField::Password,
                LoginField::Password => LoginField::Username,
            };
            return;
        }

        match self.state.section {
            Section::Wallets => self.advance_wallet_focus(),
            Section::Flows => self.advance_flow_focus(),
            Section::Vault => self.advance_vault_focus(),
            _ => {}
        }
    }

    fn active_field_mut(&mut self) -> &mut String {
        match self.state.login.focus {
            LoginField::Username => &mut self.state.login.username,
            LoginField::Password => &mut self.state.login.password,
        }
    }

    fn advance_wallet_focus(&mut self) {
        if !matches!(
            self.state.wallets.mode,
            WalletsMode::Create | WalletsMode::Rename
        ) {
            return;
        }

        if self.state.wallets.mode == WalletsMode::Rename {
            self.state.wallets.form.focus = WalletFormField::Name;
            return;
        }

        self.state.wallets.form.focus = match self.state.wallets.form.focus {
            WalletFormField::Name => WalletFormField::Opening,
            WalletFormField::Opening => WalletFormField::Name,
        };
    }

    fn advance_flow_focus(&mut self) {
        if !matches!(
            self.state.flows.mode,
            FlowsMode::Create | FlowsMode::Rename
        ) {
            return;
        }

        if self.state.flows.mode == FlowsMode::Rename {
            self.state.flows.form.focus = FlowFormField::Name;
            return;
        }

        self.state.flows.form.focus = match self.state.flows.form.focus {
            FlowFormField::Name => FlowFormField::Mode,
            FlowFormField::Mode => FlowFormField::Cap,
            FlowFormField::Cap => FlowFormField::Opening,
            FlowFormField::Opening => FlowFormField::Name,
        };
    }

    fn advance_vault_focus(&mut self) {
        if self.state.vault_ui.mode != VaultMode::Create {
            return;
        }

        self.state.vault_ui.form.error = None;
    }

    async fn attempt_login(&mut self) -> Result<()> {
        let username = self.state.login.username.trim();
        let password = self.state.login.password.trim();
        let vault_name = self.config.vault.trim();

        if username.is_empty() || password.is_empty() || vault_name.is_empty() {
            self.state.login.message = Some("Compila tutti i campi.".to_string());
            return Ok(());
        }

        match self.client.vault_get(username, password, vault_name).await {
            Ok(vault) => {
                self.state.vault = Some(vault);
                match self
                    .client
                    .vault_snapshot(username, password, vault_name)
                    .await
                {
                    Ok(snapshot) => {
                        self.state.last_flow_id = Some(snapshot.unallocated_flow_id);
                        self.state.snapshot = Some(snapshot);
                        self.state.screen = Screen::Home;
                        self.state.login.message = None;
                        self.load_transactions(true).await?;
                    }
                    Err(err) => {
                        self.state.login.message = Some(login_message_for_error(err));
                    }
                }
            }
            Err(err) => {
                self.state.login.message = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn handle_transactions_submit(&mut self) -> Result<()> {
        match self.state.transactions.mode {
            TransactionsMode::List => {
                if self.state.transactions.quick_active {
                    self.submit_quick_add().await
                } else {
                    self.open_transaction_detail().await
                }
            }
            TransactionsMode::Detail => Ok(()),
            TransactionsMode::Edit => self.apply_transaction_edit().await,
        }
    }

    async fn handle_wallets_submit(&mut self) -> Result<()> {
        match self.state.wallets.mode {
            WalletsMode::List => self.open_wallet_detail().await,
            WalletsMode::Detail => Ok(()),
            WalletsMode::Create => self.submit_wallet_create().await,
            WalletsMode::Rename => self.submit_wallet_rename().await,
        }
    }

    async fn handle_flows_submit(&mut self) -> Result<()> {
        match self.state.flows.mode {
            FlowsMode::List => self.open_flow_detail().await,
            FlowsMode::Detail => Ok(()),
            FlowsMode::Create => self.submit_flow_create().await,
            FlowsMode::Rename => self.submit_flow_rename().await,
        }
    }

    async fn handle_vault_submit(&mut self) -> Result<()> {
        if self.state.vault_ui.mode == VaultMode::Create {
            self.submit_vault_create().await?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn client(&self) -> &Client {
        &self.client
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    async fn handle_non_login_key(&mut self, ch: char) -> Result<()> {
        match ch {
            'h' | 'H' => {
                self.state.section = Section::Home;
                self.state.transactions.mode = TransactionsMode::List;
                return Ok(());
            }
            't' | 'T' => {
                if self.state.section == Section::Transactions {
                    if self.state.transactions.mode == TransactionsMode::List {
                        self.state.transactions.include_transfers =
                            !self.state.transactions.include_transfers;
                        self.load_transactions(true).await?;
                    }
                } else {
                    self.state.section = Section::Transactions;
                    if self.state.transactions.items.is_empty() {
                        self.load_transactions(true).await?;
                    }
                }
                return Ok(());
            }
            'w' | 'W' => {
                self.state.section = Section::Wallets;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.snapshot.is_none() {
                    self.refresh_snapshot().await?;
                }
                return Ok(());
            }
            'f' | 'F' => {
                self.state.section = Section::Flows;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.snapshot.is_none() {
                    self.refresh_snapshot().await?;
                }
                return Ok(());
            }
            'v' | 'V' => {
                if self.state.section == Section::Transactions {
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.void_transaction().await?;
                    } else {
                        self.state.transactions.include_voided =
                            !self.state.transactions.include_voided;
                        self.load_transactions(true).await?;
                    }
                } else {
                    self.state.section = Section::Vault;
                    self.state.transactions.mode = TransactionsMode::List;
                }
                return Ok(());
            }
            's' | 'S' => {
                self.state.section = Section::Stats;
                self.state.transactions.mode = TransactionsMode::List;
                self.load_stats().await?;
                return Ok(());
            }
            'r' | 'R' => {
                if self.state.section == Section::Transactions {
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.repeat_transaction().await?;
                    } else if self.state.transactions.mode == TransactionsMode::List {
                        self.load_transactions(true).await?;
                    }
                } else if self.state.section == Section::Stats {
                    self.load_stats().await?;
                } else if self.state.section == Section::Wallets || self.state.section == Section::Flows
                {
                    self.refresh_snapshot().await?;
                }
                return Ok(());
            }
            'n' | 'N' => {
                if self.state.section == Section::Transactions {
                    self.load_transactions_next().await?;
                }
                return Ok(());
            }
            'p' | 'P' => {
                if self.state.section == Section::Transactions {
                    self.load_transactions_prev().await?;
                }
                return Ok(());
            }
            'j' | 'J' => {
                if self.state.section == Section::Transactions {
                    self.state.transactions.select_next();
                }
                return Ok(());
            }
            'k' | 'K' => {
                if self.state.section == Section::Transactions {
                    self.state.transactions.select_prev();
                }
                return Ok(());
            }
            'a' | 'A' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.quick_active = true;
                    self.state.transactions.quick_error = None;
                } else if self.state.section == Section::Wallets
                    && self.state.wallets.mode == WalletsMode::List
                {
                    self.toggle_wallet_archive().await?;
                } else if self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::List
                {
                    self.toggle_flow_archive().await?;
                }
                return Ok(());
            }
            'e' | 'E' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Detail
                {
                    self.state.transactions.mode = TransactionsMode::Edit;
                    self.state.transactions.edit_input.clear();
                    self.state.transactions.edit_error = None;
                } else if self.state.section == Section::Wallets
                    && self.state.wallets.mode == WalletsMode::List
                {
                    self.start_wallet_rename();
                } else if self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::List
                {
                    self.start_flow_rename();
                }
                return Ok(());
            }
            'b' | 'B' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode != TransactionsMode::List
                {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.detail = None;
                    self.state.transactions.edit_input.clear();
                    self.state.transactions.edit_error = None;
                } else if self.state.section == Section::Wallets
                    && self.state.wallets.mode != WalletsMode::List
                {
                    self.state.wallets.mode = WalletsMode::List;
                    self.state.wallets.detail = WalletDetailState::default();
                    self.reset_wallet_form();
                } else if self.state.section == Section::Flows
                    && self.state.flows.mode != FlowsMode::List
                {
                    self.state.flows.mode = FlowsMode::List;
                    self.state.flows.detail = FlowDetailState::default();
                    self.reset_flow_form();
                }
                return Ok(());
            }
            'c' | 'C' => {
                if self.state.section == Section::Wallets
                    && self.state.wallets.mode == WalletsMode::List
                {
                    self.start_wallet_create();
                } else if self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::List
                {
                    self.start_flow_create();
                } else if self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::View
                {
                    self.start_vault_create();
                }
                return Ok(());
            }
            'm' | 'M' => {
                if self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::Create
                    && self.state.flows.form.focus == FlowFormField::Mode
                {
                    self.cycle_flow_mode();
                    return Ok(());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_form_input(&mut self, ch: char) -> bool {
        match self.state.section {
            Section::Wallets => {
                if matches!(
                    self.state.wallets.mode,
                    WalletsMode::Create | WalletsMode::Rename
                ) {
                    match self.state.wallets.form.focus {
                        WalletFormField::Name => self.state.wallets.form.name.push(ch),
                        WalletFormField::Opening => self.state.wallets.form.opening.push(ch),
                    }
                    return true;
                }
            }
            Section::Flows => {
                if matches!(
                    self.state.flows.mode,
                    FlowsMode::Create | FlowsMode::Rename
                ) {
                    match self.state.flows.form.focus {
                        FlowFormField::Name => self.state.flows.form.name.push(ch),
                        FlowFormField::Cap => self.state.flows.form.cap.push(ch),
                        FlowFormField::Opening => self.state.flows.form.opening.push(ch),
                        FlowFormField::Mode => {
                            if matches!(ch, 'm' | 'M' | ' ') {
                                self.cycle_flow_mode();
                            }
                            return true;
                        }
                    }
                    return true;
                }
            }
            Section::Vault => {
                if self.state.vault_ui.mode == VaultMode::Create {
                    self.state.vault_ui.form.name.push(ch);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn backspace_wallet_form(&mut self) {
        if !matches!(
            self.state.wallets.mode,
            WalletsMode::Create | WalletsMode::Rename
        ) {
            return;
        }
        match self.state.wallets.form.focus {
            WalletFormField::Name => {
                self.state.wallets.form.name.pop();
            }
            WalletFormField::Opening => {
                self.state.wallets.form.opening.pop();
            }
        }
    }

    fn backspace_flow_form(&mut self) {
        if !matches!(
            self.state.flows.mode,
            FlowsMode::Create | FlowsMode::Rename
        ) {
            return;
        }
        match self.state.flows.form.focus {
            FlowFormField::Name => {
                self.state.flows.form.name.pop();
            }
            FlowFormField::Cap => {
                self.state.flows.form.cap.pop();
            }
            FlowFormField::Opening => {
                self.state.flows.form.opening.pop();
            }
            FlowFormField::Mode => {}
        }
    }

    fn backspace_vault_form(&mut self) {
        if self.state.vault_ui.mode != VaultMode::Create {
            return;
        }
        self.state.vault_ui.form.name.pop();
    }

    fn reset_wallet_form(&mut self) {
        self.state.wallets.form = WalletFormState::default();
        self.state.wallets.error = None;
    }

    fn reset_flow_form(&mut self) {
        self.state.flows.form = FlowFormState::default();
        self.state.flows.error = None;
    }

    fn reset_vault_form(&mut self) {
        self.state.vault_ui.form = VaultFormState::default();
        self.state.vault_ui.error = None;
    }

    fn wallets_select_next(&mut self) {
        let len = self.wallets_len();
        if len == 0 {
            return;
        }
        self.state.wallets.selected = (self.state.wallets.selected + 1).min(len - 1);
    }

    fn wallets_select_prev(&mut self) {
        if self.wallets_len() == 0 {
            return;
        }
        self.state.wallets.selected = self.state.wallets.selected.saturating_sub(1);
    }

    fn flows_select_next(&mut self) {
        let len = self.flows_len();
        if len == 0 {
            return;
        }
        self.state.flows.selected = (self.state.flows.selected + 1).min(len - 1);
    }

    fn flows_select_prev(&mut self) {
        if self.flows_len() == 0 {
            return;
        }
        self.state.flows.selected = self.state.flows.selected.saturating_sub(1);
    }

    fn wallets_len(&self) -> usize {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| snap.wallets.len())
            .unwrap_or(0)
    }

    fn flows_len(&self) -> usize {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| snap.flows.len())
            .unwrap_or(0)
    }

    fn start_wallet_create(&mut self) {
        self.reset_wallet_form();
        self.state.wallets.mode = WalletsMode::Create;
    }

    fn start_wallet_rename(&mut self) {
        let Some(name) = self
            .selected_wallet()
            .map(|wallet| wallet.name.clone())
        else {
            self.state.wallets.error = Some("Nessun wallet selezionato.".to_string());
            return;
        };
        self.reset_wallet_form();
        self.state.wallets.form.name = name;
        self.state.wallets.mode = WalletsMode::Rename;
        self.state.wallets.form.focus = WalletFormField::Name;
    }

    fn start_flow_create(&mut self) {
        self.reset_flow_form();
        self.state.flows.mode = FlowsMode::Create;
    }

    fn start_flow_rename(&mut self) {
        let Some((name, is_unallocated)) = self
            .selected_flow()
            .map(|flow| (flow.name.clone(), flow.is_unallocated))
        else {
            self.state.flows.error = Some("Nessun flow selezionato.".to_string());
            return;
        };
        if is_unallocated {
            self.state.flows.error = Some("Unallocated non si può rinominare.".to_string());
            return;
        }
        self.reset_flow_form();
        self.state.flows.form.name = name;
        self.state.flows.mode = FlowsMode::Rename;
        self.state.flows.form.focus = FlowFormField::Name;
    }

    fn start_vault_create(&mut self) {
        self.reset_vault_form();
        self.state.vault_ui.mode = VaultMode::Create;
    }

    fn cycle_flow_mode(&mut self) {
        self.state.flows.form.mode = match self.state.flows.form.mode {
            FlowModeChoice::Unlimited => FlowModeChoice::NetCapped,
            FlowModeChoice::NetCapped => FlowModeChoice::IncomeCapped,
            FlowModeChoice::IncomeCapped => FlowModeChoice::Unlimited,
        };
    }

    async fn refresh_snapshot(&mut self) -> Result<()> {
        let vault_name = self.current_vault_name();
        let res = self
            .client
            .vault_snapshot(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                vault_name.as_str(),
            )
            .await;

        match res {
            Ok(snapshot) => {
                self.state.snapshot = Some(snapshot);
                self.ensure_last_flow();
                let wallets_len = self.wallets_len();
                if wallets_len == 0 {
                    self.state.wallets.selected = 0;
                } else if self.state.wallets.selected >= wallets_len {
                    self.state.wallets.selected = wallets_len - 1;
                }

                let flows_len = self.flows_len();
                if flows_len == 0 {
                    self.state.flows.selected = 0;
                } else if self.state.flows.selected >= flows_len {
                    self.state.flows.selected = flows_len - 1;
                }
            }
            Err(err) => {
                let message = login_message_for_error(err);
                self.state.wallets.error = Some(message.clone());
                self.state.flows.error = Some(message.clone());
                self.state.stats.error = Some(message);
            }
        }

        Ok(())
    }

    fn ensure_last_flow(&mut self) {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            return;
        };
        let last_valid = self
            .state
            .last_flow_id
            .and_then(|last| snapshot.flows.iter().find(|flow| flow.id == last))
            .map(|flow| flow.id);
        self.state.last_flow_id = last_valid.or(Some(snapshot.unallocated_flow_id));
    }

    fn current_vault_name(&self) -> String {
        self.state
            .vault
            .as_ref()
            .and_then(|vault| vault.name.clone())
            .unwrap_or_else(|| self.config.vault.clone())
    }

    fn current_vault_id(&self) -> Result<String> {
        self.state
            .vault
            .as_ref()
            .and_then(|vault| vault.id.clone())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))
    }

    fn selected_wallet(&self) -> Option<&api_types::vault::WalletView> {
        self.state
            .snapshot
            .as_ref()
            .and_then(|snap| snap.wallets.get(self.state.wallets.selected))
    }

    fn selected_flow(&self) -> Option<&api_types::vault::FlowView> {
        self.state
            .snapshot
            .as_ref()
            .and_then(|snap| snap.flows.get(self.state.flows.selected))
    }

    fn select_wallet_by_id(&mut self, wallet_id: uuid::Uuid) {
        if let Some(snapshot) = &self.state.snapshot {
            if let Some(index) = snapshot.wallets.iter().position(|w| w.id == wallet_id) {
                self.state.wallets.selected = index;
            }
        }
    }

    fn select_flow_by_id(&mut self, flow_id: uuid::Uuid) {
        if let Some(snapshot) = &self.state.snapshot {
            if let Some(index) = snapshot.flows.iter().position(|f| f.id == flow_id) {
                self.state.flows.selected = index;
            }
        }
    }

    async fn load_transactions(&mut self, reset: bool) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;

        if reset {
            self.state.transactions.reset();
        }

        let payload = TransactionList {
            vault_id: vault_id.to_string(),
            flow_id: None,
            wallet_id: None,
            limit: Some(20),
            cursor: self.state.transactions.cursor.clone(),
            from: None,
            to: None,
            kinds: None,
            include_voided: Some(self.state.transactions.include_voided),
            include_transfers: Some(self.state.transactions.include_transfers),
        };

        let res = self
            .client
            .transactions_list(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                payload,
            )
            .await;

        match res {
            Ok(TransactionListResponse {
                transactions,
                next_cursor,
            }) => {
                self.state.transactions.items = transactions;
                self.state.transactions.next_cursor = next_cursor;
                self.state.transactions.error = None;
                self.state.transactions.selected = 0;
            }
            Err(err) => {
                self.state.transactions.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn load_transactions_next(&mut self) -> Result<()> {
        if let Some(next) = self.state.transactions.next_cursor.clone() {
            self.state
                .transactions
                .push_cursor(self.state.transactions.cursor.clone());
            self.state.transactions.cursor = Some(next);
            self.load_transactions(false).await?;
        }
        Ok(())
    }

    async fn load_transactions_prev(&mut self) -> Result<()> {
        if let Some(prev) = self.state.transactions.pop_cursor() {
            self.state.transactions.cursor = prev;
            self.load_transactions(false).await?;
        }
        Ok(())
    }

    async fn open_transaction_detail(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let Some(selected) = self
            .state
            .transactions
            .items
            .get(self.state.transactions.selected)
        else {
            return Ok(());
        };

        let res = self
            .client
            .transaction_detail(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                TransactionGet {
                    vault_id: vault_id.to_string(),
                    id: selected.id,
                },
            )
            .await;

        match res {
            Ok(detail) => {
                self.state.transactions.detail = Some(detail);
                self.state.transactions.mode = TransactionsMode::Detail;
                self.state.transactions.edit_input.clear();
                self.state.transactions.edit_error = None;
            }
            Err(err) => {
                self.state.transactions.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn void_transaction(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let Some(detail) = self.state.transactions.detail.as_ref() else {
            return Ok(());
        };

        let res = self
            .client
            .transaction_void(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                detail.transaction.id,
                TransactionVoid {
                    vault_id: vault_id.to_string(),
                    voided_at: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.state.transactions.mode = TransactionsMode::List;
                self.state.transactions.detail = None;
                self.load_transactions(true).await?;
            }
            Err(err) => {
                self.state.transactions.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn apply_transaction_edit(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let Some(detail) = self.state.transactions.detail.as_ref() else {
            return Ok(());
        };

        let currency = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur);

        let input = self.state.transactions.edit_input.trim();
        if input.is_empty() {
            self.state.transactions.edit_error = Some("Inserisci: importo [nota]".to_string());
            return Ok(());
        }

        let mut parts = input.splitn(2, ' ');
        let amount_raw = parts.next().unwrap_or("");
        let note = parts.next().map(str::trim).filter(|s| !s.is_empty());
        let amount = match Money::parse_major(amount_raw, currency) {
            Ok(money) => money.minor().abs(),
            Err(_) => {
                self.state.transactions.edit_error = Some("Importo non valido".to_string());
                return Ok(());
            }
        };

        let res = self
            .client
            .transaction_update(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                detail.transaction.id,
                TransactionUpdate {
                    vault_id: vault_id.to_string(),
                    amount_minor: Some(amount),
                    wallet_id: None,
                    flow_id: None,
                    from_wallet_id: None,
                    to_wallet_id: None,
                    from_flow_id: None,
                    to_flow_id: None,
                    category: None,
                    note: note.map(|s| s.to_string()),
                    occurred_at: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.state.transactions.mode = TransactionsMode::Detail;
                self.state.transactions.edit_input.clear();
                self.state.transactions.edit_error = None;
                self.load_transactions(true).await?;
            }
            Err(err) => {
                self.state.transactions.edit_error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn repeat_transaction(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let Some(detail) = self.state.transactions.detail.as_ref() else {
            return Ok(());
        };
        let occurred_at = self.now_in_timezone();

        let mut last_flow_id = None;
        let res = match detail.transaction.kind {
            api_types::transaction::TransactionKind::Income => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                last_flow_id = flow_id;
                self.client
                    .income_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        IncomeNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            flow_id,
                            wallet_id,
                            category: detail.transaction.category.clone(),
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            api_types::transaction::TransactionKind::Expense => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                last_flow_id = flow_id;
                self.client
                    .expense_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        ExpenseNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            flow_id,
                            wallet_id,
                            category: detail.transaction.category.clone(),
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            api_types::transaction::TransactionKind::Refund => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                last_flow_id = flow_id;
                self.client
                    .refund_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        Refund {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            flow_id,
                            wallet_id,
                            category: detail.transaction.category.clone(),
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            api_types::transaction::TransactionKind::TransferWallet => {
                let (from_wallet_id, to_wallet_id) = extract_wallet_transfer(detail)?;
                self.client
                    .transfer_wallet_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        TransferWalletNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            from_wallet_id,
                            to_wallet_id,
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            api_types::transaction::TransactionKind::TransferFlow => {
                let (from_flow_id, to_flow_id) = extract_flow_transfer(detail)?;
                self.client
                    .transfer_flow_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        TransferFlowNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: detail.transaction.amount_minor,
                            from_flow_id,
                            to_flow_id,
                            note: detail.transaction.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
        };

        match res {
            Ok(_) => {
                if let Some(flow_id) = last_flow_id {
                    self.state.last_flow_id = Some(flow_id);
                }
                self.load_transactions(true).await?;
            }
            Err(err) => {
                self.state.transactions.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    fn now_in_timezone(&self) -> DateTime<FixedOffset> {
        let tz = Tz::from_str(self.config.timezone.as_str()).unwrap_or(Tz::UTC);
        let now = Utc::now();
        let local = tz.from_utc_datetime(&now.naive_utc());
        let offset = local.offset().fix();
        local.with_timezone(&offset)
    }

    async fn submit_quick_add(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;

        let (wallet_id, flow_id, _wallet_name, _flow_name) =
            match default_wallet_flow(&self.state) {
                Ok(res) => res,
                Err(message) => {
                    self.state.transactions.quick_error = Some(message);
                    return Ok(());
                }
            };

        let currency = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur);

        let parsed = match crate::quick_add::parse(&self.state.transactions.quick_input, currency)
        {
            Ok(parsed) => parsed,
            Err(message) => {
                self.state.transactions.quick_error = Some(message);
                return Ok(());
            }
        };

        let occurred_at = self.now_in_timezone();
        let res = match parsed.kind {
            QuickAddKind::Income => {
                self.client
                    .income_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        IncomeNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: parsed.amount_minor,
                            flow_id: Some(flow_id),
                            wallet_id: Some(wallet_id),
                            category: parsed.category.clone(),
                            note: parsed.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            QuickAddKind::Expense => {
                self.client
                    .expense_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        ExpenseNew {
                            vault_id: vault_id.to_string(),
                            amount_minor: parsed.amount_minor,
                            flow_id: Some(flow_id),
                            wallet_id: Some(wallet_id),
                            category: parsed.category.clone(),
                            note: parsed.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
            QuickAddKind::Refund => {
                self.client
                    .refund_new(
                        self.state.login.username.as_str(),
                        self.state.login.password.as_str(),
                        Refund {
                            vault_id: vault_id.to_string(),
                            amount_minor: parsed.amount_minor,
                            flow_id: Some(flow_id),
                            wallet_id: Some(wallet_id),
                            category: parsed.category.clone(),
                            note: parsed.note.clone(),
                            idempotency_key: None,
                            occurred_at,
                        },
                    )
                    .await
            }
        };

        match res {
            Ok(_) => {
                self.state.last_flow_id = Some(flow_id);
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.load_transactions(true).await?;
            }
            Err(err) => {
                self.state.transactions.quick_error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn open_wallet_detail(&mut self) -> Result<()> {
        let Some(wallet_id) = self.selected_wallet().map(|wallet| wallet.id) else {
            self.state.wallets.error = Some("Nessun wallet selezionato.".to_string());
            return Ok(());
        };
        self.state.wallets.detail.wallet_id = Some(wallet_id);
        self.state.wallets.mode = WalletsMode::Detail;
        self.load_wallet_transactions(wallet_id).await?;
        Ok(())
    }

    async fn load_wallet_transactions(&mut self, wallet_id: uuid::Uuid) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let payload = TransactionList {
            vault_id,
            flow_id: None,
            wallet_id: Some(wallet_id),
            limit: Some(10),
            cursor: None,
            from: None,
            to: None,
            kinds: None,
            include_voided: Some(false),
            include_transfers: Some(false),
        };
        let res = self
            .client
            .transactions_list(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                payload,
            )
            .await;

        match res {
            Ok(list) => {
                self.state.wallets.detail.transactions = list.transactions;
                self.state.wallets.detail.error = None;
            }
            Err(err) => {
                self.state.wallets.detail.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn submit_wallet_create(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let name = self.state.wallets.form.name.trim();
        if name.is_empty() {
            self.state.wallets.form.error = Some("Inserisci un nome.".to_string());
            return Ok(());
        }

        let currency = self.current_currency();
        let opening_raw = self.state.wallets.form.opening.trim();
        let opening_raw = if opening_raw.is_empty() { "0" } else { opening_raw };
        let opening = match Money::parse_major(opening_raw, currency) {
            Ok(money) => money.minor(),
            Err(_) => {
                self.state.wallets.form.error = Some("Saldo iniziale non valido.".to_string());
                return Ok(());
            }
        };

        let res = self
            .client
            .wallet_new(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                WalletNew {
                    vault_id,
                    name: name.to_string(),
                    opening_balance_minor: opening,
                    occurred_at: self.now_in_timezone(),
                },
            )
            .await;

        match res {
            Ok(created) => {
                self.reset_wallet_form();
                self.state.wallets.mode = WalletsMode::List;
                self.refresh_snapshot().await?;
                self.select_wallet_by_id(created.id);
            }
            Err(err) => {
                self.state.wallets.form.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn submit_wallet_rename(&mut self) -> Result<()> {
        let Some(wallet) = self.selected_wallet() else {
            self.state.wallets.form.error = Some("Nessun wallet selezionato.".to_string());
            return Ok(());
        };
        let name = self.state.wallets.form.name.trim();
        if name.is_empty() {
            self.state.wallets.form.error = Some("Inserisci un nome.".to_string());
            return Ok(());
        }

        let res = self
            .client
            .wallet_update(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                wallet.id,
                WalletUpdate {
                    vault_id: self.current_vault_id()?,
                    name: Some(name.to_string()),
                    archived: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.reset_wallet_form();
                self.state.wallets.mode = WalletsMode::List;
                self.refresh_snapshot().await?;
            }
            Err(err) => {
                self.state.wallets.form.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn toggle_wallet_archive(&mut self) -> Result<()> {
        let Some(wallet) = self.selected_wallet() else {
            self.state.wallets.error = Some("Nessun wallet selezionato.".to_string());
            return Ok(());
        };
        let res = self
            .client
            .wallet_update(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                wallet.id,
                WalletUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(!wallet.archived),
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.refresh_snapshot().await?;
            }
            Err(err) => {
                self.state.wallets.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn open_flow_detail(&mut self) -> Result<()> {
        let Some(flow_id) = self.selected_flow().map(|flow| flow.id) else {
            self.state.flows.error = Some("Nessun flow selezionato.".to_string());
            return Ok(());
        };
        self.state.flows.detail.flow_id = Some(flow_id);
        self.state.flows.mode = FlowsMode::Detail;
        self.load_flow_transactions(flow_id).await?;
        Ok(())
    }

    async fn load_flow_transactions(&mut self, flow_id: uuid::Uuid) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let payload = TransactionList {
            vault_id,
            flow_id: Some(flow_id),
            wallet_id: None,
            limit: Some(10),
            cursor: None,
            from: None,
            to: None,
            kinds: None,
            include_voided: Some(false),
            include_transfers: Some(false),
        };
        let res = self
            .client
            .transactions_list(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                payload,
            )
            .await;

        match res {
            Ok(list) => {
                self.state.flows.detail.transactions = list.transactions;
                self.state.flows.detail.error = None;
            }
            Err(err) => {
                self.state.flows.detail.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn submit_flow_create(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let name = self.state.flows.form.name.trim().to_string();
        if name.is_empty() {
            self.state.flows.form.error = Some("Inserisci un nome.".to_string());
            return Ok(());
        }

        let currency = self.current_currency();
        let opening_raw = self.state.flows.form.opening.trim();
        let opening_raw = if opening_raw.is_empty() { "0" } else { opening_raw };
        let opening = match Money::parse_major(opening_raw, currency) {
            Ok(money) => money.minor(),
            Err(_) => {
                self.state.flows.form.error = Some("Saldo iniziale non valido.".to_string());
                return Ok(());
            }
        };
        if opening < 0 {
            self.state.flows.form.error =
                Some("Saldo iniziale deve essere >= 0.".to_string());
            return Ok(());
        }

        let mode = match self.state.flows.form.mode {
            FlowModeChoice::Unlimited => FlowMode::Unlimited,
            FlowModeChoice::NetCapped => {
                let cap = match self.parse_flow_cap(currency) {
                    Some(cap) => cap,
                    None => return Ok(()),
                };
                FlowMode::NetCapped { cap_minor: cap }
            }
            FlowModeChoice::IncomeCapped => {
                let cap = match self.parse_flow_cap(currency) {
                    Some(cap) => cap,
                    None => return Ok(()),
                };
                FlowMode::IncomeCapped { cap_minor: cap }
            }
        };

        let res = self
            .client
            .flow_new(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                FlowNew {
                    vault_id,
                    name,
                    mode,
                    opening_balance_minor: opening,
                    occurred_at: self.now_in_timezone(),
                },
            )
            .await;

        match res {
            Ok(created) => {
                self.reset_flow_form();
                self.state.flows.mode = FlowsMode::List;
                self.refresh_snapshot().await?;
                self.select_flow_by_id(created.id);
            }
            Err(err) => {
                self.state.flows.form.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn submit_flow_rename(&mut self) -> Result<()> {
        let Some(flow) = self.selected_flow() else {
            self.state.flows.form.error = Some("Nessun flow selezionato.".to_string());
            return Ok(());
        };
        if flow.is_unallocated {
            self.state.flows.form.error = Some("Unallocated non si può rinominare.".to_string());
            return Ok(());
        }
        let name = self.state.flows.form.name.trim();
        if name.is_empty() {
            self.state.flows.form.error = Some("Inserisci un nome.".to_string());
            return Ok(());
        }

        let res = self
            .client
            .flow_update(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                flow.id,
                FlowUpdate {
                    vault_id: self.current_vault_id()?,
                    name: Some(name.to_string()),
                    archived: None,
                    mode: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.reset_flow_form();
                self.state.flows.mode = FlowsMode::List;
                self.refresh_snapshot().await?;
            }
            Err(err) => {
                self.state.flows.form.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn toggle_flow_archive(&mut self) -> Result<()> {
        let Some(flow) = self.selected_flow() else {
            self.state.flows.error = Some("Nessun flow selezionato.".to_string());
            return Ok(());
        };
        if flow.is_unallocated {
            self.state.flows.error = Some("Unallocated non si può archiviare.".to_string());
            return Ok(());
        }
        let res = self
            .client
            .flow_update(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                flow.id,
                FlowUpdate {
                    vault_id: self.current_vault_id()?,
                    name: None,
                    archived: Some(!flow.archived),
                    mode: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.refresh_snapshot().await?;
            }
            Err(err) => {
                self.state.flows.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn submit_vault_create(&mut self) -> Result<()> {
        let name = self.state.vault_ui.form.name.trim();
        if name.is_empty() {
            self.state.vault_ui.form.error = Some("Inserisci un nome.".to_string());
            return Ok(());
        }

        let res = self
            .client
            .vault_new(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                VaultNew {
                    name: name.to_string(),
                    currency: Some(api_types::Currency::Eur),
                },
            )
            .await;

        match res {
            Ok(vault) => {
                self.state.vault = Some(vault);
                self.state.vault_ui.mode = VaultMode::View;
                self.reset_vault_form();
                self.refresh_snapshot().await?;
            }
            Err(err) => {
                self.state.vault_ui.form.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    async fn load_stats(&mut self) -> Result<()> {
        let payload = Vault {
            id: self.state.vault.as_ref().and_then(|v| v.id.clone()),
            name: self.state.vault.as_ref().and_then(|v| v.name.clone()),
            currency: None,
        };

        let res = self
            .client
            .stats_get(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                payload,
            )
            .await;

        match res {
            Ok(stat) => {
                self.state.stats.data = Some(stat);
                self.state.stats.error = None;
            }
            Err(err) => {
                self.state.stats.error = Some(login_message_for_error(err));
            }
        }

        Ok(())
    }

    fn current_currency(&self) -> engine::Currency {
        self.state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur)
    }

    fn parse_flow_cap(&mut self, currency: engine::Currency) -> Option<i64> {
        let cap_raw = self.state.flows.form.cap.trim();
        if cap_raw.is_empty() {
            self.state.flows.form.error = Some("Inserisci un cap.".to_string());
            return None;
        }
        let cap = match Money::parse_major(cap_raw, currency) {
            Ok(money) => money.minor().abs(),
            Err(_) => {
                self.state.flows.form.error = Some("Cap non valido.".to_string());
                return None;
            }
        };
        if cap <= 0 {
            self.state.flows.form.error = Some("Cap deve essere > 0.".to_string());
            return None;
        }
        Some(cap)
    }
}

#[derive(Debug)]
pub struct TransactionsState {
    pub items: Vec<TransactionView>,
    pub cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub prev_cursors: Vec<Option<String>>,
    pub selected: usize,
    pub include_voided: bool,
    pub include_transfers: bool,
    pub error: Option<String>,
    pub mode: TransactionsMode,
    pub detail: Option<TransactionDetailResponse>,
    pub edit_input: String,
    pub edit_error: Option<String>,
    pub quick_input: String,
    pub quick_error: Option<String>,
    pub quick_active: bool,
}

impl Default for TransactionsState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            cursor: None,
            next_cursor: None,
            prev_cursors: Vec::new(),
            selected: 0,
            include_voided: false,
            include_transfers: false,
            error: None,
            mode: TransactionsMode::List,
            detail: None,
            edit_input: String::new(),
            edit_error: None,
            quick_input: String::new(),
            quick_error: None,
            quick_active: false,
        }
    }
}

impl TransactionsState {
    fn reset(&mut self) {
        self.cursor = None;
        self.next_cursor = None;
        self.prev_cursors.clear();
        self.items.clear();
        self.selected = 0;
        self.mode = TransactionsMode::List;
        self.detail = None;
        self.edit_input.clear();
        self.edit_error = None;
        self.quick_input.clear();
        self.quick_error = None;
        self.quick_active = false;
    }

    fn push_cursor(&mut self, cursor: Option<String>) {
        self.prev_cursors.push(cursor);
    }

    fn pop_cursor(&mut self) -> Option<Option<String>> {
        self.prev_cursors.pop()
    }

    fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.items.len() - 1);
    }

    fn select_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionsMode {
    List,
    Detail,
    Edit,
}

#[derive(Debug)]
pub struct WalletsState {
    pub selected: usize,
    pub mode: WalletsMode,
    pub error: Option<String>,
    pub detail: WalletDetailState,
    pub form: WalletFormState,
}

impl Default for WalletsState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: WalletsMode::List,
            error: None,
            detail: WalletDetailState::default(),
            form: WalletFormState::default(),
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

#[derive(Debug)]
pub struct FlowsState {
    pub selected: usize,
    pub mode: FlowsMode,
    pub error: Option<String>,
    pub detail: FlowDetailState,
    pub form: FlowFormState,
}

impl Default for FlowsState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: FlowsMode::List,
            error: None,
            detail: FlowDetailState::default(),
            form: FlowFormState::default(),
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

#[derive(Debug)]
pub struct VaultState {
    pub mode: VaultMode,
    pub form: VaultFormState,
    pub error: Option<String>,
}

impl Default for VaultState {
    fn default() -> Self {
        Self {
            mode: VaultMode::View,
            form: VaultFormState::default(),
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultMode {
    View,
    Create,
}

#[derive(Debug)]
pub struct VaultFormState {
    pub name: String,
    pub error: Option<String>,
}

impl Default for VaultFormState {
    fn default() -> Self {
        Self {
            name: String::new(),
            error: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct StatsState {
    pub data: Option<Statistic>,
    pub error: Option<String>,
}

fn login_message_for_error(err: ClientError) -> String {
    match err {
        ClientError::Unauthorized | ClientError::Forbidden => {
            "Credenziali errate o pairing mancante.".to_string()
        }
        ClientError::NotFound => "Vault non trovato.".to_string(),
        ClientError::Conflict(message) => format!("Conflitto: {message}"),
        ClientError::Validation(message) => format!("Errore di validazione: {message}"),
        ClientError::Server(message) => format!("Errore server: {message}"),
        ClientError::Transport(err) => format!("Server non raggiungibile: {err}"),
    }
}

fn extract_wallet_flow(
    detail: &TransactionDetailResponse,
) -> (Option<uuid::Uuid>, Option<uuid::Uuid>) {
    let mut wallet_id = None;
    let mut flow_id = None;
    for leg in &detail.legs {
        match leg.target {
            api_types::transaction::LegTarget::Wallet { wallet_id: id } => {
                wallet_id = Some(id);
            }
            api_types::transaction::LegTarget::Flow { flow_id: id } => {
                flow_id = Some(id);
            }
        }
    }
    (wallet_id, flow_id)
}

fn extract_wallet_transfer(
    detail: &TransactionDetailResponse,
) -> std::result::Result<(uuid::Uuid, uuid::Uuid), AppError> {
    let mut from_wallet = None;
    let mut to_wallet = None;
    for leg in &detail.legs {
        if let api_types::transaction::LegTarget::Wallet { wallet_id } = leg.target {
            if leg.amount_minor < 0 {
                from_wallet = Some(wallet_id);
            } else if leg.amount_minor > 0 {
                to_wallet = Some(wallet_id);
            }
        }
    }
    match (from_wallet, to_wallet) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::Terminal(
            "impossibile determinare i wallet del transfer".to_string(),
        )),
    }
}

fn extract_flow_transfer(
    detail: &TransactionDetailResponse,
) -> std::result::Result<(uuid::Uuid, uuid::Uuid), AppError> {
    let mut from_flow = None;
    let mut to_flow = None;
    for leg in &detail.legs {
        if let api_types::transaction::LegTarget::Flow { flow_id } = leg.target {
            if leg.amount_minor < 0 {
                from_flow = Some(flow_id);
            } else if leg.amount_minor > 0 {
                to_flow = Some(flow_id);
            }
        }
    }
    match (from_flow, to_flow) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::Terminal(
            "impossibile determinare i flow del transfer".to_string(),
        )),
    }
}

fn map_currency(currency: &api_types::Currency) -> engine::Currency {
    match currency {
        api_types::Currency::Eur => engine::Currency::Eur,
    }
}

fn default_wallet_flow(
    state: &AppState,
) -> std::result::Result<(uuid::Uuid, uuid::Uuid, String, String), String> {
    let snapshot = state
        .snapshot
        .as_ref()
        .ok_or_else(|| "Snapshot non disponibile.".to_string())?;

    let wallet = snapshot
        .wallets
        .iter()
        .find(|wallet| !wallet.archived)
        .ok_or_else(|| "Nessun wallet disponibile.".to_string())?;
    let flow = state
        .last_flow_id
        .and_then(|last_id| {
            snapshot
                .flows
                .iter()
                .find(|flow| flow.id == last_id && !flow.archived)
        })
        .or_else(|| snapshot.flows.iter().find(|flow| flow.is_unallocated))
        .ok_or_else(|| "Flow Unallocated mancante.".to_string())?;

    Ok((wallet.id, flow.id, wallet.name.clone(), flow.name.clone()))
}
