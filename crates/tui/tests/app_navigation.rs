use crossterm::event::{KeyCode, KeyEvent};

use api_types::{
    Currency,
    transaction::{TransactionKind, TransactionView},
    vault::{FlowView, Vault, VaultSnapshot, WalletView},
};
use chrono::{FixedOffset, Utc};
use sparagne_tui::{
    app::{App, Screen, Section, TransactionFormField, TransactionsMode},
    config::AppConfig,
};
use uuid::Uuid;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::from(code)
}

fn sample_vault() -> Vault {
    Vault {
        id: Some("vault-test".to_string()),
        name: Some("Test Vault".to_string()),
        currency: Some(Currency::Eur),
        owner: Some("tester".to_string()),
    }
}

fn sample_snapshot() -> VaultSnapshot {
    let wallet_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    VaultSnapshot {
        id: "vault-test".to_string(),
        name: "Test Vault".to_string(),
        currency: Currency::Eur,
        owner: Some("tester".to_string()),
        wallets: vec![WalletView {
            id: wallet_id,
            name: "Cash".to_string(),
            balance_minor: 0,
            archived: false,
        }],
        flows: vec![FlowView {
            id: flow_id,
            name: "Main".to_string(),
            balance_minor: 0,
            archived: false,
            is_unallocated: true,
            allow_negative: false,
            max_balance: None,
            is_shared: false,
            is_reference: false,
            owner_user_id: None,
        }],
        unallocated_flow_id: flow_id,
    }
}

fn sample_transaction_view() -> TransactionView {
    let offset = match FixedOffset::east_opt(0) {
        Some(offset) => offset,
        None => panic!("expected fixed offset for UTC"),
    };
    let occurred_at = Utc::now().with_timezone(&offset);
    TransactionView {
        id: Uuid::new_v4(),
        kind: TransactionKind::Expense,
        occurred_at,
        amount_minor: 1000,
        category_id: Uuid::new_v4(),
        category: Some("Food".to_string()),
        note: None,
        voided: false,
        wallet_id: None,
        flow_id: None,
    }
}

fn test_app() -> App {
    let mut app =
        App::new(AppConfig::default()).unwrap_or_else(|err| panic!("create test app: {err}"));
    app.state_mut().screen = Screen::Home;
    app.state_mut().section = Section::Transactions;
    app.state_mut().vault = Some(sample_vault());
    app.state_mut().snapshot = Some(sample_snapshot());
    app
}

#[tokio::test]
async fn transaction_form_flow() {
    let mut app = test_app();

    assert!(app.handle_key(key(KeyCode::Char('i'))).await.is_ok());
    assert_eq!(app.state_mut().transactions.mode, TransactionsMode::Form);

    for ch in "50.00".chars() {
        assert!(app.handle_key(key(KeyCode::Char(ch))).await.is_ok());
    }
    assert_eq!(app.state_mut().transactions.form.amount.value(), "50.00");

    assert!(app.handle_key(key(KeyCode::Tab)).await.is_ok());
    assert_eq!(
        app.state_mut().transactions.form.focus,
        TransactionFormField::Wallet
    );

    assert!(app.handle_key(key(KeyCode::Esc)).await.is_ok());
    assert!(app.state_mut().overlays.has_confirm_dialog());
    assert!(app.handle_key(key(KeyCode::Char('d'))).await.is_ok());
    assert_eq!(app.state_mut().transactions.mode, TransactionsMode::List);
}

#[tokio::test]
async fn section_navigation_without_fetch() {
    let mut app = test_app();
    app.state_mut()
        .transactions
        .items
        .push(sample_transaction_view());

    assert!(app.handle_key(key(KeyCode::Char('t'))).await.is_ok());
    assert_eq!(app.state_mut().section, Section::Transactions);

    assert!(app.handle_key(key(KeyCode::Char('a'))).await.is_ok());
    assert_eq!(app.state_mut().section, Section::Accounts);
}

#[tokio::test]
async fn list_boundary_navigation() {
    let mut app = test_app();
    app.state_mut().transactions.mode = TransactionsMode::List;
    app.state_mut().transactions.items = (0..5).map(|_| sample_transaction_view()).collect();

    app.state_mut().transactions.selected = 0;
    assert!(app.handle_key(key(KeyCode::Up)).await.is_ok());
    assert_eq!(app.state_mut().transactions.selected, 0);

    app.state_mut().transactions.selected = 4;
    assert!(app.handle_key(key(KeyCode::Down)).await.is_ok());
    assert_eq!(app.state_mut().transactions.selected, 4);
}
