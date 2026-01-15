use api_types::vault::VaultSnapshot;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    i18n::{self, TextKey},
    parsing::QuickKind,
    state::{UserPrefs, WizardSession},
    ui::shared::flow_display_name,
};

/// Renders the wizard screen for guided transaction entry.
pub(crate) fn render_wizard(
    locale: i18n::Locale,
    snapshot: &VaultSnapshot,
    prefs: &UserPrefs,
    wizard: &WizardSession,
) -> (String, InlineKeyboardMarkup) {
    let title = match wizard.kind {
        QuickKind::Expense => i18n::t(locale, TextKey::WizardTitleExpense),
        QuickKind::Income => i18n::t(locale, TextKey::WizardTitleIncome),
    };

    let default_wallet = prefs
        .default_wallet_id
        .and_then(|id| snapshot.wallets.iter().find(|w| w.id == id))
        .map(|w| w.name.as_str())
        .unwrap_or(i18n::t(locale, TextKey::UnsetValue));

    let last_flow = prefs
        .last_flow_id
        .and_then(|id| snapshot.flows.iter().find(|f| f.id == id))
        .map(|f| flow_display_name(locale, f.is_unallocated, &f.name))
        .unwrap_or(i18n::t(locale, TextKey::UnallocatedFlow));

    let body = i18n::format(
        locale,
        TextKey::WizardBodySimple,
        &[("wallet", default_wallet), ("flow", last_flow)],
    );
    let text = format!("{title}\n\n{body}");

    let rows = vec![
        // Row 1: Main input action
        vec![InlineKeyboardButton::callback(
            format!("✏️ {}", i18n::t(locale, TextKey::WizardBtnInput)),
            "wiz:input",
        )],
        // Row 2: Wallet and Budget pickers
        vec![
            InlineKeyboardButton::callback(
                format!("👛 {}", i18n::t(locale, TextKey::WizardBtnWallet)),
                "wiz:wallet",
            ),
            InlineKeyboardButton::callback(
                format!("🎯 {}", i18n::t(locale, TextKey::WizardBtnFlow)),
                "wiz:flow",
            ),
        ],
        // Row 3: Back to home
        vec![InlineKeyboardButton::callback(
            format!("🏠 {}", i18n::t(locale, TextKey::WizardBtnHome)),
            "wiz:cancel",
        )],
    ];

    (text, InlineKeyboardMarkup::new(rows))
}
