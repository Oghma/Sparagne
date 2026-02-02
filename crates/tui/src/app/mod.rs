mod actions;
mod handlers;
mod helpers;
mod state;

pub(crate) use helpers::{
    FlowAlertItem, FlowAlertSeverity, HomeFeedItem, filter_commands, flow_name_suggestions,
    flows_visible_indices, home_feed_items, ordered_flow_ids_from_state,
    ordered_wallet_ids_from_state, resolve_category_matches, resolve_flow_matches,
    resolve_flow_name, resolve_wallet_matches, resolve_wallet_name, transactions_visible_indices,
    wallets_visible_indices,
};
pub use state::*;

use std::time::Duration;

use crossterm::event::{self, Event};

use crate::{
    client::Client,
    config::AppConfig,
    error::{AppError, Result},
    local_state::{LocalState, default_state_path},
    ui,
};

pub struct App {
    config: AppConfig,
    client: Client,
    pub state: AppState,
    should_quit: bool,
    local_state: LocalState,
    local_state_path: String,
}

impl App {
    pub fn new(config: AppConfig) -> Result<Self> {
        let client = Client::new(&config.base_url)?;
        let local_state_path = default_state_path().to_string();
        let local_state = LocalState::load(local_state_path.as_str())?;
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
            accounts_tab: AccountsTab::Sources,
            home_feed_selected: 0,
            home_low_balance_minor: config.low_balance_minor,
            undo_toast_secs: config.undo_toast_secs.max(1),
            emoji_mode: config.emoji_mode,
            density: config.density,
            transactions: TransactionsState::default(),
            wallets: WalletsState::default(),
            flows: FlowsState::default(),
            vault_ui: VaultState::default(),
            categories: CategoriesState::default(),
            members: MembersState::default(),
            stats: StatsState::default(),
            palette: CommandPaletteState::default(),
            global_search: GlobalSearchState::default(),
            help: HelpState::default(),
            toast: None,
            overlays: OverlayState::default(),
            connection: ConnectionState::default(),
            spinner: SpinnerState::default(),
            last_refresh: None,
            last_flow_id: None,
            default_wallet_id: None,
            default_flow_id: None,
        };

        Ok(Self {
            config,
            client,
            state,
            should_quit: false,
            local_state,
            local_state_path,
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
            self.tick_spinner();
            self.expire_toast().await?;
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

    fn tick_spinner(&mut self) {
        self.state.spinner.tick();
    }

    #[allow(dead_code)]
    pub fn client(&self) -> &Client {
        &self.client
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}
