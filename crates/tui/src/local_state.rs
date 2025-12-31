use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

const DEFAULT_STATE_PATH: &str = "config/tui_state.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalState {
    pub defaults: Vec<DefaultsEntry>,
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
}

pub fn default_state_path() -> &'static str {
    DEFAULT_STATE_PATH
}
