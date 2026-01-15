use api_types::{transaction::TransactionView, vault::VaultSnapshot};
use engine::Currency as EngineCurrency;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    i18n::{self, TextKey},
    parsing::QuickKind,
    state::{UserPrefs, WizardSession},
    ui::shared::flow_display_name,
};

/// Renders a simplified wizard focused on quick input.
/// Categories should be added via hashtag inline (e.g., "12.50 #food caffè").
/// Recents are shown only if explicitly requested via `show_recents` flag.
pub(crate) fn render_wizard(
    locale: i18n::Locale,
    _currency: EngineCurrency,
    snapshot: &VaultSnapshot,
    prefs: &UserPrefs,
    wizard: &WizardSession,
    _recents: &[TransactionView],
) -> (String, InlineKeyboardMarkup) {
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
        .map(|f| flow_display_name(locale, f.is_unallocated, &f.name))
        .unwrap_or(i18n::t(locale, TextKey::UnallocatedFlow));

    // Simplified body without category (use hashtags inline)
    let body = i18n::format(
        locale,
        TextKey::WizardBodySimple,
        &[("wallet", default_wallet), ("flow", last_flow)],
    );
    let text = format!("{title}\n\n{body}");

    // Simplified button layout:
    // Row 1: Main input action (prominent)
    // Row 2: Wallet and Flow settings
    // Row 3: Back to home
    let rows = vec![
        vec![InlineKeyboardButton::callback(
            format!("✏️ {}", i18n::t(locale, TextKey::WizardBtnInput)),
            "wiz:input",
        )],
        vec![
            InlineKeyboardButton::callback(
                format!("👛 {}", i18n::t(locale, TextKey::WizardBtnWallet)),
                "wiz:pick_wallet",
            ),
            InlineKeyboardButton::callback(
                format!("🎯 {}", i18n::t(locale, TextKey::WizardBtnFlow)),
                "wiz:pick_flow",
            ),
        ],
        vec![InlineKeyboardButton::callback(
            format!("⬅️ {}", i18n::t(locale, TextKey::WizardBtnHome)),
            "wiz:close",
        )],
    ];

    (text, InlineKeyboardMarkup::new(rows))
}
