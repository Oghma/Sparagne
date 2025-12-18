use api_types::{
    stats::Statistic,
    transaction::{
        TransactionDetailResponse, TransactionKind, TransactionListResponse, TransactionView,
    },
    vault::VaultSnapshot,
};
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use uuid::Uuid;

use crate::state::UserPrefs;

pub(crate) fn render_home(
    snapshot: &VaultSnapshot,
    prefs: &UserPrefs,
) -> (String, InlineKeyboardMarkup) {
    let default_wallet = prefs
        .default_wallet_id
        .and_then(|id| snapshot.wallets.iter().find(|w| w.id == id))
        .map(|w| w.name.as_str())
        .unwrap_or("Non impostato");

    let default_flow = prefs
        .default_flow_id
        .and_then(|id| snapshot.flows.iter().find(|f| f.id == id))
        .map(|f| flow_display_name(f.is_unallocated, &f.name))
        .unwrap_or("Non in flow");

    let last_flow = prefs
        .last_flow_id
        .and_then(|id| snapshot.flows.iter().find(|f| f.id == id))
        .map(|f| flow_display_name(f.is_unallocated, &f.name))
        .unwrap_or("Non in flow");

    let text = format!(
        "Sparagne • Vault: {}\nWallet default: {}\nFlow default: {}\nUltimo flow: {}",
        snapshot.name, default_wallet, default_flow, last_flow
    );

    let kb = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("➖ Uscita", "home:expense"),
            InlineKeyboardButton::callback("➕ Entrata", "home:income"),
            InlineKeyboardButton::callback("↩ Refund", "home:refund"),
        ],
        vec![
            InlineKeyboardButton::callback("🧾 Ultime", "home:list"),
            InlineKeyboardButton::callback("📊 Stats", "home:stats"),
        ],
        vec![
            InlineKeyboardButton::callback("👛 Wallet default", "home:pick_wallet"),
            InlineKeyboardButton::callback("🎯 Flow default", "home:pick_flow"),
        ],
        vec![InlineKeyboardButton::callback("⚙️ Pairing", "home:pair")],
    ]);

    (text, kb)
}

pub(crate) fn render_wallet_picker(snapshot: &VaultSnapshot) -> (String, InlineKeyboardMarkup) {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for wallet in snapshot.wallets.iter().filter(|w| !w.archived) {
        rows.push(vec![InlineKeyboardButton::callback(
            wallet.name.clone(),
            format!("wallet:set:{id}", id = wallet.id),
        )]);
    }
    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Indietro",
        "nav:home",
    )]);

    (
        "Scegli il wallet di default:".to_string(),
        InlineKeyboardMarkup::new(rows),
    )
}

pub(crate) fn render_flow_picker(snapshot: &VaultSnapshot) -> (String, InlineKeyboardMarkup) {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for flow in snapshot.flows.iter().filter(|f| !f.archived) {
        rows.push(vec![InlineKeyboardButton::callback(
            flow_display_name(flow.is_unallocated, &flow.name).to_string(),
            format!("flow:set:{id}", id = flow.id),
        )]);
    }
    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Indietro",
        "nav:home",
    )]);

    (
        "Scegli il flow (ultimo flow usato):".to_string(),
        InlineKeyboardMarkup::new(rows),
    )
}

