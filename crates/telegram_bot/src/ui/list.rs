use api_types::transaction::TransactionListResponse;
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    i18n::{self, TextKey},
    ui::shared::tx_button_label,
};

pub(crate) fn render_list(
    locale: i18n::Locale,
    currency: EngineCurrency,
    list: &TransactionListResponse,
    include_voided: bool,
    has_prev: bool,
    has_next: bool,
) -> (String, InlineKeyboardMarkup) {
    let mut text = format!("{}\n", i18n::t(locale, TextKey::ListHeader));
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
            if tx.voided {
                i18n::t(locale, TextKey::TxVoidedSuffix)
            } else {
                ""
            }
        ));
    }

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for tx in &list.transactions {
        rows.push(vec![InlineKeyboardButton::callback(
            tx_button_label(locale, currency, tx),
            format!("tx:detail:{id}", id = tx.id),
        )]);
    }

    let mut nav_row: Vec<InlineKeyboardButton> = Vec::new();
    if has_prev {
        nav_row.push(InlineKeyboardButton::callback(
            format!("⬅️ {}", i18n::t(locale, TextKey::ListPrev)),
            "list:prev",
        ));
    }
    if has_next {
        nav_row.push(InlineKeyboardButton::callback(
            format!("{} ➡️", i18n::t(locale, TextKey::ListNext)),
            "list:next",
        ));
    }
    if !nav_row.is_empty() {
        rows.push(nav_row);
    }

    rows.push(vec![InlineKeyboardButton::callback(
        i18n::format(
            locale,
            TextKey::ListToggleVoided,
            &[(
                "state",
                if include_voided {
                    i18n::t(locale, TextKey::ListStateOn)
                } else {
                    i18n::t(locale, TextKey::ListStateOff)
                },
            )],
        ),
        "prefs:toggle_voided",
    )]);
    rows.push(vec![InlineKeyboardButton::callback(
        format!("⬅️ {}", i18n::t(locale, TextKey::ListBtnHome)),
        "nav:home",
    )]);

    (text, InlineKeyboardMarkup::new(rows))
}
