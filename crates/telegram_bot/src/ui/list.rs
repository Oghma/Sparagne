use api_types::transaction::TransactionListResponse;
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::i18n::{self, TextKey};

/// Renders the transaction list with numbered buttons for selection.
pub(crate) fn render_list(
    locale: i18n::Locale,
    currency: EngineCurrency,
    list: &TransactionListResponse,
    include_voided: bool,
    has_prev: bool,
    has_next: bool,
) -> (String, InlineKeyboardMarkup) {
    let mut text = format!("{}\n", i18n::t(locale, TextKey::ListHeader));

    // Render transaction list as text
    for (idx, tx) in list.transactions.iter().enumerate() {
        let amount = Money::new(tx.amount_minor).format(currency);
        let category = tx.category.as_deref().unwrap_or("");
        let note = tx.note.as_deref().unwrap_or("");
        let voided_suffix = if tx.voided {
            i18n::t(locale, TextKey::TxVoidedSuffix)
        } else {
            ""
        };

        // Format: "1. 15/01 -12.50€ cibo caffè"
        let date = tx.occurred_at.format("%d/%m");
        let mut line = format!("\n{}. {} {}", idx + 1, date, amount);
        if !category.is_empty() {
            line.push_str(&format!(" {category}"));
        }
        if !note.is_empty() {
            line.push_str(&format!(" {note}"));
        }
        line.push_str(voided_suffix);
        text.push_str(&line);
    }

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // Row of numbered buttons [1] [2] [3] [4] [5]
    if !list.transactions.is_empty() {
        let num_buttons: Vec<InlineKeyboardButton> = list
            .transactions
            .iter()
            .enumerate()
            .map(|(idx, _)| {
                InlineKeyboardButton::callback(
                    format!("[{}]", idx + 1),
                    format!("tx:detail:{}", idx + 1),
                )
            })
            .collect();
        rows.push(num_buttons);
    }

    // Navigation row (Prev / Next)
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

    // Toggle voided visibility
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
        "list:toggle_voided",
    )]);

    // Home button
    rows.push(vec![InlineKeyboardButton::callback(
        format!("🏠 {}", i18n::t(locale, TextKey::ListBtnHome)),
        "nav:home",
    )]);

    (text, InlineKeyboardMarkup::new(rows))
}
