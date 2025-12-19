use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};

use crate::{
    client::{Client, ClientError},
    config::AppConfig,
    error::{AppError, Result},
    ui,
};

use api_types::vault::{Vault, VaultSnapshot};

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
                self.should_quit = true;
            }
            crate::ui::keymap::AppAction::NextField => {
                self.advance_focus();
            }
            crate::ui::keymap::AppAction::Submit => {
                if self.state.screen == Screen::Login {
                    self.attempt_login().await?;
                }
            }
            crate::ui::keymap::AppAction::Backspace => {
                let field = self.active_field_mut();
                field.pop();
            }
            crate::ui::keymap::AppAction::Input(ch) => {
                if self.state.screen == Screen::Login {
                    let field = self.active_field_mut();
                    field.push(ch);
                } else {
                    self.handle_section_key(ch);
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

    #[allow(dead_code)]
    pub fn client(&self) -> &Client {
        &self.client
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    fn handle_section_key(&mut self, ch: char) {
        self.state.section = match ch {
            'h' | 'H' => Section::Home,
            't' | 'T' => Section::Transactions,
            'w' | 'W' => Section::Wallets,
            'f' | 'F' => Section::Flows,
            'v' | 'V' => Section::Vault,
            's' | 'S' => Section::Stats,
            _ => self.state.section,
        };
    }
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
