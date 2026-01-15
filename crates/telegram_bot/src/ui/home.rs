use api_types::vault::VaultSnapshot;
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
    let default_wallet = prefs
        .default_wallet_id
        .and_then(|id| snapshot.wallets.iter().find(|w| w.id == id))
        .map(|w| w.name.as_str())
        .unwrap_or(i18n::t(locale, TextKey::UnsetValue));

    // Use last_flow if set, otherwise fall back to default_flow
    let current_flow = prefs
        .last_flow_id
        .or(prefs.default_flow_id)
        .and_then(|id| snapshot.flows.iter().find(|f| f.id == id))
        .map(|f| flow_display_name(locale, f.is_unallocated, &f.name))
        .unwrap_or(i18n::t(locale, TextKey::UnallocatedFlow));

    let text = i18n::format(
        locale,
        TextKey::HomeSummary,
        &[
            ("display_name", display_name),
            ("vault", snapshot.name.as_str()),
            ("wallet", default_wallet),
            ("flow", current_flow),
        ],
    );

    let kb = InlineKeyboardMarkup::new(vec![
        // Primary actions: expense and income
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
        // Secondary actions: history and stats
        vec![
            InlineKeyboardButton::callback(
                format!("🧾 {}", i18n::t(locale, TextKey::HomeBtnList)),
                "home:list",
            ),
            InlineKeyboardButton::callback(
                format!("📊 {}", i18n::t(locale, TextKey::HomeBtnStats)),
                "home:stats",
            ),
        ],
        // Utility: settings and commands
        vec![
            InlineKeyboardButton::callback(
                format!("⚙️ {}", i18n::t(locale, TextKey::HomeBtnSettings)),
                "nav:settings",
            ),
            InlineKeyboardButton::callback(
                format!("📋 {}", i18n::t(locale, TextKey::HomeBtnCommands)),
                "nav:commands",
            ),
        ],
    ]);

    (text, kb)
}

pub(crate) fn render_settings(locale: i18n::Locale) -> (String, InlineKeyboardMarkup) {
    let text = i18n::t(locale, TextKey::SettingsTitle).to_string();

    let kb = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            format!("👛 {}", i18n::t(locale, TextKey::SettingsBtnWallet)),
            "home:pick_wallet",
        )],
        vec![InlineKeyboardButton::callback(
            format!("🎯 {}", i18n::t(locale, TextKey::SettingsBtnFlow)),
            "home:pick_flow",
        )],
        vec![InlineKeyboardButton::callback(
            format!("⬅️ {}", i18n::t(locale, TextKey::DetailBtnBack)),
            "nav:home",
        )],
    ]);

    (text, kb)
}

pub(crate) fn render_commands(locale: i18n::Locale) -> (String, InlineKeyboardMarkup) {
    let title = i18n::t(locale, TextKey::CommandsTitle);
    let body = i18n::t(locale, TextKey::CommandsBody);
    let text = format!("{title}\n\n{body}");

    let kb = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        format!("⬅️ {}", i18n::t(locale, TextKey::DetailBtnBack)),
        "nav:home",
    )]]);

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
