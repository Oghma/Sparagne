use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};

use crate::{
    client::{Client, ClientError},
    config::AppConfig,
    error::{AppError, Result},
    ui,
};

use api_types::{
    transaction::{
        ExpenseNew, IncomeNew, Refund, TransactionDetailResponse, TransactionGet, TransactionList,
        TransactionListResponse, TransactionUpdate, TransactionView, TransactionVoid,
        TransferFlowNew, TransferWalletNew,
    },
    vault::{Vault, VaultSnapshot},
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
    pub base_url: String,
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
            base_url: config.base_url.clone(),
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
                            self.state.section = Section::Home;
                        }
                    }
                } else {
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
                }
            }
            crate::ui::keymap::AppAction::Up => {
                if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.select_prev();
                }
            }
            crate::ui::keymap::AppAction::Down => {
                if self.state.screen == Screen::Home
                    && self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.select_next();
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
        self.state.login.focus = match self.state.login.focus {
            LoginField::Username => LoginField::Password,
            LoginField::Password => LoginField::Username,
        };
    }

    fn active_field_mut(&mut self) -> &mut String {
        match self.state.login.focus {
            LoginField::Username => &mut self.state.login.username,
            LoginField::Password => &mut self.state.login.password,
        }
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
            TransactionsMode::List => self.open_transaction_detail().await,
            TransactionsMode::Detail => Ok(()),
            TransactionsMode::Edit => self.apply_transaction_edit().await,
        }
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
                return Ok(());
            }
            'f' | 'F' => {
                self.state.section = Section::Flows;
                self.state.transactions.mode = TransactionsMode::List;
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
                return Ok(());
            }
            'r' | 'R' => {
                if self.state.section == Section::Transactions {
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.repeat_transaction().await?;
                    } else if self.state.transactions.mode == TransactionsMode::List {
                        self.load_transactions(true).await?;
                    }
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
            'e' | 'E' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Detail
                {
                    self.state.transactions.mode = TransactionsMode::Edit;
                    self.state.transactions.edit_input.clear();
                    self.state.transactions.edit_error = None;
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
                }
                return Ok(());
            }
            _ => {}
        }
        Ok(())
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

        let res = match detail.transaction.kind {
            api_types::transaction::TransactionKind::Income => {
                let (wallet_id, flow_id) = extract_wallet_flow(detail);
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
