use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use teloxide::types::{ChatId, MessageId};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::parsing::{QuickAdd, QuickKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct UserPrefs {
    pub active_vault_name: String,
    pub default_wallet_id: Option<Uuid>,
    pub default_flow_id: Option<Uuid>,
    pub last_flow_id: Option<Uuid>,
    pub include_voided: bool,
}

impl Default for UserPrefs {
    fn default() -> Self {
        Self {
            active_vault_name: "Main".to_string(),
            default_wallet_id: None,
            default_flow_id: None,
            last_flow_id: None,
            include_voided: false,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PrefsFile {
    users: HashMap<String, UserPrefs>,
}

#[derive(Clone)]
pub(crate) struct PrefsStore {
    path: PathBuf,
    inner: Arc<Mutex<PrefsFile>>,
}

impl PrefsStore {
    pub(crate) fn load_or_empty(path: PathBuf) -> Self {
        let file = read_json_file(&path).unwrap_or_default();
        Self {
            path,
            inner: Arc::new(Mutex::new(file)),
        }
    }

    pub(crate) async fn get_or_default(&self, telegram_user_id: u64) -> UserPrefs {
        let key = telegram_user_id.to_string();
        let mut guard = self.inner.lock().await;
        let prefs = guard.users.entry(key).or_insert_with(UserPrefs::default);
        prefs.clone()
    }

    pub(crate) async fn update<F>(&self, telegram_user_id: u64, f: F) -> Result<UserPrefs, String>
    where
        F: FnOnce(&mut UserPrefs),
    {
        let key = telegram_user_id.to_string();
        let mut guard = self.inner.lock().await;
        let prefs = guard.users.entry(key).or_insert_with(UserPrefs::default);
        f(prefs);

        let snapshot = prefs.clone();
        write_json_file(&self.path, &guard).map_err(|e| format!("state save failed: {e}"))?;
        Ok(snapshot)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DraftCreate {
    pub kind: QuickKind,
    pub amount_minor: i64,
    pub category: Option<String>,
    pub note: Option<String>,
    pub idempotency_key: String,
}

impl From<(QuickAdd, String)> for DraftCreate {
    fn from((draft, idempotency_key): (QuickAdd, String)) -> Self {
        Self {
            kind: draft.kind,
            amount_minor: draft.amount_minor,
            category: draft.category,
            note: draft.note,
            idempotency_key,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum PendingAction {
    PairCode,
    WalletForQuickAdd(DraftCreate),
    EditAmount { tx_id: Uuid },
    EditNote { tx_id: Uuid },
    WizardDraft { kind: QuickKind },
}

#[derive(Clone, Debug)]
pub(crate) struct ListSession {
    pub wallet_id: Uuid,
    pub cursors: Vec<Option<String>>,
    pub current: Option<String>,
    pub next: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct WizardSession {
    pub kind: QuickKind,
    pub category: Option<String>,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Session {
    pub hub_message_id: Option<MessageId>,
    pub display_name: Option<String>,
    pub pending: Option<PendingAction>,
    pub list: Option<ListSession>,
    pub last_detail_tx: Option<Uuid>,
    pub wizard: Option<WizardSession>,
}

#[derive(Clone, Default)]
pub(crate) struct SessionStore {
    inner: Arc<Mutex<HashMap<ChatId, Session>>>,
}

impl SessionStore {
    pub(crate) async fn get(&self, chat_id: ChatId) -> Session {
        let guard = self.inner.lock().await;
        guard.get(&chat_id).cloned().unwrap_or_default()
    }

    pub(crate) async fn update<F>(&self, chat_id: ChatId, f: F) -> Session
    where
        F: FnOnce(&mut Session),
    {
        let mut guard = self.inner.lock().await;
        let session = guard.entry(chat_id).or_insert_with(Session::default);
        f(session);
        session.clone()
    }
}

fn read_json_file(path: &Path) -> Option<PrefsFile> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_json_file(path: &Path, prefs: &PrefsFile) -> Result<(), std::io::Error> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent)?;

    let json = serde_json::to_string_pretty(prefs)
        .map_err(|_| std::io::Error::other("serialize failed"))?;

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, json)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(&tmp, path)?;
            let _ = fs::remove_file(&tmp);
            Ok(())
        }
    }
}
