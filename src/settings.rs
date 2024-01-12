//! Handles settings for the application. Configuration is written in
//! `settings.toml`.
//!
//! See `settings.toml` for the configuration.
use config::{Config, ConfigError, File};
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct App {
    pub level: String,
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum Database {
    Memory,
    Sqlite(String),
}

#[derive(Debug, Deserialize)]
pub struct Server {
    pub database: Database,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct Telegram {
    pub token: String,
    pub server: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub app: App,
    pub server: Option<Server>,
    pub telegram: Option<Telegram>,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let settings = Config::builder()
            .add_source(File::with_name("settings"))
            .build()?;

        settings.try_deserialize()
    }
}