pub(crate) fn render_list(
    currency: EngineCurrency,
    list: &TransactionListResponse,
    include_voided: bool,
    has_prev: bool,
    has_next: bool,
) -> (String, InlineKeyboardMarkup) {
    let mut text = String::from("Ultime voci:\n");
    for (idx, tx) in list.transactions.iter().enumerate() {
        text.push_str(&format!(
            "\n{}. {} • {}{}{}{}",
            idx + 1,
            tx.occurred_at.date_naive(),
            Money::new(tx.amount_minor).format(currency),
            tx.category
                .as_deref()
                .map(|c| format!(" • {c}"))
                .unwrap_or_default(),
            tx.note
                .as_deref()
                .map(|n| format!(" • {n}"))
                .unwrap_or_default(),
            if tx.voided { " • void" } else { "" }
        ));
    }

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for tx in &list.transactions {
        rows.push(vec![InlineKeyboardButton::callback(
            tx_button_label(currency, tx),
            format!("tx:detail:{id}", id = tx.id),
        )]);
    }

    let mut nav_row: Vec<InlineKeyboardButton> = Vec::new();
    if has_prev {
        nav_row.push(InlineKeyboardButton::callback("⬅️ Prev", "list:prev"));
    }
    if has_next {
        nav_row.push(InlineKeyboardButton::callback("Next ➡️", "list:next"));
    }
    if !nav_row.is_empty() {
        rows.push(nav_row);
    }

    rows.push(vec![InlineKeyboardButton::callback(
        format!(
            "Mostra voided: {}",
            if include_voided { "On" } else { "Off" }
        ),
        "prefs:toggle_voided",
    )]);
    rows.push(vec![InlineKeyboardButton::callback("⬅️ Home", "nav:home")]);

    (text, InlineKeyboardMarkup::new(rows))
}

pub(crate) fn render_detail(
    currency: EngineCurrency,
    detail: &TransactionDetailResponse,
) -> (String, InlineKeyboardMarkup) {
    let tx = &detail.transaction;
    let mut text = format!(
        "Dettaglio\n\nKind: {:?}\nQuando: {}\nImporto: {}\nCategoria: {}\nNota: {}\nVoided: {}",
        tx.kind,
        tx.occurred_at,
        Money::new(tx.amount_minor).format(currency),
        tx.category.as_deref().unwrap_or("-"),
        tx.note.as_deref().unwrap_or("-"),
        if tx.voided { "sì" } else { "no" }
    );

    text.push_str("\n\nLegs:");
    for leg in &detail.legs {
        text.push_str(&format!(
            "\n- {:?}: {}",
            leg.target,
            Money::new(leg.amount_minor).format(currency)
        ));
    }

    let kb = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("↩ Void", format!("tx:void:{id}", id = tx.id)),
            InlineKeyboardButton::callback("✏️ Edit", format!("tx:edit:{id}", id = tx.id)),
            InlineKeyboardButton::callback("📌 Ripeti", format!("tx:repeat:{id}", id = tx.id)),
        ],
        vec![InlineKeyboardButton::callback("⬅️ Indietro", "nav:list")],
    ]);

    (text, kb)
}

pub(crate) fn render_edit_menu(tx_id: Uuid) -> (String, InlineKeyboardMarkup) {
    (
        "Cosa vuoi modificare?".to_string(),
        InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::callback("💶 Importo", format!("tx:edit_amount:{tx_id}")),
                InlineKeyboardButton::callback("📝 Nota", format!("tx:edit_note:{tx_id}")),
            ],
            vec![InlineKeyboardButton::callback(
                "⬅️ Indietro",
                format!("tx:detail:{tx_id}"),
            )],
        ]),
    )
}

pub(crate) fn render_stats(
    currency: EngineCurrency,
    stats: &Statistic,
) -> (String, InlineKeyboardMarkup) {
    let text = format!(
        "Stats\n\nBilancio: {}\nTotale entrate: {}\nTotale uscite: {}",
        Money::new(stats.balance_minor).format(currency),
        Money::new(stats.total_income_minor).format(currency),
        Money::new(stats.total_expenses_minor).format(currency),
    );
    let kb = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "⬅️ Home",
        "nav:home",
    )]]);
    (text, kb)
}

pub(crate) fn flow_display_name(is_unallocated: bool, name: &str) -> &str {
    if is_unallocated { "Non in flow" } else { name }
}

fn tx_button_label(currency: EngineCurrency, tx: &TransactionView) -> String {
    let amount = Money::new(tx.amount_minor).format(currency);
    let kind = match tx.kind {
        TransactionKind::Income => "+",
        TransactionKind::Expense => "-",
        TransactionKind::Refund => "r",
        TransactionKind::TransferWallet => "tw",
        TransactionKind::TransferFlow => "tf",
    };
    let category = tx.category.as_deref().unwrap_or("-");
    let voided = if tx.voided { " • void" } else { "" };
    format!("{kind} {amount} • {category}{voided}")
}
