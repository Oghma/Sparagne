use clap::Parser;
use serde::Deserialize;
use std::env;

use crate::error::Result;

const DEFAULT_CONFIG_PATH: &str = "config/config.toml";
const DEFAULT_XDG_CONFIG: &str = "sparagne/config.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub base_url: String,
    pub username: String,
    pub vault: String,
    pub timezone: String,
    /// Home feed low-balance warning threshold in minor units.
    pub low_balance_minor: i64,
    /// Undo toast duration in seconds.
    pub undo_toast_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:3000".to_string(),
            username: String::new(),
            vault: "Main".to_string(),
            timezone: "Europe/Rome".to_string(),
            low_balance_minor: 25_00,
            undo_toast_secs: 5,
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
    /// Override low-balance warning threshold (minor units, e.g. 2500 = 25.00).
    #[arg(long)]
    low_balance_minor: Option<i64>,
    /// Override undo toast duration in seconds.
    #[arg(long)]
    undo_toast_secs: Option<u64>,
}

pub fn load() -> Result<AppConfig> {
    let args = Args::parse();

    let mut builder = config::Config::builder();
    if let Some(path) = args.config.or_else(|| env::var("SPARAGNE_CONFIG").ok()) {
        builder =
            builder.add_source(config::File::new(&path, config::FileFormat::Toml).required(false));
    } else {
        for path in default_config_paths() {
            builder = builder
                .add_source(config::File::new(&path, config::FileFormat::Toml).required(false));
        }
    }
    builder = builder.add_source(config::Environment::with_prefix("SPARAGNE").separator("__"));
    let settings = builder.build()?;
    let mut settings = match settings.get::<AppConfig>("tui") {
        Ok(config) => config,
        Err(config::ConfigError::NotFound(_)) => AppConfig::default(),
        Err(err) => return Err(err.into()),
    };

    if let Ok(base_url) = env::var("SPARAGNE_URL_SERVER") {
        settings.base_url = base_url;
    }
    if let Ok(username) = env::var("SPARAGNE_USERNAME") {
        settings.username = username;
    }
    if let Ok(vault) = env::var("SPARAGNE_VAULT") {
        settings.vault = vault;
    }
    if let Ok(timezone) = env::var("SPARAGNE_TIMEZONE") {
        settings.timezone = timezone;
    }
    if let Ok(low_balance_minor) = env::var("SPARAGNE_LOW_BALANCE_MINOR")
        && let Ok(parsed) = low_balance_minor.parse::<i64>()
    {
        settings.low_balance_minor = parsed;
    }
    if let Ok(undo_toast_secs) = env::var("SPARAGNE_UNDO_TOAST_SECS")
        && let Ok(parsed) = undo_toast_secs.parse::<u64>()
    {
        settings.undo_toast_secs = parsed.max(1);
    }

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
    if let Some(low_balance_minor) = args.low_balance_minor {
        settings.low_balance_minor = low_balance_minor;
    }
    if let Some(undo_toast_secs) = args.undo_toast_secs {
        settings.undo_toast_secs = undo_toast_secs.max(1);
    }

    Ok(settings)
}

fn default_config_paths() -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(home) = env::var("XDG_CONFIG_HOME")
        .ok()
        .or_else(|| env::var("HOME").ok().map(|home| format!("{home}/.config")))
    {
        paths.push(format!("{home}/{DEFAULT_XDG_CONFIG}"));
    }
    paths.push(DEFAULT_CONFIG_PATH.to_string());
    paths
}
