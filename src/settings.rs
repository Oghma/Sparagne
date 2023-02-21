///! Handles settings for the application. Configuration is written in
///! `settings.toml`.
///!
///! See `settings.toml` for the configuration.
use config::{Config, ConfigError, File};
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Sqlite {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub sqlite: Sqlite,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let settings = Config::builder()
            .add_source(File::with_name("settings"))
            .build()?;

        settings.try_deserialize()
    }
}
