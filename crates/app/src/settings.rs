//! Handles settings for the application.
//!
//! The default config path is `config/config.toml`, with optional overrides
//! from `$XDG_CONFIG_HOME/sparagne/config.toml` and `SPARAGNE_CONFIG`.
use config::{Config, ConfigError, File, FileFormat};
use serde::Deserialize;
use std::env;

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
    /// Bind address for the HTTP server.
    ///
    /// Examples: `127.0.0.1` (local only), `0.0.0.0` (all interfaces, e.g.
    /// docker).
    pub bind: Option<String>,
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
        let mut builder = Config::builder();
        for path in config_paths() {
            builder = builder.add_source(File::new(&path, FileFormat::Toml).required(false));
        }
        let settings = builder.build()?.try_deserialize()?;
        apply_env_overrides(settings)
    }

    pub fn redacted_log_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();

        lines.push(format!("app.level={}", self.app.level));

        if let Some(server) = &self.server {
            let db = match &server.database {
                Database::Memory => "sqlite::memory".to_string(),
                Database::Sqlite(path) => format!("sqlite:{path}?mode=rwc"),
            };
            lines.push(format!(
                "server.bind={}",
                server.bind.as_deref().unwrap_or("127.0.0.1")
            ));
            lines.push(format!("server.port={}", server.port));
            lines.push(format!("server.database={}", db));
        } else {
            lines.push("server.disabled=true".to_string());
        }

        if let Some(telegram) = &self.telegram {
            lines.push(format!("telegram.server={}", telegram.server));
            lines.push(format!("telegram.username={}", telegram.username));
        } else {
            lines.push("telegram.disabled=true".to_string());
        }

        lines
    }
}

fn apply_env_overrides(mut settings: Settings) -> Result<Settings, ConfigError> {
    if let Ok(server_override) = env::var("SPARAGNE_SERVER") {
        let Some(server) = settings.server.as_mut() else {
            return Err(ConfigError::Message(
                "SPARAGNE_SERVER requires [server] in config".to_string(),
            ));
        };
        let (bind, port) = parse_server_override(server_override.as_str())?;
        server.bind = Some(bind);
        server.port = port;
    }
    Ok(settings)
}

fn parse_server_override(value: &str) -> Result<(String, u16), ConfigError> {
    let (bind, port) = value
        .rsplit_once(':')
        .ok_or_else(|| ConfigError::Message("SPARAGNE_SERVER must be host:port".to_string()))?;
    let port = port
        .parse::<u16>()
        .map_err(|err| ConfigError::Message(format!("SPARAGNE_SERVER invalid port: {err}")))?;
    if bind.is_empty() {
        return Err(ConfigError::Message(
            "SPARAGNE_SERVER requires a non-empty host".to_string(),
        ));
    }
    Ok((bind.to_string(), port))
}

fn config_paths() -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(home) = env::var("XDG_CONFIG_HOME")
        .ok()
        .or_else(|| env::var("HOME").ok().map(|home| format!("{home}/.config")))
    {
        paths.push(format!("{home}/sparagne/config.toml"));
    }
    paths.push("config/config.toml".to_string());
    if let Ok(path) = env::var("SPARAGNE_CONFIG") {
        paths.push(path);
    }
    paths
}
