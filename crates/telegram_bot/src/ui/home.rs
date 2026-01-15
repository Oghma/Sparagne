use api_types::vault::VaultSnapshot;
use engine::Money;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    i18n::{self, TextKey},
    state::UserPrefs,
    ui::shared::flow_display_name,
};

pub(crate) fn render_home(
    locale: i18n::Locale,
    display_name: &str,
    snapshot: &VaultSnapshot,
    prefs: &UserPrefs,
) -> (String, InlineKeyboardMarkup) {
    // Find default wallet
    let default_wallet = prefs
        .default_wallet_id
        .and_then(|id| snapshot.wallets.iter().find(|w| w.id == id));

    let wallet_name = default_wallet
        .map(|w| w.name.as_str())
        .unwrap_or(i18n::t(locale, TextKey::UnsetValue));

    // Calculate balance from default wallet
    let currency = super::shared::api_currency_to_engine(snapshot.currency);
    let balance = default_wallet
        .map(|w| Money::new(w.balance_minor).format(currency))
        .unwrap_or_else(|| Money::new(0).format(currency));

    // Use last_flow if set, otherwise fall back to default_flow
    let current_flow = prefs
        .last_flow_id
        .or(prefs.default_flow_id)
        .and_then(|id| snapshot.flows.iter().find(|f| f.id == id));

    let flow_name = current_flow
        .map(|f| flow_display_name(locale, f.is_unallocated, &f.name))
        .unwrap_or(i18n::t(locale, TextKey::UnallocatedFlow));

    let text = i18n::format(
        locale,
        TextKey::HomeSummary,
        &[
            ("display_name", display_name),
            ("vault", snapshot.name.as_str()),
            ("wallet", wallet_name),
            ("flow", flow_name),
            ("balance", &balance),
        ],
    );

    let kb = InlineKeyboardMarkup::new(vec![
        // Row 1: Expense and Income
        vec![
            InlineKeyboardButton::callback(
                format!("➖ {}", i18n::t(locale, TextKey::HomeBtnExpense)),
                "home:expense",
            ),
            InlineKeyboardButton::callback(
                format!("➕ {}", i18n::t(locale, TextKey::HomeBtnIncome)),
                "home:income",
            ),
        ],
        // Row 2: History and Stats
        vec![
            InlineKeyboardButton::callback(
                format!("📜 {}", i18n::t(locale, TextKey::HomeBtnHistory)),
                "home:history",
            ),
            InlineKeyboardButton::callback(
                format!("📊 {}", i18n::t(locale, TextKey::HomeBtnStats)),
                "home:stats",
            ),
        ],
        // Row 3: Wallet and Flow pickers (inline)
        vec![
            InlineKeyboardButton::callback(format!("👛 {wallet_name}"), "home:wallet"),
            InlineKeyboardButton::callback(format!("🎯 {flow_name}"), "home:flow"),
        ],
    ]);

    (text, kb)
}

pub(crate) fn render_wallet_picker(
    locale: i18n::Locale,
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
        format!("⬅️ {}", i18n::t(locale, TextKey::DetailBtnBack)),
        back_callback,
    )]);

    (
        i18n::t(locale, TextKey::PickerWalletTitle).to_string(),
        InlineKeyboardMarkup::new(rows),
    )
}

pub(crate) fn render_flow_picker(
    locale: i18n::Locale,
    snapshot: &VaultSnapshot,
    back_callback: &str,
) -> (String, InlineKeyboardMarkup) {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for flow in snapshot.flows.iter().filter(|f| !f.archived) {
        rows.push(vec![InlineKeyboardButton::callback(
            flow_display_name(locale, flow.is_unallocated, &flow.name).to_string(),
            format!("flow:set:{id}", id = flow.id),
        )]);
    }
    rows.push(vec![InlineKeyboardButton::callback(
        format!("⬅️ {}", i18n::t(locale, TextKey::DetailBtnBack)),
        back_callback,
    )]);

    (
        i18n::t(locale, TextKey::PickerFlowTitle).to_string(),
        InlineKeyboardMarkup::new(rows),
    )
}
