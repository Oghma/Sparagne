use api_types::transaction::TransactionListResponse;
use chrono::{Datelike, NaiveDate};
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    i18n::{self, TextKey},
    state::ListFilters,
};

/// Formats a date header for grouping transactions.
fn format_date_header(locale: i18n::Locale, date: NaiveDate) -> String {
    let month_names_it = [
        "Gennaio",
        "Febbraio",
        "Marzo",
        "Aprile",
        "Maggio",
        "Giugno",
        "Luglio",
        "Agosto",
        "Settembre",
        "Ottobre",
        "Novembre",
        "Dicembre",
    ];
    let month_names_en = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let day = date.day();
    let month_idx = date.month0() as usize;
    let month_name = match locale {
        i18n::Locale::It => month_names_it[month_idx],
        i18n::Locale::En => month_names_en[month_idx],
    };
    format!("📅 {day} {month_name}")
}

/// Renders the transaction list with numbered buttons for selection.
pub(crate) fn render_list(
    locale: i18n::Locale,
    currency: EngineCurrency,
    list: &TransactionListResponse,
    include_voided: bool,
    has_prev: bool,
    has_next: bool,
    page_number: usize,
    filters: &ListFilters,
) -> (String, InlineKeyboardMarkup) {
    // Breadcrumb + header
    let breadcrumb = i18n::t(locale, TextKey::NavBreadcrumbList);
    let header = i18n::t(locale, TextKey::ListHeader);
    let page_str = page_number.to_string();
    let page_indicator = i18n::format(locale, TextKey::ListPageNumber, &[("page", &page_str)]);

    let mut text = format!("{breadcrumb}\n{header}\n{page_indicator}");

    // Show active filter indicator
    if filters.is_active() {
        text.push_str(&format!("\n{}", i18n::t(locale, TextKey::FilterActiveIndicator)));
    }

    // Render transaction list grouped by date
    let mut last_date: Option<NaiveDate> = None;
    for (idx, tx) in list.transactions.iter().enumerate() {
        let tx_date = tx.occurred_at.date_naive();

        // Add date header when date changes
        if last_date != Some(tx_date) {
            text.push_str(&format!("\n\n{}", format_date_header(locale, tx_date)));
            last_date = Some(tx_date);
        }

        let amount = Money::new(tx.amount_minor).format(currency);
        let category = tx.category.as_deref().unwrap_or("");
        let note = tx.note.as_deref().unwrap_or("");
        let voided_suffix = if tx.voided {
            i18n::t(locale, TextKey::TxVoidedSuffix)
        } else {
            ""
        };

        // Format: "1. -12.50€ cibo caffè" (date is in header now)
        let mut line = format!("\n{}. {}", idx + 1, amount);
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

    // Filter and toggle voided buttons
    let filter_label = if filters.is_active() {
        format!("🔍 {} ✓", i18n::t(locale, TextKey::ListBtnFilter))
    } else {
        format!("🔍 {}", i18n::t(locale, TextKey::ListBtnFilter))
    };
    rows.push(vec![
        InlineKeyboardButton::callback(filter_label, "list:filters"),
        InlineKeyboardButton::callback(
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
        ),
    ]);

    // Home button
    rows.push(vec![InlineKeyboardButton::callback(
        format!("🏠 {}", i18n::t(locale, TextKey::ListBtnHome)),
        "nav:home",
    )]);

    (text, InlineKeyboardMarkup::new(rows))
}
