use api_types::vault::VaultSnapshot;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{state::UserPrefs, ui::shared::flow_display_name};

pub(crate) fn render_home(
    display_name: &str,
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
        "{display_name} • Vault: {}\nWallet default: {}\nFlow default: {}\nUltimo flow: {}",
        snapshot.name, default_wallet, default_flow, last_flow,
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
    ]);

    (text, kb)
}

pub(crate) fn render_wallet_picker(
    snapshot: &VaultSnapshot,
    back_callback: &str,
) -> (String, InlineKeyboardMarkup) {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for wallet in snapshot.wallets.iter().filter(|w| !w.archived) {
        rows.push(vec![InlineKeyboardButton::callback(
            wallet.name.clone(),
            format!("wallet:set:{id}", id = wallet.id),
        )]);
    }
    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Indietro",
        back_callback,
    )]);

    (
        "Scegli il wallet di default:".to_string(),
        InlineKeyboardMarkup::new(rows),
    )
}

pub(crate) fn render_flow_picker(
    snapshot: &VaultSnapshot,
    back_callback: &str,
) -> (String, InlineKeyboardMarkup) {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for flow in snapshot.flows.iter().filter(|f| !f.archived) {
        rows.push(vec![InlineKeyboardButton::callback(
            flow_display_name(flow.is_unallocated, &flow.name).to_string(),
            format!("flow:set:{id}", id = flow.id),
        )]);
    }
    rows.push(vec![InlineKeyboardButton::callback(
        "⬅️ Indietro",
        back_callback,
    )]);

    (
        "Scegli il flow (ultimo flow usato):".to_string(),
        InlineKeyboardMarkup::new(rows),
    )
}
