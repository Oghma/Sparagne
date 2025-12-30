use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};

use crate::{
    client::{Client, ClientError},
    config::AppConfig,
    error::{AppError, Result},
    quick_add::QuickAddKind,
    ui,
};

use api_types::{
    flow::{FlowMode, FlowNew, FlowUpdate},
    stats::Statistic,
    transaction::{
        ExpenseNew, IncomeNew, Refund, TransactionDetailResponse, TransactionGet, TransactionKind,
        TransactionList, TransactionListResponse, TransactionUpdate, TransactionView,
        TransactionVoid, TransferFlowNew, TransferWalletNew,
    },
    vault::{Vault, VaultNew, VaultSnapshot},
    wallet::{WalletNew, WalletUpdate},
};
use chrono::{DateTime, Datelike, Duration as ChronoDuration, FixedOffset, Offset, TimeZone, Utc};
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
    pub palette: CommandPaletteState,
    pub help: HelpState,
    pub toast: Option<ToastState>,
    pub connection: ConnectionState,
    pub last_refresh: Option<DateTime<FixedOffset>>,
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
            palette: CommandPaletteState::default(),
            help: HelpState::default(),
            toast: None,
            connection: ConnectionState::default(),
            last_refresh: None,
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
            self.expire_toast();
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
        let action = crate::ui::keymap::map_key(key);
        if self.state.help.active {
            self.handle_help_action(action);
            return Ok(());
        }
        if self.state.palette.active {
            self.handle_palette_action(action).await?;
            return Ok(());
        }

        match action {
            crate::ui::keymap::AppAction::TogglePalette => {
                if self.state.screen == Screen::Home {
                    self.open_palette();
                }
            }
            crate::ui::keymap::AppAction::Search => {
                if self.state.screen == Screen::Home {
                    self.start_search();
                }
            }
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
                } else if self.stop_search_if_active().await? {
                    return Ok(());
                } else if self.state.section == Section::Transactions {
                    match self.state.transactions.mode {
                        TransactionsMode::Edit => {
                            self.state.transactions.mode = TransactionsMode::Detail;
                            self.state.transactions.form = TransactionFormState::default();
                        }
                        TransactionsMode::Detail => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.detail = None;
                        }
                        TransactionsMode::PickWallet | TransactionsMode::PickFlow => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.picker_index = 0;
                        }
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                            if self.state.transactions.transfer.editing_id.is_some() {
                                self.state.transactions.mode = TransactionsMode::Detail;
                            } else {
                                self.state.transactions.mode = TransactionsMode::List;
                            }
                            self.state.transactions.transfer = TransferFormState::default();
                        }
                        TransactionsMode::Form => {
                            if self.state.transactions.form.editing_id.is_some() {
                                self.state.transactions.mode = TransactionsMode::Detail;
                            } else {
                                self.state.transactions.mode = TransactionsMode::List;
                            }
                            self.state.transactions.form = TransactionFormState::default();
                        }
                        TransactionsMode::Filter => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.filter.error = None;
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
                } else if self.handle_search_backspace().await? {
                    return Ok(());
                } else if self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::Form | TransactionsMode::Edit
                    )
                {
                    self.backspace_transaction_form();
                } else if self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
                    )
                {
                    match self.state.transactions.transfer.focus {
                        TransferField::Amount => {
                            self.state.transactions.transfer.amount.pop();
                        }
                        TransferField::Note => {
                            self.state.transactions.transfer.note.pop();
                        }
                        TransferField::OccurredAt => {
                            self.state.transactions.transfer.occurred_at.pop();
                        }
                        _ => {}
                    }
                } else if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Filter
                {
                    match self.state.transactions.filter.focus {
                        FilterField::From => {
                            self.state.transactions.filter.from_input.pop();
                        }
                        FilterField::To => {
                            self.state.transactions.filter.to_input.pop();
                        }
                        FilterField::Kinds => {}
                    }
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
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::List | TransactionsMode::Detail
                    )
                {
                    self.transactions_select_prev();
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.open_transaction_detail().await?;
                    }
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::PickWallet | TransactionsMode::PickFlow
                    )
                {
                    self.transactions_picker_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
                    )
                {
                    self.transfer_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::Form | TransactionsMode::Edit
                    )
                {
                    self.transaction_form_select_prev();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Wallets
                    && matches!(
                        self.state.wallets.mode,
                        WalletsMode::List | WalletsMode::Detail
                    )
                {
                    self.wallets_select_prev();
                    if self.state.wallets.mode == WalletsMode::Detail {
                        self.open_wallet_detail().await?;
                    }
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Flows
                    && matches!(self.state.flows.mode, FlowsMode::List | FlowsMode::Detail)
                {
                    self.flows_select_prev();
                    if self.state.flows.mode == FlowsMode::Detail {
                        self.open_flow_detail().await?;
                    }
                }
            }
            crate::ui::keymap::AppAction::Down => {
                if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::List | TransactionsMode::Detail
                    )
                {
                    self.transactions_select_next();
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.open_transaction_detail().await?;
                    }
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::PickWallet | TransactionsMode::PickFlow
                    )
                {
                    self.transactions_picker_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
                    )
                {
                    self.transfer_select_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::Form | TransactionsMode::Edit
                    )
                {
                    self.transaction_form_select_next();
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Wallets
                    && matches!(
                        self.state.wallets.mode,
                        WalletsMode::List | WalletsMode::Detail
                    )
                {
                    self.wallets_select_next();
                    if self.state.wallets.mode == WalletsMode::Detail {
                        self.open_wallet_detail().await?;
                    }
                } else if self.state.screen == Screen::Home
                    && self.state.section == Section::Flows
                    && matches!(self.state.flows.mode, FlowsMode::List | FlowsMode::Detail)
                {
                    self.flows_select_next();
                    if self.state.flows.mode == FlowsMode::Detail {
                        self.open_flow_detail().await?;
                    }
                }
            }
            crate::ui::keymap::AppAction::Input(ch) => {
                if self.state.screen == Screen::Login {
                    let field = self.active_field_mut();
                    field.push(ch);
                } else {
                    if self.handle_search_input(ch).await? {
                        return Ok(());
                    } else if self.state.section == Section::Transactions
                        && matches!(
                            self.state.transactions.mode,
                            TransactionsMode::Form | TransactionsMode::Edit
                        )
                    {
                        self.handle_transaction_form_input(ch);
                        return Ok(());
                    } else if self.state.section == Section::Transactions
                        && matches!(
                            self.state.transactions.mode,
                            TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
                        )
                    {
                        match self.state.transactions.transfer.focus {
                            TransferField::Amount => {
                                self.state.transactions.transfer.amount.push(ch);
                                return Ok(());
                            }
                            TransferField::Note => {
                                self.state.transactions.transfer.note.push(ch);
                                return Ok(());
                            }
                            TransferField::OccurredAt => {
                                self.state.transactions.transfer.occurred_at.push(ch);
                                return Ok(());
                            }
                            _ => {}
                        }
                    } else if self.state.section == Section::Transactions
                        && self.state.transactions.mode == TransactionsMode::Filter
                    {
                        self.handle_filter_input(ch);
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

        if self.state.section == Section::Transactions
            && matches!(
                self.state.transactions.mode,
                TransactionsMode::Form | TransactionsMode::Edit
            )
        {
            self.advance_transaction_form_focus();
            return;
        }

        if self.state.section == Section::Transactions
            && matches!(
                self.state.transactions.mode,
                TransactionsMode::TransferWallet | TransactionsMode::TransferFlow
            )
        {
            self.advance_transfer_focus();
            return;
        }
        if self.state.section == Section::Transactions
            && self.state.transactions.mode == TransactionsMode::Filter
        {
            self.advance_filter_focus();
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
        if !matches!(self.state.flows.mode, FlowsMode::Create | FlowsMode::Rename) {
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

    fn advance_transfer_focus(&mut self) {
        let transfer = &mut self.state.transactions.transfer;
        transfer.focus = match transfer.focus {
            TransferField::From => TransferField::To,
            TransferField::To => TransferField::Amount,
            TransferField::Amount => TransferField::Note,
            TransferField::Note => TransferField::OccurredAt,
            TransferField::OccurredAt => TransferField::From,
        };
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
            TransactionsMode::Edit | TransactionsMode::Form => self.submit_transaction_form().await,
            TransactionsMode::PickWallet => self.apply_wallet_picker().await,
            TransactionsMode::PickFlow => self.apply_flow_picker().await,
            TransactionsMode::TransferWallet => self.submit_transfer_wallet().await,
            TransactionsMode::TransferFlow => self.submit_transfer_flow().await,
            TransactionsMode::Filter => self.apply_filter().await,
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
            'i' | 'I' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Income).await?;
                }
                return Ok(());
            }
            // Navigation keys - always navigate to section
            't' | 'T' => {
                self.state.section = Section::Transactions;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.transactions.items.is_empty() {
                    self.load_transactions(true).await?;
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
                // In transaction detail, 'v' voids the transaction
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Detail
                {
                    self.void_transaction().await?;
                } else {
                    self.state.section = Section::Vault;
                    self.state.transactions.mode = TransactionsMode::List;
                }
                return Ok(());
            }
            // Transaction list context actions (use different keys)
            'x' | 'X' => {
                // Toggle transfers visibility in transactions list
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.include_transfers =
                        !self.state.transactions.include_transfers;
                    self.load_transactions(true).await?;
                }
                return Ok(());
            }
            'z' | 'Z' => {
                // Toggle voided visibility in transactions list
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.include_voided =
                        !self.state.transactions.include_voided;
                    self.load_transactions(true).await?;
                }
                return Ok(());
            }
            '1' => {
                // Open wallet picker in transactions list
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_wallet_picker();
                }
                return Ok(());
            }
            '2' => {
                // Open flow picker in transactions list
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_flow_picker();
                }
                return Ok(());
            }
            's' | 'S' => {
                self.state.section = Section::Stats;
                self.state.transactions.mode = TransactionsMode::List;
                self.load_stats().await?;
                return Ok(());
            }
            'R' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Refund).await?;
                }
                return Ok(());
            }
            'r' => {
                if self.state.section == Section::Transactions {
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.repeat_transaction().await?;
                    } else if self.state.transactions.mode == TransactionsMode::List {
                        self.load_transactions(true).await?;
                    }
                } else if self.state.section == Section::Stats {
                    self.load_stats().await?;
                } else if self.state.section == Section::Wallets
                    || self.state.section == Section::Flows
                {
                    self.refresh_snapshot().await?;
                }
                return Ok(());
            }
            'n' | 'N' => {
                if self.state.section == Section::Transactions {
                    self.load_transactions_next().await?;
                } else if self.state.section == Section::Stats {
                    self.stats_next_month();
                }
                return Ok(());
            }
            'p' | 'P' => {
                if self.state.section == Section::Transactions {
                    self.load_transactions_prev().await?;
                } else if self.state.section == Section::Stats {
                    self.stats_prev_month();
                }
                return Ok(());
            }
            'j' | 'J' => {
                if self.state.section == Section::Transactions {
                    self.transactions_select_next();
                }
                return Ok(());
            }
            'k' | 'K' => {
                if self.state.section == Section::Transactions {
                    self.transactions_select_prev();
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
            'u' | 'U' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.undo_last_transaction().await?;
                }
                return Ok(());
            }
            'e' | 'E' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Expense)
                        .await?;
                } else if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Detail
                {
                    self.start_transaction_edit().await?;
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
                    match self.state.transactions.mode {
                        TransactionsMode::Detail => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.detail = None;
                        }
                        TransactionsMode::Edit => {
                            self.state.transactions.mode = TransactionsMode::Detail;
                            self.state.transactions.form = TransactionFormState::default();
                        }
                        TransactionsMode::Form => {
                            if self.state.transactions.form.editing_id.is_some() {
                                self.state.transactions.mode = TransactionsMode::Detail;
                            } else {
                                self.state.transactions.mode = TransactionsMode::List;
                            }
                            self.state.transactions.form = TransactionFormState::default();
                        }
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                            if self.state.transactions.transfer.editing_id.is_some() {
                                self.state.transactions.mode = TransactionsMode::Detail;
                            } else {
                                self.state.transactions.mode = TransactionsMode::List;
                            }
                            self.state.transactions.transfer = TransferFormState::default();
                        }
                        TransactionsMode::PickWallet | TransactionsMode::PickFlow => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.picker_index = 0;
                        }
                        TransactionsMode::Filter => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.filter.error = None;
                        }
                        TransactionsMode::List => {}
                    }
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
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.clear_filters().await?;
                } else if self.state.section == Section::Wallets
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
            '/' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_filter();
                }
                return Ok(());
            }
            '?' => {
                if self.state.screen == Screen::Home {
                    self.state.help.active = true;
                }
                return Ok(());
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
                if matches!(self.state.flows.mode, FlowsMode::Create | FlowsMode::Rename) {
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

    fn handle_help_action(&mut self, action: crate::ui::keymap::AppAction) {
        match action {
            crate::ui::keymap::AppAction::Cancel => {
                self.state.help.active = false;
            }
            crate::ui::keymap::AppAction::Input('?') => {
                self.state.help.active = false;
            }
            _ => {}
        }
    }

    fn start_search(&mut self) {
        match self.state.section {
            Section::Transactions => {
                self.state.transactions.search_active = true;
            }
            Section::Wallets => {
                self.state.wallets.search_active = true;
            }
            Section::Flows => {
                self.state.flows.search_active = true;
            }
            _ => {}
        }
    }

    async fn stop_search_if_active(&mut self) -> Result<bool> {
        if self.state.transactions.search_active {
            self.state.transactions.search_active = false;
            self.refresh_transactions_search().await?;
            return Ok(true);
        }
        if self.state.wallets.search_active {
            self.state.wallets.search_active = false;
            self.refresh_wallets_search().await?;
            return Ok(true);
        }
        if self.state.flows.search_active {
            self.state.flows.search_active = false;
            self.refresh_flows_search().await?;
            return Ok(true);
        }
        Ok(false)
    }

    async fn handle_search_input(&mut self, ch: char) -> Result<bool> {
        match self.state.section {
            Section::Transactions if self.state.transactions.search_active => {
                self.state.transactions.search_query.push(ch);
                self.refresh_transactions_search().await?;
                return Ok(true);
            }
            Section::Wallets if self.state.wallets.search_active => {
                self.state.wallets.search_query.push(ch);
                self.refresh_wallets_search().await?;
                return Ok(true);
            }
            Section::Flows if self.state.flows.search_active => {
                self.state.flows.search_query.push(ch);
                self.refresh_flows_search().await?;
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    async fn handle_search_backspace(&mut self) -> Result<bool> {
        match self.state.section {
            Section::Transactions if self.state.transactions.search_active => {
                self.state.transactions.search_query.pop();
                self.refresh_transactions_search().await?;
                return Ok(true);
            }
            Section::Wallets if self.state.wallets.search_active => {
                self.state.wallets.search_query.pop();
                self.refresh_wallets_search().await?;
                return Ok(true);
            }
            Section::Flows if self.state.flows.search_active => {
                self.state.flows.search_query.pop();
                self.refresh_flows_search().await?;
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    async fn refresh_transactions_search(&mut self) -> Result<()> {
        let visible_len = transactions_visible_indices(&self.state).len();
        if visible_len == 0 {
            self.state.transactions.selected = 0;
            self.state.transactions.detail = None;
            return Ok(());
        }
        if self.state.transactions.selected >= visible_len {
            self.state.transactions.selected = 0;
        }
        if self.state.transactions.mode == TransactionsMode::Detail {
            self.open_transaction_detail().await?;
        }
        Ok(())
    }

    async fn refresh_wallets_search(&mut self) -> Result<()> {
        let visible_len = wallets_visible_indices(&self.state).len();
        if visible_len == 0 {
            self.state.wallets.selected = 0;
            self.state.wallets.detail = WalletDetailState::default();
            return Ok(());
        }
        if self.state.wallets.selected >= visible_len {
            self.state.wallets.selected = 0;
        }
        if self.state.wallets.mode == WalletsMode::Detail {
            self.open_wallet_detail().await?;
        }
        Ok(())
    }

    async fn refresh_flows_search(&mut self) -> Result<()> {
        let visible_len = flows_visible_indices(&self.state).len();
        if visible_len == 0 {
            self.state.flows.selected = 0;
            self.state.flows.detail = FlowDetailState::default();
            return Ok(());
        }
        if self.state.flows.selected >= visible_len {
            self.state.flows.selected = 0;
        }
        if self.state.flows.mode == FlowsMode::Detail {
            self.open_flow_detail().await?;
        }
        Ok(())
    }

    fn advance_filter_focus(&mut self) {
        let filter = &mut self.state.transactions.filter;
        filter.focus = match filter.focus {
            FilterField::From => FilterField::To,
            FilterField::To => FilterField::Kinds,
            FilterField::Kinds => FilterField::From,
        };
    }

    fn handle_filter_input(&mut self, ch: char) {
        let filter = &mut self.state.transactions.filter;
        match filter.focus {
            FilterField::From => {
                filter.from_input.push(ch);
            }
            FilterField::To => {
                filter.to_input.push(ch);
            }
            FilterField::Kinds => match ch {
                'i' | 'I' => filter.kind_income = !filter.kind_income,
                'e' | 'E' => filter.kind_expense = !filter.kind_expense,
                'r' | 'R' => filter.kind_refund = !filter.kind_refund,
                'w' | 'W' => filter.kind_transfer_wallet = !filter.kind_transfer_wallet,
                'f' | 'F' => filter.kind_transfer_flow = !filter.kind_transfer_flow,
                _ => {}
            },
        }
    }

    fn expire_toast(&mut self) {
        if let Some(toast) = &self.state.toast {
            if std::time::Instant::now() >= toast.expires_at {
                self.state.toast = None;
            }
        }
    }

    fn set_toast(&mut self, message: &str, level: ToastLevel) {
        self.state.toast = Some(ToastState {
            message: message.to_string(),
            level,
            expires_at: std::time::Instant::now() + Duration::from_secs(3),
        });
    }

    fn connection_ok(&mut self, message: Option<&str>) {
        self.state.connection.ok = true;
        self.state.connection.message = message.map(|msg| msg.to_string());
        self.state.last_refresh = Some(self.now_in_timezone());
    }

    fn connection_error(&mut self, message: &str) {
        self.state.connection.ok = false;
        self.state.connection.message = Some(message.to_string());
    }

    fn handle_auth_error(&mut self, err: &ClientError) -> bool {
        if matches!(err, ClientError::Unauthorized | ClientError::Forbidden) {
            self.state.screen = Screen::Login;
            self.state.login.password.clear();
            self.state.login.message = Some("Credenziali errate o pairing mancante.".to_string());
            self.state.vault = None;
            self.state.snapshot = None;
            self.state.section = Section::Home;
            self.state.transactions = TransactionsState::default();
            return true;
        }
        false
    }

    fn update_recent_categories_from_items(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut categories = Vec::new();
        for tx in &self.state.transactions.items {
            if let Some(category) = tx.category.as_ref() {
                let key = category.to_lowercase();
                if seen.insert(key) {
                    categories.push(category.clone());
                }
            }
            if categories.len() >= 5 {
                break;
            }
        }
        self.state.transactions.recent_categories = categories;
    }

    async fn refresh_recent_targets(&mut self) -> Result<()> {
        const RECENTS_LIMIT: usize = 5;
        const RECENTS_FETCH_LIMIT: u64 = 50;
        const RECENTS_WINDOW_DAYS: i64 = 90;

        let vault_id = match self.current_vault_id() {
            Ok(id) => id,
            Err(_) => return Ok(()),
        };
        let to = self.now_in_timezone();
        let from = to - ChronoDuration::days(RECENTS_WINDOW_DAYS);

        let payload = TransactionList {
            vault_id: vault_id.clone(),
            flow_id: self.state.transactions.scope_flow_id,
            wallet_id: self.state.transactions.scope_wallet_id,
            limit: Some(RECENTS_FETCH_LIMIT),
            cursor: None,
            from: Some(from),
            to: Some(to),
            kinds: Some(vec![
                TransactionKind::Income,
                TransactionKind::Expense,
                TransactionKind::Refund,
            ]),
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

        let Ok(list) = res else {
            return Ok(());
        };

        let mut categories = Vec::new();
        let mut seen_categories = std::collections::HashSet::new();
        for tx in &list.transactions {
            if let Some(category) = tx.category.as_ref() {
                let key = category.to_lowercase();
                if seen_categories.insert(key) {
                    categories.push(category.clone());
                }
            }
            if categories.len() >= RECENTS_LIMIT {
                break;
            }
        }
        if !categories.is_empty() {
            self.state.transactions.recent_categories = categories;
        }

        let mut recent_wallets = Vec::new();
        let mut recent_flows = Vec::new();
        for tx in &list.transactions {
            if recent_wallets.len() >= RECENTS_LIMIT && recent_flows.len() >= RECENTS_LIMIT {
                break;
            }
            let detail = self
                .client
                .transaction_detail(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
                    TransactionGet {
                        vault_id: vault_id.clone(),
                        id: tx.id,
                    },
                )
                .await;
            let Ok(detail) = detail else {
                continue;
            };
            let (wallet_id, flow_id) = extract_wallet_flow(&detail);
            if let Some(wallet_id) = wallet_id {
                push_recent_id(&mut recent_wallets, wallet_id, RECENTS_LIMIT);
            }
            if let Some(flow_id) = flow_id {
                push_recent_id(&mut recent_flows, flow_id, RECENTS_LIMIT);
            }
        }

        if !recent_wallets.is_empty() {
            self.state.transactions.recent_wallet_ids = recent_wallets;
        }
        if !recent_flows.is_empty() {
            self.state.transactions.recent_flow_ids = recent_flows;
        }

        Ok(())
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
        if !matches!(self.state.flows.mode, FlowsMode::Create | FlowsMode::Rename) {
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
        let len = wallets_visible_indices(&self.state).len();
        if len == 0 {
            return;
        }
        self.state.wallets.selected = (self.state.wallets.selected + 1).min(len - 1);
    }

    fn wallets_select_prev(&mut self) {
        if wallets_visible_indices(&self.state).is_empty() {
            return;
        }
        self.state.wallets.selected = self.state.wallets.selected.saturating_sub(1);
    }

    fn flows_select_next(&mut self) {
        let len = flows_visible_indices(&self.state).len();
        if len == 0 {
            return;
        }
        self.state.flows.selected = (self.state.flows.selected + 1).min(len - 1);
    }

    fn flows_select_prev(&mut self) {
        if flows_visible_indices(&self.state).is_empty() {
            return;
        }
        self.state.flows.selected = self.state.flows.selected.saturating_sub(1);
    }

    fn transactions_select_next(&mut self) {
        let len = transactions_visible_indices(&self.state).len();
        if len == 0 {
            return;
        }
        self.state.transactions.selected = (self.state.transactions.selected + 1).min(len - 1);
    }

    fn transactions_select_prev(&mut self) {
        if transactions_visible_indices(&self.state).is_empty() {
            return;
        }
        self.state.transactions.selected = self.state.transactions.selected.saturating_sub(1);
    }

    fn transactions_picker_next(&mut self) {
        let len = self.transactions_picker_len();
        if len == 0 {
            return;
        }
        self.state.transactions.picker_index =
            (self.state.transactions.picker_index + 1).min(len - 1);
    }

    fn transactions_picker_prev(&mut self) {
        let len = self.transactions_picker_len();
        if len == 0 {
            return;
        }
        self.state.transactions.picker_index =
            self.state.transactions.picker_index.saturating_sub(1);
    }

    fn transactions_picker_len(&self) -> usize {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            return 0;
        };
        match self.state.transactions.mode {
            TransactionsMode::PickWallet => snapshot.wallets.len() + 1,
            TransactionsMode::PickFlow => snapshot.flows.len() + 1,
            _ => 0,
        }
    }

    fn open_wallet_picker(&mut self) {
        self.state.transactions.quick_active = false;
        self.state.transactions.picker_index = self
            .state
            .transactions
            .scope_wallet_id
            .and_then(|wallet_id| {
                self.state.snapshot.as_ref().and_then(|snap| {
                    snap.wallets
                        .iter()
                        .position(|wallet| wallet.id == wallet_id)
                })
            })
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.state.transactions.mode = TransactionsMode::PickWallet;
    }

    fn open_flow_picker(&mut self) {
        self.state.transactions.quick_active = false;
        self.state.transactions.picker_index = self
            .state
            .transactions
            .scope_flow_id
            .and_then(|flow_id| {
                self.state
                    .snapshot
                    .as_ref()
                    .and_then(|snap| snap.flows.iter().position(|flow| flow.id == flow_id))
            })
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.state.transactions.mode = TransactionsMode::PickFlow;
    }

    async fn apply_wallet_picker(&mut self) -> Result<()> {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            self.state.transactions.error = Some("Snapshot non disponibile.".to_string());
            self.state.transactions.mode = TransactionsMode::List;
            return Ok(());
        };

        if self.state.transactions.picker_index == 0 {
            self.state.transactions.scope_wallet_id = None;
        } else {
            let index = self.state.transactions.picker_index - 1;
            if let Some(wallet) = snapshot.wallets.get(index) {
                self.state.transactions.scope_wallet_id = Some(wallet.id);
            }
        }

        self.state.transactions.scope_flow_id = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.state.transactions.picker_index = 0;
        self.load_transactions(true).await?;
        Ok(())
    }

    async fn apply_flow_picker(&mut self) -> Result<()> {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            self.state.transactions.error = Some("Snapshot non disponibile.".to_string());
            self.state.transactions.mode = TransactionsMode::List;
            return Ok(());
        };

        if self.state.transactions.picker_index == 0 {
            self.state.transactions.scope_flow_id = None;
        } else {
            let index = self.state.transactions.picker_index - 1;
            if let Some(flow) = snapshot.flows.get(index) {
                self.state.transactions.scope_flow_id = Some(flow.id);
                self.state.last_flow_id = Some(flow.id);
            }
        }

        self.state.transactions.scope_wallet_id = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.state.transactions.picker_index = 0;
        self.load_transactions(true).await?;
        Ok(())
    }

    fn start_transfer_wallet(&mut self) {
        let occurred_at = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.transfer = TransferFormState {
            occurred_at,
            ..TransferFormState::default()
        };
        self.state.transactions.mode = TransactionsMode::TransferWallet;
        self.init_transfer_indices();
    }

    fn start_transfer_flow(&mut self) {
        let occurred_at = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.transfer = TransferFormState {
            occurred_at,
            ..TransferFormState::default()
        };
        self.state.transactions.mode = TransactionsMode::TransferFlow;
        self.init_transfer_indices();
    }

    fn init_transfer_indices(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            self.state.transactions.transfer.error =
                Some("Nessun elemento disponibile.".to_string());
            return;
        }
        self.state.transactions.transfer.from_index = 0;
        self.state.transactions.transfer.to_index = if len > 1 { 1 } else { 0 };
    }

    fn transfer_select_next(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            return;
        }
        match self.state.transactions.transfer.focus {
            TransferField::From => {
                self.state.transactions.transfer.from_index =
                    (self.state.transactions.transfer.from_index + 1) % len;
            }
            TransferField::To => {
                self.state.transactions.transfer.to_index =
                    (self.state.transactions.transfer.to_index + 1) % len;
            }
            _ => {}
        }
    }

    fn transfer_select_prev(&mut self) {
        let len = match self.state.transactions.mode {
            TransactionsMode::TransferWallet => self.active_wallets_len(),
            TransactionsMode::TransferFlow => self.active_flows_len(),
            _ => 0,
        };
        if len == 0 {
            return;
        }
        match self.state.transactions.transfer.focus {
            TransferField::From => {
                self.state.transactions.transfer.from_index =
                    (self.state.transactions.transfer.from_index + len - 1) % len;
            }
            TransferField::To => {
                self.state.transactions.transfer.to_index =
                    (self.state.transactions.transfer.to_index + len - 1) % len;
            }
            _ => {}
        }
    }

    async fn start_transaction_form(&mut self, kind: TransactionKind) -> Result<()> {
        self.state.section = Section::Transactions;
        if self.state.snapshot.is_none() {
            self.refresh_snapshot().await?;
        }
        self.ensure_last_flow();

        let (wallet_index, flow_index) = match self.default_transaction_form_indices() {
            Ok(indices) => indices,
            Err(message) => {
                self.set_toast(&message, ToastLevel::Error);
                return Ok(());
            }
        };

        let occurred_at = self.format_local_datetime(self.now_in_timezone());
        self.state.transactions.form = TransactionFormState {
            kind,
            amount: String::new(),
            wallet_index,
            flow_index,
            category: String::new(),
            note: String::new(),
            occurred_at,
            focus: TransactionFormField::Amount,
            error: None,
            category_index: None,
            editing_id: None,
        };
        self.state.transactions.quick_active = false;
        self.state.transactions.quick_input.clear();
        self.state.transactions.quick_error = None;
        self.state.transactions.mode = TransactionsMode::Form;
        Ok(())
    }

    async fn start_transaction_edit(&mut self) -> Result<()> {
        if self.state.snapshot.is_none() {
            self.refresh_snapshot().await?;
        }

        let Some(detail) = self.state.transactions.detail.as_ref() else {
            return Ok(());
        };
        if detail.transaction.voided {
            self.set_toast(
                "Transazione annullata: modifica non disponibile.",
                ToastLevel::Error,
            );
            return Ok(());
        }

        let currency = self.current_currency();
        let occurred_at = self.format_local_datetime(detail.transaction.occurred_at);
        let amount = format_amount_input(detail.transaction.amount_minor, currency);

        match detail.transaction.kind {
            TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
                let (Some(wallet_id), Some(flow_id)) = (wallet_id, flow_id) else {
                    self.set_toast("Transazione non valida.", ToastLevel::Error);
                    return Ok(());
                };

                let wallet_ids = self.ordered_wallet_ids();
                let flow_ids = self.ordered_flow_ids();
                let Some(wallet_index) = wallet_ids.iter().position(|id| *id == wallet_id) else {
                    self.set_toast(
                        "Wallet archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(flow_index) = flow_ids.iter().position(|id| *id == flow_id) else {
                    self.set_toast(
                        "Flow archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.form = TransactionFormState {
                    kind: detail.transaction.kind,
                    amount,
                    wallet_index,
                    flow_index,
                    category: detail.transaction.category.clone().unwrap_or_default(),
                    note: detail.transaction.note.clone().unwrap_or_default(),
                    occurred_at,
                    focus: TransactionFormField::Amount,
                    error: None,
                    category_index: None,
                    editing_id: Some(detail.transaction.id),
                };
                self.state.transactions.quick_active = false;
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.mode = TransactionsMode::Edit;
            }
            TransactionKind::TransferWallet => {
                let (from_id, to_id) = match extract_wallet_transfer(detail) {
                    Ok(values) => values,
                    Err(_) => {
                        self.set_toast("Transfer wallet non valido.", ToastLevel::Error);
                        return Ok(());
                    }
                };
                let ids = self.active_wallet_ids();
                let Some(from_index) = ids.iter().position(|id| *id == from_id) else {
                    self.set_toast(
                        "Wallet archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(to_index) = ids.iter().position(|id| *id == to_id) else {
                    self.set_toast(
                        "Wallet archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.transfer = TransferFormState {
                    from_index,
                    to_index,
                    amount,
                    note: detail.transaction.note.clone().unwrap_or_default(),
                    occurred_at,
                    focus: TransferField::From,
                    error: None,
                    editing_id: Some(detail.transaction.id),
                };
                self.state.transactions.quick_active = false;
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.mode = TransactionsMode::TransferWallet;
            }
            TransactionKind::TransferFlow => {
                let (from_id, to_id) = match extract_flow_transfer(detail) {
                    Ok(values) => values,
                    Err(_) => {
                        self.set_toast("Transfer flow non valido.", ToastLevel::Error);
                        return Ok(());
                    }
                };
                let ids = self.active_flow_ids();
                let Some(from_index) = ids.iter().position(|id| *id == from_id) else {
                    self.set_toast(
                        "Flow archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };
                let Some(to_index) = ids.iter().position(|id| *id == to_id) else {
                    self.set_toast(
                        "Flow archiviato: modifica non disponibile.",
                        ToastLevel::Error,
                    );
                    return Ok(());
                };

                self.state.transactions.transfer = TransferFormState {
                    from_index,
                    to_index,
                    amount,
                    note: detail.transaction.note.clone().unwrap_or_default(),
                    occurred_at,
                    focus: TransferField::From,
                    error: None,
                    editing_id: Some(detail.transaction.id),
                };
                self.state.transactions.quick_active = false;
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.state.transactions.mode = TransactionsMode::TransferFlow;
            }
        }

        Ok(())
    }

    fn default_transaction_form_indices(&self) -> std::result::Result<(usize, usize), String> {
        let (default_wallet_id, default_flow_id, _wallet_name, _flow_name) =
            default_wallet_flow(&self.state)?;
        let wallet_ids = self.ordered_wallet_ids();
        let flow_ids = self.ordered_flow_ids();
        if wallet_ids.is_empty() {
            return Err("Nessun wallet disponibile.".to_string());
        }
        if flow_ids.is_empty() {
            return Err("Nessun flow disponibile.".to_string());
        }
        let wallet_id = if self.state.transactions.scope_wallet_id.is_some() {
            default_wallet_id
        } else {
            self.state
                .transactions
                .recent_wallet_ids
                .iter()
                .find(|id| wallet_ids.contains(id))
                .copied()
                .unwrap_or(default_wallet_id)
        };
        let flow_id = if self.state.transactions.scope_flow_id.is_some() {
            default_flow_id
        } else {
            self.state
                .transactions
                .recent_flow_ids
                .iter()
                .find(|id| flow_ids.contains(id))
                .copied()
                .unwrap_or(default_flow_id)
        };

        let wallet_index = wallet_ids
            .iter()
            .position(|id| *id == wallet_id)
            .unwrap_or(0);
        let flow_index = flow_ids.iter().position(|id| *id == flow_id).unwrap_or(0);
        Ok((wallet_index, flow_index))
    }

    fn advance_transaction_form_focus(&mut self) {
        let form = &mut self.state.transactions.form;
        form.error = None;
        form.focus = match form.focus {
            TransactionFormField::Amount => TransactionFormField::Wallet,
            TransactionFormField::Wallet => TransactionFormField::Flow,
            TransactionFormField::Flow => TransactionFormField::Category,
            TransactionFormField::Category => TransactionFormField::Note,
            TransactionFormField::Note => TransactionFormField::OccurredAt,
            TransactionFormField::OccurredAt => TransactionFormField::Amount,
        };
    }

    fn handle_transaction_form_input(&mut self, ch: char) {
        let form = &mut self.state.transactions.form;
        form.error = None;
        match form.focus {
            TransactionFormField::Amount => form.amount.push(ch),
            TransactionFormField::Category => {
                form.category.push(ch);
                form.category_index = None;
            }
            TransactionFormField::Note => form.note.push(ch),
            TransactionFormField::OccurredAt => form.occurred_at.push(ch),
            TransactionFormField::Wallet | TransactionFormField::Flow => {}
        }
    }

    fn backspace_transaction_form(&mut self) {
        let form = &mut self.state.transactions.form;
        form.error = None;
        match form.focus {
            TransactionFormField::Amount => {
                form.amount.pop();
            }
            TransactionFormField::Category => {
                form.category.pop();
                form.category_index = None;
            }
            TransactionFormField::Note => {
                form.note.pop();
            }
            TransactionFormField::OccurredAt => {
                form.occurred_at.pop();
            }
            TransactionFormField::Wallet | TransactionFormField::Flow => {}
        }
    }

    fn transaction_form_select_next(&mut self) {
        let focus = self.state.transactions.form.focus;
        self.state.transactions.form.error = None;
        match focus {
            TransactionFormField::Wallet => {
                let len = self.active_wallets_len();
                if len > 0 {
                    let current = self.state.transactions.form.wallet_index;
                    self.state.transactions.form.wallet_index = (current + 1) % len;
                }
            }
            TransactionFormField::Flow => {
                let len = self.active_flows_len();
                if len > 0 {
                    let current = self.state.transactions.form.flow_index;
                    self.state.transactions.form.flow_index = (current + 1) % len;
                }
            }
            TransactionFormField::Category => {
                self.select_category_next();
            }
            _ => {}
        }
    }

    fn transaction_form_select_prev(&mut self) {
        let focus = self.state.transactions.form.focus;
        self.state.transactions.form.error = None;
        match focus {
            TransactionFormField::Wallet => {
                let len = self.active_wallets_len();
                if len > 0 {
                    let current = self.state.transactions.form.wallet_index;
                    self.state.transactions.form.wallet_index = (current + len - 1) % len;
                }
            }
            TransactionFormField::Flow => {
                let len = self.active_flows_len();
                if len > 0 {
                    let current = self.state.transactions.form.flow_index;
                    self.state.transactions.form.flow_index = (current + len - 1) % len;
                }
            }
            TransactionFormField::Category => {
                self.select_category_prev();
            }
            _ => {}
        }
    }

    fn select_category_next(&mut self) {
        let categories = self.state.transactions.recent_categories.clone();
        if categories.is_empty() {
            return;
        }
        let form = &mut self.state.transactions.form;
        let next = match form.category_index {
            Some(idx) => (idx + 1) % categories.len(),
            None => 0,
        };
        form.category_index = Some(next);
        form.category = categories[next].clone();
    }

    fn select_category_prev(&mut self) {
        let categories = self.state.transactions.recent_categories.clone();
        if categories.is_empty() {
            return;
        }
        let form = &mut self.state.transactions.form;
        let prev = match form.category_index {
            Some(idx) => (idx + categories.len() - 1) % categories.len(),
            None => categories.len() - 1,
        };
        form.category_index = Some(prev);
        form.category = categories[prev].clone();
    }

    async fn submit_transaction_form(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let currency = self.current_currency();
        self.state.transactions.form.error = None;
        let (kind, amount_raw, wallet_index, flow_index, category_raw, note_raw, occurred_raw) = {
            let form = &self.state.transactions.form;
            (
                form.kind,
                form.amount.trim().to_string(),
                form.wallet_index,
                form.flow_index,
                form.category.trim().to_string(),
                form.note.trim().to_string(),
                form.occurred_at.trim().to_string(),
            )
        };
        let editing_id = self.state.transactions.form.editing_id;

        let amount_raw = amount_raw.as_str();
        if amount_raw.is_empty() {
            self.set_transaction_form_error("Inserisci un importo.");
            return Ok(());
        }
        let amount_minor = match Money::parse_major(amount_raw, currency) {
            Ok(money) => money.minor().abs(),
            Err(_) => {
                self.set_transaction_form_error("Importo non valido.");
                return Ok(());
            }
        };
        if amount_minor <= 0 {
            self.set_transaction_form_error("Importo deve essere > 0.");
            return Ok(());
        }

        let wallet_ids = self.ordered_wallet_ids();
        if wallet_ids.is_empty() {
            self.set_transaction_form_error("Nessun wallet disponibile.");
            return Ok(());
        }
        let wallet_id = match wallet_ids.get(wallet_index) {
            Some(id) => *id,
            None => {
                self.set_transaction_form_error("Wallet non valido.");
                return Ok(());
            }
        };

        let flow_ids = self.ordered_flow_ids();
        if flow_ids.is_empty() {
            self.set_transaction_form_error("Nessun flow disponibile.");
            return Ok(());
        }
        let flow_id = match flow_ids.get(flow_index) {
            Some(id) => *id,
            None => {
                self.set_transaction_form_error("Flow non valido.");
                return Ok(());
            }
        };

        let occurred_at = if occurred_raw.is_empty() {
            None
        } else {
            match self.parse_local_datetime(occurred_raw.as_str()) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.set_transaction_form_error(&message);
                    return Ok(());
                }
            }
        };
        let occurred_at_new = occurred_at.unwrap_or_else(|| self.now_in_timezone());

        let category_clean = category_raw.trim_start_matches('#').trim();
        let category = if editing_id.is_some() {
            Some(category_clean.to_string())
        } else if category_clean.is_empty() {
            None
        } else {
            Some(category_clean.to_string())
        };
        let note = if editing_id.is_some() {
            Some(note_raw)
        } else if note_raw.is_empty() {
            None
        } else {
            Some(note_raw)
        };

        if let Some(transaction_id) = editing_id {
            let res = self
                .client
                .transaction_update(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
                    transaction_id,
                    TransactionUpdate {
                        vault_id: vault_id.to_string(),
                        amount_minor: Some(amount_minor),
                        wallet_id: Some(wallet_id),
                        flow_id: Some(flow_id),
                        from_wallet_id: None,
                        to_wallet_id: None,
                        from_flow_id: None,
                        to_flow_id: None,
                        category,
                        note,
                        occurred_at,
                    },
                )
                .await;

            match res {
                Ok(()) => {
                    self.state.last_flow_id = Some(flow_id);
                    self.state.transactions.form = TransactionFormState::default();
                    self.set_toast("Transazione aggiornata.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.form.error = Some(login_message_for_error(err));
                    self.set_toast("Errore aggiornamento.", ToastLevel::Error);
                }
            }
        } else {
            let res = match kind {
                TransactionKind::Income => {
                    self.client
                        .income_new(
                            self.state.login.username.as_str(),
                            self.state.login.password.as_str(),
                            IncomeNew {
                                vault_id: vault_id.to_string(),
                                amount_minor,
                                flow_id: Some(flow_id),
                                wallet_id: Some(wallet_id),
                                category,
                                note,
                                idempotency_key: None,
                                occurred_at: occurred_at_new,
                            },
                        )
                        .await
                }
                TransactionKind::Expense => {
                    self.client
                        .expense_new(
                            self.state.login.username.as_str(),
                            self.state.login.password.as_str(),
                            ExpenseNew {
                                vault_id: vault_id.to_string(),
                                amount_minor,
                                flow_id: Some(flow_id),
                                wallet_id: Some(wallet_id),
                                category,
                                note,
                                idempotency_key: None,
                                occurred_at: occurred_at_new,
                            },
                        )
                        .await
                }
                TransactionKind::Refund => {
                    self.client
                        .refund_new(
                            self.state.login.username.as_str(),
                            self.state.login.password.as_str(),
                            Refund {
                                vault_id: vault_id.to_string(),
                                amount_minor,
                                flow_id: Some(flow_id),
                                wallet_id: Some(wallet_id),
                                category,
                                note,
                                idempotency_key: None,
                                occurred_at: occurred_at_new,
                            },
                        )
                        .await
                }
                TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                    self.set_transaction_form_error("Usa il form transfer dedicato.");
                    return Ok(());
                }
            };

            match res {
                Ok(created) => {
                    self.state.last_flow_id = Some(flow_id);
                    self.state.transactions.last_created_id = Some(created.id);
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.form = TransactionFormState::default();
                    self.set_toast("Transazione salvata.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.form.error = Some(login_message_for_error(err));
                    self.set_toast("Errore salvataggio.", ToastLevel::Error);
                }
            }
        }

        Ok(())
    }

    fn set_transaction_form_error(&mut self, message: &str) {
        self.state.transactions.form.error = Some(message.to_string());
    }

    fn active_wallets_len(&self) -> usize {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| snap.wallets.iter().filter(|w| !w.archived).count())
            .unwrap_or(0)
    }

    fn active_flows_len(&self) -> usize {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| snap.flows.iter().filter(|f| !f.archived).count())
            .unwrap_or(0)
    }

    fn active_wallet_ids(&self) -> Vec<uuid::Uuid> {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| {
                snap.wallets
                    .iter()
                    .filter(|wallet| !wallet.archived)
                    .map(|wallet| wallet.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn active_flow_ids(&self) -> Vec<uuid::Uuid> {
        self.state
            .snapshot
            .as_ref()
            .map(|snap| {
                snap.flows
                    .iter()
                    .filter(|flow| !flow.archived)
                    .map(|flow| flow.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn ordered_wallet_ids(&self) -> Vec<uuid::Uuid> {
        ordered_ids(
            self.active_wallet_ids(),
            &self.state.transactions.recent_wallet_ids,
        )
    }

    fn ordered_flow_ids(&self) -> Vec<uuid::Uuid> {
        ordered_ids(
            self.active_flow_ids(),
            &self.state.transactions.recent_flow_ids,
        )
    }

    async fn submit_transfer_wallet(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let editing_id = self.state.transactions.transfer.editing_id;
        let ids = self.active_wallet_ids();
        if ids.len() < 2 {
            self.state.transactions.transfer.error = Some("Servono almeno 2 wallet.".to_string());
            return Ok(());
        }
        let from_id = ids[self.state.transactions.transfer.from_index];
        let to_id = ids[self.state.transactions.transfer.to_index];
        if from_id == to_id {
            self.state.transactions.transfer.error = Some("Scegli due wallet diversi.".to_string());
            return Ok(());
        }

        let currency = self.current_currency();
        let amount =
            match Money::parse_major(self.state.transactions.transfer.amount.trim(), currency) {
                Ok(money) => money.minor().abs(),
                Err(_) => {
                    self.state.transactions.transfer.error =
                        Some("Importo non valido.".to_string());
                    return Ok(());
                }
            };
        if amount <= 0 {
            self.state.transactions.transfer.error = Some("Importo deve essere > 0.".to_string());
            return Ok(());
        }

        let note = self.state.transactions.transfer.note.trim();
        let occurred_raw = self.state.transactions.transfer.occurred_at.trim();
        let occurred_at = if occurred_raw.is_empty() {
            None
        } else {
            match self.parse_local_datetime(occurred_raw) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.transfer.error = Some(message);
                    return Ok(());
                }
            }
        };
        let occurred_at_new = occurred_at.unwrap_or_else(|| self.now_in_timezone());

        if let Some(transaction_id) = editing_id {
            let res = self
                .client
                .transaction_update(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
                    transaction_id,
                    TransactionUpdate {
                        vault_id,
                        amount_minor: Some(amount),
                        wallet_id: None,
                        flow_id: None,
                        from_wallet_id: Some(from_id),
                        to_wallet_id: Some(to_id),
                        from_flow_id: None,
                        to_flow_id: None,
                        category: None,
                        note: Some(note.to_string()),
                        occurred_at,
                    },
                )
                .await;

            match res {
                Ok(()) => {
                    self.state.transactions.transfer = TransferFormState::default();
                    self.set_toast("Transfer wallet aggiornato.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.transfer.error = Some(login_message_for_error(err));
                    self.set_toast("Errore transfer wallet.", ToastLevel::Error);
                }
            }
        } else {
            let res = self
                .client
                .transfer_wallet_new(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
                    TransferWalletNew {
                        vault_id,
                        amount_minor: amount,
                        from_wallet_id: from_id,
                        to_wallet_id: to_id,
                        note: if note.is_empty() {
                            None
                        } else {
                            Some(note.to_string())
                        },
                        idempotency_key: None,
                        occurred_at: occurred_at_new,
                    },
                )
                .await;

            match res {
                Ok(created) => {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.transfer = TransferFormState::default();
                    self.state.transactions.last_created_id = Some(created.id);
                    self.set_toast("Transfer wallet salvato.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.transfer.error = Some(login_message_for_error(err));
                    self.set_toast("Errore transfer wallet.", ToastLevel::Error);
                }
            }
        }

        Ok(())
    }

    async fn submit_transfer_flow(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let editing_id = self.state.transactions.transfer.editing_id;
        let ids = self.active_flow_ids();
        if ids.len() < 2 {
            self.state.transactions.transfer.error = Some("Servono almeno 2 flow.".to_string());
            return Ok(());
        }
        let from_id = ids[self.state.transactions.transfer.from_index];
        let to_id = ids[self.state.transactions.transfer.to_index];
        if from_id == to_id {
            self.state.transactions.transfer.error = Some("Scegli due flow diversi.".to_string());
            return Ok(());
        }

        let currency = self.current_currency();
        let amount =
            match Money::parse_major(self.state.transactions.transfer.amount.trim(), currency) {
                Ok(money) => money.minor().abs(),
                Err(_) => {
                    self.state.transactions.transfer.error =
                        Some("Importo non valido.".to_string());
                    return Ok(());
                }
            };
        if amount <= 0 {
            self.state.transactions.transfer.error = Some("Importo deve essere > 0.".to_string());
            return Ok(());
        }

        let note = self.state.transactions.transfer.note.trim();
        let occurred_raw = self.state.transactions.transfer.occurred_at.trim();
        let occurred_at = if occurred_raw.is_empty() {
            None
        } else {
            match self.parse_local_datetime(occurred_raw) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.transfer.error = Some(message);
                    return Ok(());
                }
            }
        };
        let occurred_at_new = occurred_at.unwrap_or_else(|| self.now_in_timezone());

        if let Some(transaction_id) = editing_id {
            let res = self
                .client
                .transaction_update(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
                    transaction_id,
                    TransactionUpdate {
                        vault_id,
                        amount_minor: Some(amount),
                        wallet_id: None,
                        flow_id: None,
                        from_wallet_id: None,
                        to_wallet_id: None,
                        from_flow_id: Some(from_id),
                        to_flow_id: Some(to_id),
                        category: None,
                        note: Some(note.to_string()),
                        occurred_at,
                    },
                )
                .await;

            match res {
                Ok(()) => {
                    self.state.transactions.transfer = TransferFormState::default();
                    self.set_toast("Transfer flow aggiornato.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                    self.open_transaction_detail_by_id(transaction_id).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.transfer.error = Some(login_message_for_error(err));
                    self.set_toast("Errore transfer flow.", ToastLevel::Error);
                }
            }
        } else {
            let res = self
                .client
                .transfer_flow_new(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
                    TransferFlowNew {
                        vault_id,
                        amount_minor: amount,
                        from_flow_id: from_id,
                        to_flow_id: to_id,
                        note: if note.is_empty() {
                            None
                        } else {
                            Some(note.to_string())
                        },
                        idempotency_key: None,
                        occurred_at: occurred_at_new,
                    },
                )
                .await;

            match res {
                Ok(created) => {
                    self.state.transactions.mode = TransactionsMode::List;
                    self.state.transactions.transfer = TransferFormState::default();
                    self.state.transactions.last_created_id = Some(created.id);
                    self.set_toast("Transfer flow salvato.", ToastLevel::Success);
                    self.load_transactions(true).await?;
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.transactions.transfer.error = Some(login_message_for_error(err));
                    self.set_toast("Errore transfer flow.", ToastLevel::Error);
                }
            }
        }

        Ok(())
    }

    fn open_filter(&mut self) {
        let from_input = self
            .state
            .transactions
            .filter_from
            .map(|dt| self.format_local_datetime(dt))
            .unwrap_or_default();
        let to_input = self
            .state
            .transactions
            .filter_to
            .map(|dt| self.format_local_datetime(dt))
            .unwrap_or_default();
        let kind_income = self.has_kind(api_types::transaction::TransactionKind::Income);
        let kind_expense = self.has_kind(api_types::transaction::TransactionKind::Expense);
        let kind_refund = self.has_kind(api_types::transaction::TransactionKind::Refund);
        let kind_transfer_wallet =
            self.has_kind(api_types::transaction::TransactionKind::TransferWallet);
        let kind_transfer_flow =
            self.has_kind(api_types::transaction::TransactionKind::TransferFlow);

        let filter = &mut self.state.transactions.filter;
        filter.error = None;
        filter.focus = FilterField::From;
        filter.from_input = from_input;
        filter.to_input = to_input;
        filter.kind_income = kind_income;
        filter.kind_expense = kind_expense;
        filter.kind_refund = kind_refund;
        filter.kind_transfer_wallet = kind_transfer_wallet;
        filter.kind_transfer_flow = kind_transfer_flow;

        self.state.transactions.mode = TransactionsMode::Filter;
    }

    async fn apply_filter(&mut self) -> Result<()> {
        let (from_input, to_input, kind_income, kind_expense, kind_refund, kind_tw, kind_tf) = {
            let filter = &self.state.transactions.filter;
            (
                filter.from_input.clone(),
                filter.to_input.clone(),
                filter.kind_income,
                filter.kind_expense,
                filter.kind_refund,
                filter.kind_transfer_wallet,
                filter.kind_transfer_flow,
            )
        };

        let from = if from_input.trim().is_empty() {
            None
        } else {
            match self.parse_local_datetime(&from_input) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.filter.error = Some(message);
                    return Ok(());
                }
            }
        };

        let to = if to_input.trim().is_empty() {
            None
        } else {
            match self.parse_local_datetime(&to_input) {
                Ok(dt) => Some(dt),
                Err(message) => {
                    self.state.transactions.filter.error = Some(message);
                    return Ok(());
                }
            }
        };

        let mut kinds = Vec::new();
        if kind_income {
            kinds.push(api_types::transaction::TransactionKind::Income);
        }
        if kind_expense {
            kinds.push(api_types::transaction::TransactionKind::Expense);
        }
        if kind_refund {
            kinds.push(api_types::transaction::TransactionKind::Refund);
        }
        if kind_tw {
            kinds.push(api_types::transaction::TransactionKind::TransferWallet);
        }
        if kind_tf {
            kinds.push(api_types::transaction::TransactionKind::TransferFlow);
        }

        self.state.transactions.filter_from = from;
        self.state.transactions.filter_to = to;
        self.state.transactions.filter_kinds = if kinds.is_empty() { None } else { Some(kinds) };

        self.state.transactions.filter.error = None;
        self.state.transactions.mode = TransactionsMode::List;
        self.load_transactions(true).await?;
        Ok(())
    }

    async fn clear_filters(&mut self) -> Result<()> {
        self.state.transactions.scope_wallet_id = None;
        self.state.transactions.scope_flow_id = None;
        self.state.transactions.filter_from = None;
        self.state.transactions.filter_to = None;
        self.state.transactions.filter_kinds = None;
        self.state.transactions.filter = TransactionsFilterState::default();
        self.load_transactions(true).await?;
        Ok(())
    }

    async fn undo_last_transaction(&mut self) -> Result<()> {
        let Some(id) = self.state.transactions.last_created_id else {
            self.set_toast("Nessuna transazione da annullare.", ToastLevel::Info);
            return Ok(());
        };
        self.void_transaction_by_id(id).await?;
        Ok(())
    }

    async fn void_transaction_by_id(&mut self, transaction_id: uuid::Uuid) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let res = self
            .client
            .transaction_void(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                transaction_id,
                TransactionVoid {
                    vault_id,
                    voided_at: None,
                },
            )
            .await;

        match res {
            Ok(()) => {
                self.state.transactions.last_created_id = None;
                self.set_toast("Transazione annullata.", ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.set_toast(&login_message_for_error(err), ToastLevel::Error);
            }
        }

        Ok(())
    }

    fn has_kind(&self, kind: api_types::transaction::TransactionKind) -> bool {
        self.state
            .transactions
            .filter_kinds
            .as_ref()
            .map(|kinds| kinds.contains(&kind))
            .unwrap_or(false)
    }

    fn parse_local_datetime(
        &self,
        input: &str,
    ) -> std::result::Result<DateTime<FixedOffset>, String> {
        let naive = chrono::NaiveDateTime::parse_from_str(input.trim(), "%Y-%m-%d %H:%M")
            .map_err(|_| "Formato data non valido. Usa YYYY-MM-DD HH:MM".to_string())?;
        let tz = Tz::from_str(self.config.timezone.as_str()).unwrap_or(Tz::UTC);
        let localized = tz.from_local_datetime(&naive);
        let dt = match localized {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(dt, _) => dt,
            chrono::LocalResult::None => {
                return Err("Data/ora non valida.".to_string());
            }
        };
        let offset = dt.offset().fix();
        Ok(dt.with_timezone(&offset))
    }

    fn start_wallet_create(&mut self) {
        self.reset_wallet_form();
        self.state.wallets.mode = WalletsMode::Create;
    }

    fn start_wallet_rename(&mut self) {
        let Some(name) = self.selected_wallet().map(|wallet| wallet.name.clone()) else {
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
                self.refresh_wallets_search().await?;
                self.refresh_flows_search().await?;
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                let message = login_message_for_error(err);
                self.state.wallets.error = Some(message.clone());
                self.state.flows.error = Some(message.clone());
                self.state.stats.error = Some(message);
                self.connection_error("Errore connessione");
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
        let indices = wallets_visible_indices(&self.state);
        let index = indices.get(self.state.wallets.selected).copied()?;
        self.state
            .snapshot
            .as_ref()
            .and_then(|snap| snap.wallets.get(index))
    }

    fn selected_flow(&self) -> Option<&api_types::vault::FlowView> {
        let indices = flows_visible_indices(&self.state);
        let index = indices.get(self.state.flows.selected).copied()?;
        self.state
            .snapshot
            .as_ref()
            .and_then(|snap| snap.flows.get(index))
    }

    fn select_transaction_by_id(&mut self, transaction_id: uuid::Uuid) -> bool {
        let indices = transactions_visible_indices(&self.state);
        for (visible_idx, idx) in indices.iter().enumerate() {
            if self
                .state
                .transactions
                .items
                .get(*idx)
                .map(|tx| tx.id == transaction_id)
                .unwrap_or(false)
            {
                self.state.transactions.selected = visible_idx;
                return true;
            }
        }
        false
    }

    fn select_wallet_by_id(&mut self, wallet_id: uuid::Uuid) {
        let Some(snapshot) = &self.state.snapshot else {
            return;
        };
        let indices = wallets_visible_indices(&self.state);
        if let Some(pos) = indices.iter().position(|idx| {
            snapshot
                .wallets
                .get(*idx)
                .map(|wallet| wallet.id == wallet_id)
                .unwrap_or(false)
        }) {
            self.state.wallets.selected = pos;
        }
    }

    fn select_flow_by_id(&mut self, flow_id: uuid::Uuid) {
        let Some(snapshot) = &self.state.snapshot else {
            return;
        };
        let indices = flows_visible_indices(&self.state);
        if let Some(pos) = indices.iter().position(|idx| {
            snapshot
                .flows
                .get(*idx)
                .map(|flow| flow.id == flow_id)
                .unwrap_or(false)
        }) {
            self.state.flows.selected = pos;
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
            flow_id: self.state.transactions.scope_flow_id,
            wallet_id: self.state.transactions.scope_wallet_id,
            limit: Some(20),
            cursor: self.state.transactions.cursor.clone(),
            from: self.state.transactions.filter_from,
            to: self.state.transactions.filter_to,
            kinds: self.state.transactions.filter_kinds.clone(),
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
                self.update_recent_categories_from_items();
                if reset {
                    self.refresh_recent_targets().await?;
                }
                self.connection_ok(None);
                self.refresh_transactions_search().await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err));
                self.connection_error("Errore connessione");
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
        let indices = transactions_visible_indices(&self.state);
        let Some(item_index) = indices.get(self.state.transactions.selected).copied() else {
            return Ok(());
        };
        let Some(selected) = self.state.transactions.items.get(item_index) else {
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
                self.state.transactions.form = TransactionFormState::default();
                self.state.transactions.transfer = TransferFormState::default();
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err));
                self.connection_error("Errore connessione");
            }
        }

        Ok(())
    }

    async fn open_transaction_detail_by_id(&mut self, transaction_id: uuid::Uuid) -> Result<()> {
        if self.select_transaction_by_id(transaction_id) {
            return self.open_transaction_detail().await;
        }

        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.id.as_deref())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))?;
        let res = self
            .client
            .transaction_detail(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                TransactionGet {
                    vault_id: vault_id.to_string(),
                    id: transaction_id,
                },
            )
            .await;

        match res {
            Ok(detail) => {
                self.state.transactions.detail = Some(detail);
                self.state.transactions.mode = TransactionsMode::Detail;
                self.state.transactions.form = TransactionFormState::default();
                self.state.transactions.transfer = TransferFormState::default();
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err));
                self.connection_error("Errore connessione");
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
                self.set_toast("Transazione annullata.", ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err));
                self.set_toast("Errore durante l'annullamento.", ToastLevel::Error);
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
            Ok(created) => {
                if let Some(flow_id) = last_flow_id {
                    self.state.last_flow_id = Some(flow_id);
                }
                self.state.transactions.last_created_id = Some(created.id);
                self.set_toast("Transazione ripetuta.", ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.error = Some(login_message_for_error(err));
                self.set_toast("Errore durante la ripetizione.", ToastLevel::Error);
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

        let (wallet_id, flow_id, _wallet_name, _flow_name) = match default_wallet_flow(&self.state)
        {
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

        let parsed = match crate::quick_add::parse(&self.state.transactions.quick_input, currency) {
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
            Ok(created) => {
                self.state.last_flow_id = Some(flow_id);
                self.state.transactions.last_created_id = Some(created.id);
                self.state.transactions.quick_input.clear();
                self.state.transactions.quick_error = None;
                self.set_toast("Transazione salvata.", ToastLevel::Success);
                self.load_transactions(true).await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.transactions.quick_error = Some(login_message_for_error(err));
                self.set_toast("Errore durante il salvataggio.", ToastLevel::Error);
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
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.wallets.detail.error = Some(login_message_for_error(err));
                self.connection_error("Errore connessione");
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
        let opening_raw = if opening_raw.is_empty() {
            "0"
        } else {
            opening_raw
        };
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
                self.set_toast("Wallet creato.", ToastLevel::Success);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.wallets.form.error = Some(login_message_for_error(err));
                self.set_toast("Errore creazione wallet.", ToastLevel::Error);
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
                self.set_toast("Wallet aggiornato.", ToastLevel::Success);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.wallets.form.error = Some(login_message_for_error(err));
                self.set_toast("Errore aggiornamento wallet.", ToastLevel::Error);
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
                self.set_toast("Wallet aggiornato.", ToastLevel::Success);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.wallets.error = Some(login_message_for_error(err));
                self.set_toast("Errore archivio wallet.", ToastLevel::Error);
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
        self.load_flow_detail(flow_id).await?;
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
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.flows.detail.error = Some(login_message_for_error(err));
                self.connection_error("Errore connessione");
            }
        }

        Ok(())
    }

    async fn load_flow_detail(&mut self, flow_id: uuid::Uuid) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let res = self
            .client
            .cash_flow_get(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                api_types::cash_flow::CashFlowGet {
                    vault_id,
                    id: Some(flow_id),
                    name: None,
                },
            )
            .await;

        match res {
            Ok(flow) => {
                self.state.flows.detail.detail = Some(flow);
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.flows.detail.error = Some(login_message_for_error(err));
                self.connection_error("Errore connessione");
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
        let opening_raw = if opening_raw.is_empty() {
            "0"
        } else {
            opening_raw
        };
        let opening = match Money::parse_major(opening_raw, currency) {
            Ok(money) => money.minor(),
            Err(_) => {
                self.state.flows.form.error = Some("Saldo iniziale non valido.".to_string());
                return Ok(());
            }
        };
        if opening < 0 {
            self.state.flows.form.error = Some("Saldo iniziale deve essere >= 0.".to_string());
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
                self.set_toast("Flow creato.", ToastLevel::Success);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.flows.form.error = Some(login_message_for_error(err));
                self.set_toast("Errore creazione flow.", ToastLevel::Error);
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
                self.set_toast("Flow aggiornato.", ToastLevel::Success);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.flows.form.error = Some(login_message_for_error(err));
                self.set_toast("Errore aggiornamento flow.", ToastLevel::Error);
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
                self.set_toast("Flow aggiornato.", ToastLevel::Success);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.flows.error = Some(login_message_for_error(err));
                self.set_toast("Errore archivio flow.", ToastLevel::Error);
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
                self.set_toast("Vault creato.", ToastLevel::Success);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.vault_ui.form.error = Some(login_message_for_error(err));
                self.set_toast("Errore creazione vault.", ToastLevel::Error);
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
                self.connection_ok(None);
                self.load_stats_series().await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.stats.error = Some(login_message_for_error(err));
                self.connection_error("Errore connessione");
            }
        }

        Ok(())
    }

    async fn load_stats_series(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let to = self.now_in_timezone();
        let from = to - chrono::Duration::days(180);

        let mut cursor = None;
        let mut transactions = Vec::new();
        loop {
            let payload = TransactionList {
                vault_id: vault_id.clone(),
                flow_id: None,
                wallet_id: None,
                limit: Some(200),
                cursor,
                from: Some(from),
                to: Some(to),
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
                    transactions.extend(list.transactions);
                    if let Some(next) = list.next_cursor {
                        cursor = Some(next);
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.stats.error = Some(login_message_for_error(err));
                    return Ok(());
                }
            }
        }

        self.compute_stats_series(&transactions, to);
        Ok(())
    }

    fn compute_stats_series(
        &mut self,
        transactions: &[TransactionView],
        to: DateTime<FixedOffset>,
    ) {
        use api_types::transaction::TransactionKind;
        use std::collections::HashMap;

        let tz = Tz::from_str(self.config.timezone.as_str()).unwrap_or(Tz::UTC);
        let start_day = (to - chrono::Duration::days(29))
            .with_timezone(&tz)
            .date_naive();
        let end_day = to.with_timezone(&tz).date_naive();
        let days_count = (end_day - start_day).num_days().max(0) as usize + 1;
        let mut daily_net = vec![0i64; days_count];

        let mut category_breakdown: HashMap<String, i64> = HashMap::new();
        let mut monthly_income: HashMap<(i32, u32), i64> = HashMap::new();
        let mut monthly_expense: HashMap<(i32, u32), (i64, i64)> = HashMap::new(); // (expense, refund)

        let (current_year, current_month) = self.state.stats.current_month;

        for tx in transactions {
            if tx.voided {
                continue;
            }

            let local = tx.occurred_at.with_timezone(&tz);
            let date = local.date_naive();
            let year = date.year();
            let month = date.month();

            match tx.kind {
                TransactionKind::Income => {
                    if date >= start_day && date <= end_day {
                        let idx = (date - start_day).num_days() as usize;
                        daily_net[idx] += tx.amount_minor.abs();
                    }
                    *monthly_income.entry((year, month)).or_insert(0) += tx.amount_minor.abs();
                }
                TransactionKind::Expense => {
                    if date >= start_day && date <= end_day {
                        let idx = (date - start_day).num_days() as usize;
                        daily_net[idx] -= tx.amount_minor.abs();
                    }
                    let entry = monthly_expense.entry((year, month)).or_insert((0, 0));
                    entry.0 += tx.amount_minor.abs();

                    if year == current_year && month == current_month {
                        let category = tx.category.clone().unwrap_or_else(|| "Other".to_string());
                        *category_breakdown.entry(category).or_insert(0) += tx.amount_minor.abs();
                    }
                }
                TransactionKind::Refund => {
                    if date >= start_day && date <= end_day {
                        let idx = (date - start_day).num_days() as usize;
                        daily_net[idx] += tx.amount_minor.abs();
                    }
                    let entry = monthly_expense.entry((year, month)).or_insert((0, 0));
                    entry.1 += tx.amount_minor.abs();
                }
                TransactionKind::TransferWallet | TransactionKind::TransferFlow => {}
            }
        }

        let mut cumulative = Vec::with_capacity(daily_net.len());
        let mut running = 0i64;
        for delta in daily_net {
            running += delta;
            cumulative.push(running);
        }

        let min = cumulative.iter().copied().min().unwrap_or(0);
        let max = cumulative.iter().copied().max().unwrap_or(0);
        let shift = if min < 0 { -min } else { 0 };
        let sparkline = cumulative
            .iter()
            .map(|value| (value + shift) as u64)
            .collect::<Vec<_>>();

        let mut breakdown = category_breakdown.into_iter().collect::<Vec<_>>();
        breakdown.sort_by(|a, b| b.1.cmp(&a.1));

        let months = Self::build_last_months(to, 6);
        let mut monthly_expenses_vec = Vec::new();
        let mut monthly_income_vec = Vec::new();
        for (year, month, label) in months {
            let income = monthly_income.get(&(year, month)).copied().unwrap_or(0);
            let (expense, refund) = monthly_expense
                .get(&(year, month))
                .copied()
                .unwrap_or((0, 0));
            let net_expense = (expense - refund).max(0);
            monthly_income_vec.push((label.clone(), income));
            monthly_expenses_vec.push((label, net_expense));
        }

        self.state.stats.category_breakdown = breakdown;
        self.state.stats.monthly_trend = monthly_expenses_vec;
        self.state.stats.monthly_income = monthly_income_vec;
        self.state.stats.sparkline = sparkline;
        self.state.stats.sparkline_min = min;
        self.state.stats.sparkline_max = max;
    }

    fn build_last_months(to: DateTime<FixedOffset>, count: usize) -> Vec<(i32, u32, String)> {
        let mut months = Vec::new();
        let mut year = to.year();
        let mut month = to.month();
        for _ in 0..count {
            months.push((year, month, month_label(month)));
            if month == 1 {
                month = 12;
                year -= 1;
            } else {
                month -= 1;
            }
        }
        months.reverse();
        months
    }

    fn format_local_datetime(&self, dt: DateTime<FixedOffset>) -> String {
        let tz = Tz::from_str(self.config.timezone.as_str()).unwrap_or(Tz::UTC);
        dt.with_timezone(&tz).format("%Y-%m-%d %H:%M").to_string()
    }

    /// Navigate to next month in stats view
    fn stats_next_month(&mut self) {
        let (year, month) = self.state.stats.current_month;
        if month == 12 {
            self.state.stats.current_month = (year + 1, 1);
        } else {
            self.state.stats.current_month = (year, month + 1);
        }
    }

    /// Navigate to previous month in stats view
    fn stats_prev_month(&mut self) {
        let (year, month) = self.state.stats.current_month;
        if month == 1 {
            self.state.stats.current_month = (year - 1, 12);
        } else {
            self.state.stats.current_month = (year, month - 1);
        }
    }

    fn current_currency(&self) -> engine::Currency {
        self.state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur)
    }

    fn open_palette(&mut self) {
        self.state.palette.active = true;
        self.state.palette.query.clear();
        self.state.palette.selected = 0;
    }

    async fn handle_palette_action(&mut self, action: crate::ui::keymap::AppAction) -> Result<()> {
        match action {
            crate::ui::keymap::AppAction::Cancel => {
                self.state.palette.active = false;
            }
            crate::ui::keymap::AppAction::Backspace => {
                self.state.palette.query.pop();
                self.state.palette.selected = 0;
            }
            crate::ui::keymap::AppAction::Up => {
                if self.state.palette.selected > 0 {
                    self.state.palette.selected -= 1;
                }
            }
            crate::ui::keymap::AppAction::Down => {
                let max = self.filtered_commands().len();
                if max > 0 {
                    self.state.palette.selected = (self.state.palette.selected + 1).min(max - 1);
                }
            }
            crate::ui::keymap::AppAction::Input(ch) => {
                self.state.palette.query.push(ch);
                self.state.palette.selected = 0;
            }
            crate::ui::keymap::AppAction::Submit => {
                if let Some(command) = self.filtered_commands().get(self.state.palette.selected) {
                    self.execute_command(*command).await?;
                    self.state.palette.active = false;
                }
            }
            crate::ui::keymap::AppAction::TogglePalette => {
                self.state.palette.active = false;
            }
            _ => {}
        }

        Ok(())
    }

    fn filtered_commands(&self) -> Vec<PaletteCommand> {
        filter_commands(self.state.palette.query.as_str())
    }

    async fn execute_command(&mut self, command: PaletteCommand) -> Result<()> {
        match command {
            PaletteCommand::NewExpense => {
                self.start_transaction_form(TransactionKind::Expense)
                    .await?;
            }
            PaletteCommand::NewIncome => {
                self.start_transaction_form(TransactionKind::Income).await?;
            }
            PaletteCommand::NewRefund => {
                self.start_transaction_form(TransactionKind::Refund).await?;
            }
            PaletteCommand::NewTransferWallet => {
                self.state.section = Section::Transactions;
                self.start_transfer_wallet();
            }
            PaletteCommand::NewTransferFlow => {
                self.state.section = Section::Transactions;
                self.start_transfer_flow();
            }
            PaletteCommand::WalletNew => {
                self.state.section = Section::Wallets;
                self.start_wallet_create();
            }
            PaletteCommand::FlowNew => {
                self.state.section = Section::Flows;
                self.start_flow_create();
            }
            PaletteCommand::VaultCreate => {
                self.state.section = Section::Vault;
                self.start_vault_create();
            }
            PaletteCommand::Refresh => {
                self.refresh_snapshot().await?;
                if self.state.section == Section::Transactions {
                    self.load_transactions(true).await?;
                } else if self.state.section == Section::Stats {
                    self.load_stats().await?;
                }
            }
            PaletteCommand::ToggleVoided => {
                if self.state.section != Section::Transactions {
                    self.state.section = Section::Transactions;
                }
                self.state.transactions.include_voided = !self.state.transactions.include_voided;
                self.load_transactions(true).await?;
            }
        }

        Ok(())
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
    pub scope_wallet_id: Option<uuid::Uuid>,
    pub scope_flow_id: Option<uuid::Uuid>,
    pub picker_index: usize,
    pub include_voided: bool,
    pub include_transfers: bool,
    pub error: Option<String>,
    pub mode: TransactionsMode,
    pub detail: Option<TransactionDetailResponse>,
    pub quick_input: String,
    pub quick_error: Option<String>,
    pub quick_active: bool,
    pub transfer: TransferFormState,
    pub form: TransactionFormState,
    pub filter_from: Option<DateTime<FixedOffset>>,
    pub filter_to: Option<DateTime<FixedOffset>>,
    pub filter_kinds: Option<Vec<api_types::transaction::TransactionKind>>,
    pub filter: TransactionsFilterState,
    pub last_created_id: Option<uuid::Uuid>,
    pub recent_categories: Vec<String>,
    pub recent_wallet_ids: Vec<uuid::Uuid>,
    pub recent_flow_ids: Vec<uuid::Uuid>,
    pub search_query: String,
    pub search_active: bool,
}

impl Default for TransactionsState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            cursor: None,
            next_cursor: None,
            prev_cursors: Vec::new(),
            selected: 0,
            scope_wallet_id: None,
            scope_flow_id: None,
            picker_index: 0,
            include_voided: false,
            include_transfers: false,
            error: None,
            mode: TransactionsMode::List,
            detail: None,
            quick_input: String::new(),
            quick_error: None,
            quick_active: false,
            transfer: TransferFormState::default(),
            form: TransactionFormState::default(),
            filter_from: None,
            filter_to: None,
            filter_kinds: None,
            filter: TransactionsFilterState::default(),
            last_created_id: None,
            recent_categories: Vec::new(),
            recent_wallet_ids: Vec::new(),
            recent_flow_ids: Vec::new(),
            search_query: String::new(),
            search_active: false,
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
        self.quick_input.clear();
        self.quick_error = None;
        self.quick_active = false;
        self.transfer = TransferFormState::default();
        self.form = TransactionFormState::default();
        self.filter = TransactionsFilterState::default();
        self.last_created_id = None;
        self.recent_categories.clear();
        self.recent_wallet_ids.clear();
        self.recent_flow_ids.clear();
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
    Form,
    PickWallet,
    PickFlow,
    TransferWallet,
    TransferFlow,
    Filter,
}

#[derive(Debug)]
pub struct TransferFormState {
    pub from_index: usize,
    pub to_index: usize,
    pub amount: String,
    pub note: String,
    pub occurred_at: String,
    pub focus: TransferField,
    pub error: Option<String>,
    pub editing_id: Option<uuid::Uuid>,
}

impl Default for TransferFormState {
    fn default() -> Self {
        Self {
            from_index: 0,
            to_index: 1,
            amount: String::new(),
            note: String::new(),
            occurred_at: String::new(),
            focus: TransferField::From,
            error: None,
            editing_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferField {
    From,
    To,
    Amount,
    Note,
    OccurredAt,
}

#[derive(Debug)]
pub struct TransactionFormState {
    pub kind: TransactionKind,
    pub amount: String,
    pub wallet_index: usize,
    pub flow_index: usize,
    pub category: String,
    pub note: String,
    pub occurred_at: String,
    pub focus: TransactionFormField,
    pub error: Option<String>,
    pub category_index: Option<usize>,
    pub editing_id: Option<uuid::Uuid>,
}

impl Default for TransactionFormState {
    fn default() -> Self {
        Self {
            kind: TransactionKind::Expense,
            amount: String::new(),
            wallet_index: 0,
            flow_index: 0,
            category: String::new(),
            note: String::new(),
            occurred_at: String::new(),
            focus: TransactionFormField::Amount,
            error: None,
            category_index: None,
            editing_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionFormField {
    Amount,
    Wallet,
    Flow,
    Category,
    Note,
    OccurredAt,
}

#[derive(Debug)]
pub struct TransactionsFilterState {
    pub from_input: String,
    pub to_input: String,
    pub focus: FilterField,
    pub error: Option<String>,
    pub kind_income: bool,
    pub kind_expense: bool,
    pub kind_refund: bool,
    pub kind_transfer_wallet: bool,
    pub kind_transfer_flow: bool,
}

impl Default for TransactionsFilterState {
    fn default() -> Self {
        Self {
            from_input: String::new(),
            to_input: String::new(),
            focus: FilterField::From,
            error: None,
            kind_income: false,
            kind_expense: false,
            kind_refund: false,
            kind_transfer_wallet: false,
            kind_transfer_flow: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    From,
    To,
    Kinds,
}

#[derive(Debug, Default)]
pub struct HelpState {
    pub active: bool,
}

#[derive(Debug)]
pub struct ToastState {
    pub message: String,
    pub level: ToastLevel,
    pub expires_at: std::time::Instant,
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

#[derive(Debug)]
pub struct CommandPaletteState {
    pub active: bool,
    pub query: String,
    pub selected: usize,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            active: false,
            query: String::new(),
            selected: 0,
        }
    }
}

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

#[derive(Debug)]
pub struct FlowDetailState {
    pub flow_id: Option<uuid::Uuid>,
    pub transactions: Vec<TransactionView>,
    pub detail: Option<engine::CashFlow>,
    pub error: Option<String>,
}

impl Default for FlowDetailState {
    fn default() -> Self {
        Self {
            flow_id: None,
            transactions: Vec::new(),
            detail: None,
            error: None,
        }
    }
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

#[derive(Debug)]
pub struct StatsState {
    pub data: Option<Statistic>,
    pub error: Option<String>,
    /// Current month being viewed (year, month 1-12)
    pub current_month: (i32, u32),
    /// Category breakdown computed from transactions
    pub category_breakdown: Vec<(String, i64)>,
    /// Monthly trend data (last 6 months of expenses)
    pub monthly_trend: Vec<(String, i64)>,
    /// Monthly trend data (last 6 months of income)
    pub monthly_income: Vec<(String, i64)>,
    /// Sparkline data for last 30 days (shifted to >= 0)
    pub sparkline: Vec<u64>,
    pub sparkline_min: i64,
    pub sparkline_max: i64,
}

impl Default for StatsState {
    fn default() -> Self {
        let now = chrono::Local::now();
        Self {
            data: None,
            error: None,
            current_month: (now.year(), now.month()),
            category_breakdown: Vec::new(),
            monthly_trend: Vec::new(),
            monthly_income: Vec::new(),
            sparkline: Vec::new(),
            sparkline_min: 0,
            sparkline_max: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    NewExpense,
    NewIncome,
    NewRefund,
    NewTransferWallet,
    NewTransferFlow,
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
            Self::WalletNew => "Wallets: New",
            Self::FlowNew => "Flows: New",
            Self::VaultCreate => "Vault: Create",
            Self::Refresh => "Refresh",
            Self::ToggleVoided => "Transactions: Toggle voided",
        }
    }
}

pub(crate) fn filter_commands(query: &str) -> Vec<PaletteCommand> {
    let query = query.trim().to_lowercase();
    let all = PaletteCommand::all();
    if query.is_empty() {
        return all;
    }

    let mut scored = all
        .into_iter()
        .filter_map(|cmd| {
            let label = cmd.label().to_lowercase();
            fuzzy_score(&label, &query).map(|score| (score, cmd))
        })
        .collect::<Vec<_>>();

    scored.sort_by_key(|(score, _)| *score);
    scored.into_iter().map(|(_, cmd)| cmd).collect()
}

fn fuzzy_score(label: &str, query: &str) -> Option<usize> {
    let mut score = 0usize;
    let mut pos = 0usize;
    for ch in query.chars() {
        if let Some(idx) = label[pos..].find(ch) {
            score += idx;
            pos += idx + 1;
        } else {
            return None;
        }
    }
    Some(score)
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

fn month_label(month: u32) -> String {
    let label = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    };
    label.to_string()
}

fn default_wallet_flow(
    state: &AppState,
) -> std::result::Result<(uuid::Uuid, uuid::Uuid, String, String), String> {
    let snapshot = state
        .snapshot
        .as_ref()
        .ok_or_else(|| "Snapshot non disponibile.".to_string())?;

    let wallet = state
        .transactions
        .scope_wallet_id
        .and_then(|wallet_id| {
            snapshot
                .wallets
                .iter()
                .find(|wallet| wallet.id == wallet_id && !wallet.archived)
        })
        .or_else(|| {
            state
                .transactions
                .recent_wallet_ids
                .iter()
                .find_map(|recent_id| {
                    snapshot
                        .wallets
                        .iter()
                        .find(|wallet| wallet.id == *recent_id && !wallet.archived)
                })
        })
        .or_else(|| snapshot.wallets.iter().find(|wallet| !wallet.archived))
        .ok_or_else(|| "Nessun wallet disponibile.".to_string())?;
    let flow = state
        .transactions
        .scope_flow_id
        .and_then(|flow_id| {
            snapshot
                .flows
                .iter()
                .find(|flow| flow.id == flow_id && !flow.archived)
        })
        .or_else(|| {
            state
                .transactions
                .recent_flow_ids
                .iter()
                .find_map(|recent_id| {
                    snapshot
                        .flows
                        .iter()
                        .find(|flow| flow.id == *recent_id && !flow.archived)
                })
        })
        .or_else(|| {
            state.last_flow_id.and_then(|last_id| {
                snapshot
                    .flows
                    .iter()
                    .find(|flow| flow.id == last_id && !flow.archived)
            })
        })
        .or_else(|| snapshot.flows.iter().find(|flow| flow.is_unallocated))
        .ok_or_else(|| "Flow Unallocated mancante.".to_string())?;

    Ok((wallet.id, flow.id, wallet.name.clone(), flow.name.clone()))
}

pub(crate) fn transactions_visible_indices(state: &AppState) -> Vec<usize> {
    let query = normalize_query(state.transactions.search_query.as_str());
    if query.is_empty() {
        return (0..state.transactions.items.len()).collect();
    }

    state
        .transactions
        .items
        .iter()
        .enumerate()
        .filter_map(|(idx, tx)| {
            if transaction_matches_query(tx, query.as_str()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn wallets_visible_indices(state: &AppState) -> Vec<usize> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let query = normalize_query(state.wallets.search_query.as_str());
    if query.is_empty() {
        return (0..snapshot.wallets.len()).collect();
    }

    snapshot
        .wallets
        .iter()
        .enumerate()
        .filter_map(|(idx, wallet)| {
            if wallet.name.to_lowercase().contains(query.as_str()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn flows_visible_indices(state: &AppState) -> Vec<usize> {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return Vec::new();
    };
    let query = normalize_query(state.flows.search_query.as_str());
    if query.is_empty() {
        return (0..snapshot.flows.len()).collect();
    }

    snapshot
        .flows
        .iter()
        .enumerate()
        .filter_map(|(idx, flow)| {
            if flow.name.to_lowercase().contains(query.as_str()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn ordered_wallet_ids_from_state(state: &AppState) -> Vec<uuid::Uuid> {
    let active_ids = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|wallet| !wallet.archived)
                .map(|wallet| wallet.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ordered_ids(active_ids, &state.transactions.recent_wallet_ids)
}

pub(crate) fn ordered_flow_ids_from_state(state: &AppState) -> Vec<uuid::Uuid> {
    let active_ids = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.flows
                .iter()
                .filter(|flow| !flow.archived)
                .map(|flow| flow.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ordered_ids(active_ids, &state.transactions.recent_flow_ids)
}

fn transaction_matches_query(tx: &TransactionView, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let kind = transaction_kind_label(tx.kind);
    if kind.contains(query) {
        return true;
    }
    if tx
        .note
        .as_ref()
        .map(|note| note.to_lowercase().contains(query))
        .unwrap_or(false)
    {
        return true;
    }
    if tx
        .category
        .as_ref()
        .map(|category| category.to_lowercase().contains(query))
        .unwrap_or(false)
    {
        return true;
    }
    let amount = tx.amount_minor.abs().to_string();
    if amount.contains(query) {
        return true;
    }
    let when = tx.occurred_at.format("%Y-%m-%d %H:%M").to_string();
    when.contains(query)
}

fn transaction_kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Refund => "refund",
        TransactionKind::TransferWallet => "transfer wallet",
        TransactionKind::TransferFlow => "transfer flow",
    }
}

fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

fn format_amount_input(amount_minor: i64, currency: engine::Currency) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let abs = amount_minor.unsigned_abs();
    let scale = 10u64.pow(currency.minor_units() as u32);
    if scale == 1 {
        return format!("{sign}{abs}");
    }
    let major = abs / scale;
    let minor = abs % scale;
    format!(
        "{sign}{major}.{minor:0width$}",
        width = currency.minor_units() as usize
    )
}

fn ordered_ids(active: Vec<uuid::Uuid>, recents: &[uuid::Uuid]) -> Vec<uuid::Uuid> {
    let mut ordered = Vec::with_capacity(active.len());
    for recent in recents {
        if active.contains(recent) && !ordered.contains(recent) {
            ordered.push(*recent);
        }
    }
    for id in active {
        if !ordered.contains(&id) {
            ordered.push(id);
        }
    }
    ordered
}

fn push_recent_id(target: &mut Vec<uuid::Uuid>, value: uuid::Uuid, limit: usize) {
    if target.contains(&value) {
        return;
    }
    if target.len() >= limit {
        return;
    }
    target.push(value);
}
