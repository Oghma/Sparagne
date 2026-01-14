use api_types::transaction::TransactionListResponse;
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::ui::shared::tx_button_label;

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
