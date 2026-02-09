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
    api::{ApiError, mock::MockApi},
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
            allow_negative: false,
        }],
        unallocated_flow_id: flow_id,
    }
}

fn sample_datetime() -> DateTime<FixedOffset> {
    expect_ok(
        DateTime::parse_from_rfc3339("2024-01-01T12:00:00+00:00"),
        "rfc3339",
    )
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
        wallet_id: None,
        flow_id: None,
    }
}

fn expect_ok<T, E: std::fmt::Debug>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(err) => panic!("{context}: {err:?}"),
    }
}

fn expect_some<T>(value: Option<T>, context: &str) -> T {
    match value {
        Some(value) => value,
        None => panic!("{context}"),
    }
}

fn set_mock<T>(slot: &std::sync::Mutex<Option<Result<T, ApiError>>>, value: Result<T, ApiError>) {
    let mut guard = match slot.lock() {
        Ok(guard) => guard,
        Err(_) => panic!("mock api lock"),
    };
    *guard = Some(value);
}

#[tokio::test]
async fn home_flow_sends_summary_and_sets_hub_message() {
    let api = Arc::new(MockApi::new());
    let wallet_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let snapshot = sample_snapshot(wallet_id, flow_id);
    set_mock(&api.vault_snapshot, Ok(snapshot));

    let cfg = test_config(api.clone());
    let bot = MockBot::new();
    let chat_id = ChatId(10);
    let user_id = 42;
    let locale = i18n::default_locale();

    expect_ok(
        home::show_home(&bot, chat_id, user_id, &cfg, locale).await,
        "home flow",
    );

    let sent = expect_some(bot.last_sent(), "sent message");
    assert_eq!(sent.chat_id, chat_id);
    assert!(sent.has_kb);
    // Vault name is shown with emoji prefix
    assert!(sent.text.contains("Main"));
    let session = cfg.sessions.get(chat_id).await;
    assert!(session.hub_message_id.is_some());

    set_mock(&api.vault_snapshot, Ok(sample_snapshot(wallet_id, flow_id)));
    expect_ok(
        home::show_home(&bot, chat_id, user_id, &cfg, locale).await,
        "home flow edit",
    );

    let edited = expect_some(bot.last_edited(), "edited message");
    assert_eq!(edited.chat_id, chat_id);
    assert_eq!(edited.message_id, sent.message_id);
    assert!(edited.has_kb);
    assert!(edited.text.contains("Main"));
}

#[tokio::test]
async fn home_flow_shows_english_for_english_locale() {
    let api = Arc::new(MockApi::new());
    let wallet_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let snapshot = sample_snapshot(wallet_id, flow_id);
    set_mock(&api.vault_snapshot, Ok(snapshot));

    let cfg = test_config(api.clone());
    let bot = MockBot::new();
    let chat_id = ChatId(13);
    let user_id = 42;
    let locale = i18n::resolve_locale(Some("en-US"));

    expect_ok(
        home::show_home(&bot, chat_id, user_id, &cfg, locale).await,
        "home flow",
    );

    let sent = expect_some(bot.last_sent(), "sent message");
    // English locale should show "Budget" instead of Italian "Budget"
    assert!(sent.text.contains("Budget:"));
}

#[tokio::test]
async fn wizard_flow_renders_title_and_body() {
    let api = Arc::new(MockApi::new());
    let wallet_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let snapshot = sample_snapshot(wallet_id, flow_id);
    set_mock(&api.vault_snapshot, Ok(snapshot));
    set_mock(
        &api.transactions_list,
        Ok(TransactionListResponse {
            transactions: vec![sample_transaction(Uuid::new_v4(), TransactionKind::Expense)],
            next_cursor: None,
        }),
    );

    let cfg = test_config(api.clone());
    let user_id = 42;
    expect_ok(
        cfg.prefs
            .update(user_id, |prefs| {
                prefs.default_wallet_id = Some(wallet_id);
                prefs.default_flow_id = Some(flow_id);
                prefs.last_flow_id = Some(flow_id);
            })
            .await,
        "prefs update",
    );

    let bot = MockBot::new();
    let chat_id = ChatId(11);
    let locale = i18n::default_locale();

    expect_ok(
        wizard::start_wizard(&bot, chat_id, user_id, &cfg, QuickKind::Expense, locale).await,
        "wizard flow",
    );

    let sent = expect_some(bot.last_sent(), "sent message");
    assert_eq!(sent.chat_id, chat_id);
    assert!(sent.has_kb);
    assert!(sent.text.contains("Nuova Spesa"));
    // Simplified wizard body shows Wallet/Budget with emojis
    assert!(sent.text.contains("Wallet:"));
    assert!(sent.text.contains("Budget:"));
}

#[tokio::test]
async fn list_flow_renders_transactions() {
    let api = Arc::new(MockApi::new());
    let wallet_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let snapshot = sample_snapshot(wallet_id, flow_id);
    set_mock(&api.vault_snapshot, Ok(snapshot));
    set_mock(
        &api.transactions_list,
        Ok(TransactionListResponse {
            transactions: vec![sample_transaction(Uuid::new_v4(), TransactionKind::Expense)],
            next_cursor: None,
        }),
    );

    let cfg = test_config(api.clone());
    let user_id = 42;
    expect_ok(
        cfg.prefs
            .update(user_id, |prefs| {
                prefs.default_wallet_id = Some(wallet_id);
            })
            .await,
        "prefs update",
    );

    let bot = MockBot::new();
    let chat_id = ChatId(12);
    let locale = i18n::default_locale();

    expect_ok(
        list::show_list(&bot, chat_id, user_id, &cfg, locale).await,
        "list flow",
    );

    let sent = expect_some(bot.last_sent(), "sent message");
    assert_eq!(sent.chat_id, chat_id);
    assert!(sent.has_kb);
    assert!(sent.text.contains("Ultime transazioni:"));
    assert!(sent.text.contains("Food"));
    assert!(sent.text.contains("Lunch"));
}
