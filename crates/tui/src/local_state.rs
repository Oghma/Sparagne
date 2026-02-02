use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

const DEFAULT_STATE_PATH: &str = "config/tui_state.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalState {
    pub defaults: Vec<DefaultsEntry>,
    /// Per-user command history for the command palette.
    #[serde(default)]
    pub command_history: Vec<CommandHistoryEntry>,
}

/// Tracks recently used palette commands for a user/vault combination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHistoryEntry {
    pub username: String,
    pub vault_id: String,
    /// Command names in MRU order (most recent first).
    pub recent_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsEntry {
    pub username: String,
    pub vault_id: String,
    pub default_wallet_id: Option<Uuid>,
    pub default_flow_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy)]
pub struct DefaultsValue {
    pub wallet_id: Option<Uuid>,
    pub flow_id: Option<Uuid>,
}

impl LocalState {
    pub fn load(path: &str) -> Result<Self> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(err) => return Err(err.into()),
        };
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self, path: &str) -> Result<()> {
        let parent = Path::new(path).parent();
        if let Some(parent) = parent {
            fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_string_pretty(self)?;
        fs::write(path, payload)?;
        Ok(())
    }

    pub fn defaults_for(&self, username: &str, vault_id: &str) -> Option<DefaultsValue> {
        self.defaults
            .iter()
            .find(|entry| entry.username == username && entry.vault_id == vault_id)
            .map(|entry| DefaultsValue {
                wallet_id: entry.default_wallet_id,
                flow_id: entry.default_flow_id,
            })
    }

    pub fn set_defaults(
        &mut self,
        username: &str,
        vault_id: &str,
        wallet_id: Option<Uuid>,
        flow_id: Option<Uuid>,
    ) {
        if let Some(entry) = self
            .defaults
            .iter_mut()
            .find(|entry| entry.username == username && entry.vault_id == vault_id)
        {
            entry.default_wallet_id = wallet_id;
            entry.default_flow_id = flow_id;
            return;
        }

        self.defaults.push(DefaultsEntry {
            username: username.to_string(),
            vault_id: vault_id.to_string(),
            default_wallet_id: wallet_id,
            default_flow_id: flow_id,
        });
    }

    /// Returns the recent command names for a user/vault.
    pub fn mru_commands_for(&self, username: &str, vault_id: &str) -> Vec<String> {
        self.command_history
            .iter()
            .find(|entry| entry.username == username && entry.vault_id == vault_id)
            .map(|entry| entry.recent_commands.clone())
            .unwrap_or_default()
    }

    /// Adds a command to the MRU list for a user/vault.
    pub fn push_mru_command(
        &mut self,
        username: &str,
        vault_id: &str,
        command: &str,
        limit: usize,
    ) {
        let entry = self
            .command_history
            .iter_mut()
            .find(|e| e.username == username && e.vault_id == vault_id);

        if let Some(entry) = entry {
            // Remove if already present to avoid duplicates
            entry.recent_commands.retain(|c| c != command);
            // Insert at front
            entry.recent_commands.insert(0, command.to_string());
            // Trim to limit
            entry.recent_commands.truncate(limit);
        } else {
            self.command_history.push(CommandHistoryEntry {
                username: username.to_string(),
                vault_id: vault_id.to_string(),
                recent_commands: vec![command.to_string()],
            });
        }
    }
}

pub fn default_state_path() -> &'static str {
    DEFAULT_STATE_PATH
}
