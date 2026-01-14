use api_types::{transaction::TransactionView, vault::VaultSnapshot};
use engine::Currency as EngineCurrency;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    i18n::{self, TextKey},
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
    let locale = i18n::default_locale();
    let title = match wizard.kind {
        QuickKind::Expense => i18n::t(locale, TextKey::WizardTitleExpense),
        QuickKind::Income => i18n::t(locale, TextKey::WizardTitleIncome),
        QuickKind::Refund => i18n::t(locale, TextKey::WizardTitleRefund),
    };

    let default_wallet = prefs
        .default_wallet_id
        .and_then(|id| snapshot.wallets.iter().find(|w| w.id == id))
        .map(|w| w.name.as_str())
        .unwrap_or(i18n::t(locale, TextKey::UnsetValue));

    let last_flow = prefs
        .last_flow_id
        .and_then(|id| snapshot.flows.iter().find(|f| f.id == id))
        .map(|f| flow_display_name(f.is_unallocated, &f.name))
        .unwrap_or(i18n::t(locale, TextKey::UnallocatedFlow));

    let category = wizard.category.as_deref().unwrap_or("-");

    let body = i18n::format(
        locale,
        TextKey::WizardBody,
        &[
            ("wallet", default_wallet),
            ("flow", last_flow),
            ("category", category),
        ],
    );
    let text = format!("{title}\n\n{body}");

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    rows.push(vec![
        InlineKeyboardButton::callback(
            format!("✏️ {}", i18n::t(locale, TextKey::WizardBtnInput)),
            "wiz:input",
        ),
        InlineKeyboardButton::callback(
            format!("👛 {}", i18n::t(locale, TextKey::WizardBtnWallet)),
            "wiz:pick_wallet",
        ),
        InlineKeyboardButton::callback(
            format!("🎯 {}", i18n::t(locale, TextKey::WizardBtnFlow)),
            "wiz:pick_flow",
        ),
    ]);

    let mut category_buttons: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    category_buttons.push(vec![
        InlineKeyboardButton::callback(
            format!("🏷 {}", i18n::t(locale, TextKey::WizardBtnCategoryNone)),
            "wiz:cat:none",
        ),
        InlineKeyboardButton::callback(
            format!("🔁 {}", i18n::t(locale, TextKey::WizardBtnCategoryReset)),
            "wiz:cat:reset",
        ),
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
        rows.push(vec![InlineKeyboardButton::callback(
            format!("🕘 {}", i18n::t(locale, TextKey::WizardBtnRecents)),
            "noop",
        )]);
        for tx in recents.iter().take(6) {
            let label = tx_button_label(currency, tx);
            rows.push(vec![InlineKeyboardButton::callback(
                label,
                format!("wiz:recent:{id}", id = tx.id),
            )]);
        }
    }

    rows.push(vec![InlineKeyboardButton::callback(
        format!("⬅️ {}", i18n::t(locale, TextKey::WizardBtnHome)),
        "wiz:close",
    )]);

    (text, InlineKeyboardMarkup::new(rows))
}
