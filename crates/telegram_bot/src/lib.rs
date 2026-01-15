//! Telegram bot.
//!
//! The bot is a thin client: it talks only to the HTTP server API and never
//! accesses the database directly.

use std::{path::PathBuf, sync::Arc};

use base64::Engine;
use reqwest::{Client, header};
use teloxide::{prelude::*, types::BotCommand};

mod api;
mod bot_client;
mod handlers;
mod i18n;
mod parsing;
mod routing;
mod state;
mod text;
mod ui;
mod use_cases;

const DEFAULT_STATE_PATH: &str = "config/telegram_bot_state.json";

#[derive(Clone)]
pub struct ConfigParameters {
    allowed_users: Option<Vec<UserId>>,
    api: Arc<dyn api::ApiGateway>,
    prefs: state::PrefsStore,
    sessions: state::SessionStore,
}

pub struct Bot {
    token: String,
    allowed_users: Option<Vec<UserId>>,
    server: String,
    client: Client,
    state_path: PathBuf,
}

impl Bot {
    pub fn new(
        token: &str,
        allowed_users: Option<Vec<UserId>>,
        server: &str,
        username: &str,
        password: &str,
        state_path: PathBuf,
    ) -> Result<Self, String> {
        // Basic authorization is in the form "Basic `secret`" where `secret` is
        // the base64 of the string "username:password".
        let secret = format!("{username}:{password}");
        let secret = format!("Basic {}", base64::prelude::BASE64_STANDARD.encode(secret));

        let mut auth = header::HeaderValue::try_from(secret)
            .map_err(|err| format!("invalid auth header value: {err}"))?;
        auth.set_sensitive(true);

        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, auth);

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|err| format!("failed to build http client: {err}"))?;

        Ok(Self {
            token: token.to_string(),
            allowed_users,
            server: server.to_string(),
            client,
            state_path,
        })
    }

    pub fn builder() -> BotBuilder {
        BotBuilder::default()
    }

    pub async fn run(&self) {
        tracing::info!("Starting telegram bot...");

        let bot = teloxide::Bot::new(&self.token);

        // Register bot commands with Telegram (shows in "/" menu)
        if let Err(err) = Self::register_commands(&bot).await {
            tracing::warn!("Failed to register bot commands: {err}");
        }

        let prefs = state::PrefsStore::load_or_empty(self.state_path.clone());

        let parameters = ConfigParameters {
            allowed_users: self.allowed_users.clone(),
            api: Arc::new(api::ApiClient::new(
                self.client.clone(),
                self.server.clone(),
            )),
            prefs,
            sessions: state::SessionStore::default(),
        };

        let handler = dptree::entry()
            .branch(Update::filter_message().endpoint(handlers::handle_message))
            .branch(Update::filter_callback_query().endpoint(handlers::handle_callback));

        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![parameters])
            .default_handler(|upd| async move {
                tracing::warn!("Unhandled update: {:?}", upd);
            })
            .error_handler(LoggingErrorHandler::with_custom_text(
                "An error has occurred in the dispatcher",
            ))
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }

    /// Registers bot commands with Telegram for the "/" menu.
    /// Registers both Italian (default) and English localized commands.
    async fn register_commands(bot: &teloxide::Bot) -> Result<(), teloxide::RequestError> {
        // Italian commands (default)
        let commands_it = vec![
            BotCommand::new("start", "Avvia il bot"),
            BotCommand::new("home", "Torna alla home"),
            BotCommand::new("help", "Mostra aiuto e sintassi"),
            BotCommand::new("categories", "Lista categorie"),
        ];

        // English commands
        let commands_en = vec![
            BotCommand::new("start", "Start the bot"),
            BotCommand::new("home", "Go to home"),
            BotCommand::new("help", "Show help and syntax"),
            BotCommand::new("categories", "List categories"),
        ];

        // Set default commands (Italian)
        bot.set_my_commands(commands_it).await?;

        // Set English commands for users with English language
        bot.set_my_commands(commands_en).language_code("en").await?;

        tracing::info!("Bot commands registered successfully");
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct BotBuilder {
    token: String,
    allowed_users: Option<Vec<UserId>>,
    server: String,
    username: String,
    password: String,
    state_path: Option<PathBuf>,
}

impl BotBuilder {
    pub fn token(mut self, token: &str) -> BotBuilder {
        self.token = token.to_string();
        self
    }

    pub fn allowed_users(mut self, allowed_users: Vec<UserId>) -> BotBuilder {
        if !allowed_users.is_empty() {
            self.allowed_users = Some(allowed_users);
        }
        self
    }

    pub fn server(mut self, server: &str, username: &str, password: &str) -> BotBuilder {
        self.server = server.to_string();
        self.username = username.to_string();
        self.password = password.to_string();
        self
    }

    pub fn state_path(mut self, path: impl Into<PathBuf>) -> BotBuilder {
        self.state_path = Some(path.into());
        self
    }

    pub fn build(self) -> Result<Bot, String> {
        tracing::info!("Initializing telegram bot...");
        let state_path = self
            .state_path
            .unwrap_or_else(|| PathBuf::from(DEFAULT_STATE_PATH));
        Bot::new(
            &self.token,
            self.allowed_users,
            &self.server,
            &self.username,
            &self.password,
            state_path,
        )
    }
}
