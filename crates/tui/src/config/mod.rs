use clap::Parser;
use serde::Deserialize;

use crate::error::Result;

const DEFAULT_CONFIG_PATH: &str = "config/tui.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub base_url: String,
    pub username: String,
    pub vault: String,
    pub timezone: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:3000".to_string(),
            username: String::new(),
            vault: "Main".to_string(),
            timezone: "Europe/Rome".to_string(),
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "sparagne_tui", disable_version_flag = true)]
struct Args {
    /// Optional config file path (TOML).
    #[arg(long)]
    config: Option<String>,
    /// Override base URL (e.g. http://127.0.0.1:3000).
    #[arg(long)]
    base_url: Option<String>,
    /// Override username (password is never read from CLI).
    #[arg(long)]
    username: Option<String>,
    /// Override vault name.
    #[arg(long)]
    vault: Option<String>,
    /// Override timezone (IANA name).
    #[arg(long)]
    timezone: Option<String>,
}

pub fn load() -> Result<AppConfig> {
    let args = Args::parse();

    let config_path = args.config.as_deref().unwrap_or(DEFAULT_CONFIG_PATH);
    let mut builder = config::Config::builder();
    builder = builder.add_source(config::File::with_name(config_path).required(false));
    builder = builder.add_source(config::Environment::with_prefix("SPARAGNE_TUI"));
    let mut settings: AppConfig = builder.build()?.try_deserialize()?;

    if let Some(base_url) = args.base_url {
        settings.base_url = base_url;
    }
    if let Some(username) = args.username {
        settings.username = username;
    }
    if let Some(vault) = args.vault {
        settings.vault = vault;
    }
    if let Some(timezone) = args.timezone {
        settings.timezone = timezone;
    }

    Ok(settings)
}
