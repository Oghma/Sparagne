use std::{path::PathBuf, sync::Arc};

use api_types::{
    Currency,
    transaction::{TransactionKind, TransactionListResponse, TransactionView},
    vault::{FlowView, VaultSnapshot, WalletView},
};
use chrono::{DateTime, FixedOffset};
use teloxide::types::ChatId;
use uuid::Uuid;

use crate::{
    ConfigParameters,
    api::mock::MockApi,
    bot_client::mock::MockBot,
    i18n,
    parsing::QuickKind,
    state::{PrefsStore, SessionStore},
    use_cases::{home, list, wizard},
};

fn test_state_path() -> PathBuf {
    std::env::temp_dir().join(format!("sparagne-tg-state-{}.json", Uuid::new_v4()))
}

fn test_config(api: Arc<MockApi>) -> ConfigParameters {
    ConfigParameters {
        allowed_users: None,
        api,
        prefs: PrefsStore::load_or_empty(test_state_path()),
        sessions: SessionStore::default(),
    }
}

fn sample_snapshot(wallet_id: Uuid, flow_id: Uuid) -> VaultSnapshot {
    VaultSnapshot {
        id: "vault-1".to_string(),
        name: "Main".to_string(),
        currency: Currency::Eur,
        owner: None,
        wallets: vec![WalletView {
            id: wallet_id,
            name: "Cash".to_string(),
            balance_minor: 12_500,
            archived: false,
        }],
        flows: vec![FlowView {
            id: flow_id,
            name: "General".to_string(),
            balance_minor: 5_000,
            archived: false,
            is_unallocated: false,
        }],
        unallocated_flow_id: flow_id,
    }
}

fn sample_datetime() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2024-01-01T12:00:00+00:00").expect("rfc3339")
}

fn sample_transaction(id: Uuid, kind: TransactionKind) -> TransactionView {
    TransactionView {
        id,
        kind,
        occurred_at: sample_datetime(),
        amount_minor: 1_250,
        category_id: Uuid::new_v4(),
        category: Some("Food".to_string()),
        note: Some("Lunch".to_string()),
        voided: false,
    }
}

#[tokio::test]
async fn home_flow_sends_summary_and_sets_hub_message() {
    let api = Arc::new(MockApi::new());
    let wallet_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let snapshot = sample_snapshot(wallet_id, flow_id);
    *api.vault_snapshot_main.lock().expect("mock api lock") = Some(Ok(snapshot));

    let cfg = test_config(api.clone());
    let bot = MockBot::new();
    let chat_id = ChatId(10);
    let user_id = 42;
    let locale = i18n::default_locale();

    home::show_home(&bot, chat_id, user_id, &cfg, locale)
        .await
        .expect("home flow");

    let text = bot.last_sent_text().expect("sent text");
    assert!(text.contains("Vault: Main"));
    let session = cfg.sessions.get(chat_id).await;
    assert!(session.hub_message_id.is_some());
}

#[tokio::test]
async fn wizard_flow_renders_title_and_body() {
    let api = Arc::new(MockApi::new());
    let wallet_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let snapshot = sample_snapshot(wallet_id, flow_id);
    *api.vault_snapshot_main.lock().expect("mock api lock") = Some(Ok(snapshot));
    *api.transactions_list.lock().expect("mock api lock") = Some(Ok(TransactionListResponse {
        transactions: vec![sample_transaction(Uuid::new_v4(), TransactionKind::Expense)],
        next_cursor: None,
    }));

    let cfg = test_config(api.clone());
    let user_id = 42;
    cfg.prefs
        .update(user_id, |prefs| {
            prefs.default_wallet_id = Some(wallet_id);
            prefs.default_flow_id = Some(flow_id);
            prefs.last_flow_id = Some(flow_id);
        })
        .await
        .expect("prefs update");

    let bot = MockBot::new();
    let chat_id = ChatId(11);
    let locale = i18n::default_locale();

    wizard::start_wizard(&bot, chat_id, user_id, &cfg, QuickKind::Expense, locale)
        .await
        .expect("wizard flow");

    let text = bot.last_sent_text().expect("sent text");
    assert!(text.contains("Nuova uscita"));
    assert!(text.contains("Wallet:"));
    assert!(text.contains("Flow:"));
}

#[tokio::test]
async fn list_flow_renders_transactions() {
    let api = Arc::new(MockApi::new());
    let wallet_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let snapshot = sample_snapshot(wallet_id, flow_id);
    *api.vault_snapshot_main.lock().expect("mock api lock") = Some(Ok(snapshot));
    *api.transactions_list.lock().expect("mock api lock") = Some(Ok(TransactionListResponse {
        transactions: vec![sample_transaction(Uuid::new_v4(), TransactionKind::Expense)],
        next_cursor: None,
    }));

    let cfg = test_config(api.clone());
    let user_id = 42;
    cfg.prefs
        .update(user_id, |prefs| {
            prefs.default_wallet_id = Some(wallet_id);
        })
        .await
        .expect("prefs update");

    let bot = MockBot::new();
    let chat_id = ChatId(12);
    let locale = i18n::default_locale();

    list::show_list(&bot, chat_id, user_id, &cfg, locale)
        .await
        .expect("list flow");

    let text = bot.last_sent_text().expect("sent text");
    assert!(text.contains("Ultime voci:"));
    assert!(text.contains("Food"));
    assert!(text.contains("Lunch"));
}
