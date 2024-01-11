//! Library for the telegram bot
use base64::Engine;
use reqwest::{header, Client};
use teloxide::{prelude::*, Bot as TBot};

use crate::{
    commands::HandleUserAccount,
    handlers::{handle_pair_user, handle_user_commands, UserCommands},
};

mod commands;
mod handlers;

#[derive(Clone)]
pub struct ConfigParameters {
    allowed_users: Option<Vec<UserId>>,
    client: Client,
    server: String,
}

pub struct Bot {
    token: String,
    allowed_users: Option<Vec<UserId>>,
    server: String,
    client: Client,
}

impl Bot {
    pub fn new(
        token: &str,
        allowed_users: Option<Vec<UserId>>,
        server: &str,
        username: &str,
        password: &str,
    ) -> Self {
        // Basic authorization is in the form "Basic `secret`" where `secret` is
        // the base64 of the string "username:password"
        let secret = format!("{}:{}", username, password);
        let secret = format!("Basic {}", base64::prelude::BASE64_STANDARD.encode(secret));

        let mut auth = header::HeaderValue::try_from(secret).unwrap();
        auth.set_sensitive(true);

        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, auth);

        let client = Client::builder().default_headers(headers).build().unwrap();

        Self {
            token: token.to_string(),
            allowed_users,
            server: server.to_string(),
            client,
        }
    }

    pub fn builder() -> BotBuilder {
        BotBuilder::default()
    }

    /// Run the telegram bot.
    pub async fn run(&self) {
        tracing::info!("Starting telegram bot...");

        let bot = TBot::new(&self.token);
        let parameters = ConfigParameters {
            allowed_users: self.allowed_users.clone(),
            client: self.client.clone(),
            server: self.server.clone(),
        };

        let handler = Update::filter_message()
            .branch(
                dptree::filter(|cfg: ConfigParameters, msg: Message| {
                    msg.from()
                        .map(|user| match cfg.allowed_users {
                            None => true,
                            Some(ids) => ids.contains(&user.id),
                        })
                        .unwrap_or_default()
                })
                .filter_command::<UserCommands>()
                .endpoint(handle_user_commands),
            )
            .branch(
                dptree::entry()
                    .filter_command::<HandleUserAccount>()
                    .endpoint(handle_pair_user),
            );

        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![parameters])
            .default_handler(|upd| async move {
                tracing::warn!("Unhandled update {:?}", upd);
            })
            .error_handler(LoggingErrorHandler::with_custom_text(
                "An error has occured in the dispatcher",
            ))
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }
}

#[derive(Default, Debug)]
pub struct BotBuilder {
    token: String,
    allowed_users: Option<Vec<UserId>>,
    server: String,
    username: String,
    password: String,
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

    pub fn build(self) -> Bot {
        tracing::info!("Initializing...");
        Bot::new(
            &self.token,
            self.allowed_users,
            &self.server,
            &self.username,
            &self.password,
        )
    }
}
