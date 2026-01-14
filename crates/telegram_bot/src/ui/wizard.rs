use api_types::{transaction::TransactionView, vault::VaultSnapshot};
use engine::Currency as EngineCurrency;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    parsing::QuickKind,
    state::{UserPrefs, WizardSession},
    ui::shared::{flow_display_name, tx_button_label},
};

pub(crate) fn render_wizard(
    currency: EngineCurrency,
    snapshot: &VaultSnapshot,
    prefs: &UserPrefs,
    wizard: &WizardSession,
    recents: &[TransactionView],
) -> (String, InlineKeyboardMarkup) {
    let title = match wizard.kind {
        QuickKind::Expense => "Nuova uscita",
        QuickKind::Income => "Nuova entrata",
        QuickKind::Refund => "Nuovo rimborso/storno",
    };

    let default_wallet = prefs
        .default_wallet_id
        .and_then(|id| snapshot.wallets.iter().find(|w| w.id == id))
        .map(|w| w.name.as_str())
        .unwrap_or("Non impostato");

    let last_flow = prefs
        .last_flow_id
        .and_then(|id| snapshot.flows.iter().find(|f| f.id == id))
        .map(|f| flow_display_name(f.is_unallocated, &f.name))
        .unwrap_or("Non in flow");

    let category = wizard.category.as_deref().unwrap_or("-");

    let text = format!(
        "{title}\n\nWallet: {default_wallet}\nFlow: {last_flow}\nCategoria: {category}\n\nTip: puoi anche scrivere direttamente in chat (quick add)."
    );

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    rows.push(vec![
        InlineKeyboardButton::callback("✏️ Inserisci", "wiz:input"),
        InlineKeyboardButton::callback("👛 Wallet", "wiz:pick_wallet"),
        InlineKeyboardButton::callback("🎯 Flow", "wiz:pick_flow"),
    ]);

    let mut category_buttons: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    category_buttons.push(vec![
        InlineKeyboardButton::callback("🏷 Nessuna", "wiz:cat:none"),
        InlineKeyboardButton::callback("🔁 Reset", "wiz:cat:reset"),
    ]);

    let mut current_row: Vec<InlineKeyboardButton> = Vec::new();
    for (idx, cat) in wizard.categories.iter().take(6).enumerate() {
        let label = format!("🏷 {cat}");
        current_row.push(InlineKeyboardButton::callback(
            label,
            format!("wiz:cat:{idx}"),
        ));
        if current_row.len() == 2 {
            category_buttons.push(std::mem::take(&mut current_row));
        }
    }
    if !current_row.is_empty() {
        category_buttons.push(current_row);
    }
    rows.extend(category_buttons);

    if !recents.is_empty() {
        rows.push(vec![InlineKeyboardButton::callback("🕘 Recenti", "noop")]);
        for tx in recents.iter().take(6) {
            let label = tx_button_label(currency, tx);
            rows.push(vec![InlineKeyboardButton::callback(
                label,
                format!("wiz:recent:{id}", id = tx.id),
            )]);
        }
    }

    rows.push(vec![InlineKeyboardButton::callback("⬅️ Home", "wiz:close")]);

    (text, InlineKeyboardMarkup::new(rows))
}
