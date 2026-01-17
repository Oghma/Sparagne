use api_types::transaction::TransactionKind;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    i18n::{self, TextKey},
    state::ListFilters,
};

/// Renders the filter menu.
pub(crate) fn render_filter_menu(
    locale: i18n::Locale,
    filters: &ListFilters,
) -> (String, InlineKeyboardMarkup) {
    let title = i18n::t(locale, TextKey::FilterTitle);

    // Show current filter state
    let mut text = title.to_string();
    if filters.is_active() {
        text.push_str(&format!("\n\n{}", i18n::t(locale, TextKey::FilterActiveIndicator)));
        if let Some(kind) = &filters.kind {
            let kind_str = match kind {
                TransactionKind::Expense => i18n::t(locale, TextKey::FilterKindExpense),
                TransactionKind::Income => i18n::t(locale, TextKey::FilterKindIncome),
                _ => "",
            };
            if !kind_str.is_empty() {
                text.push_str(&format!(": {kind_str}"));
            }
        }
    }

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // Kind filter buttons
    let kind_all_label = format_kind_label(locale, None, filters.kind.as_ref());
    let kind_expense_label =
        format_kind_label(locale, Some(TransactionKind::Expense), filters.kind.as_ref());
    let kind_income_label =
        format_kind_label(locale, Some(TransactionKind::Income), filters.kind.as_ref());

    rows.push(vec![InlineKeyboardButton::callback(
        kind_all_label,
        "list:filter:kind:all",
    )]);
    rows.push(vec![
        InlineKeyboardButton::callback(kind_expense_label, "list:filter:kind:expense"),
        InlineKeyboardButton::callback(kind_income_label, "list:filter:kind:income"),
    ]);

    // Clear filters button (only if filters are active)
    if filters.is_active() {
        rows.push(vec![InlineKeyboardButton::callback(
            format!("❌ {}", i18n::t(locale, TextKey::FilterClear)),
            "list:filter:clear",
        )]);
    }

    // Back button
    rows.push(vec![InlineKeyboardButton::callback(
        format!("⬅️ {}", i18n::t(locale, TextKey::FilterBtnBack)),
        "nav:list",
    )]);

    (text, InlineKeyboardMarkup::new(rows))
}

/// Formats a kind filter button label with checkmark if selected.
fn format_kind_label(
    locale: i18n::Locale,
    button_kind: Option<TransactionKind>,
    current_kind: Option<&TransactionKind>,
) -> String {
    let label = match button_kind {
        None => i18n::t(locale, TextKey::FilterKindAll),
        Some(TransactionKind::Expense) => i18n::t(locale, TextKey::FilterKindExpense),
        Some(TransactionKind::Income) => i18n::t(locale, TextKey::FilterKindIncome),
        Some(_) => "",
    };

    let is_selected = match (button_kind, current_kind) {
        (None, None) => true,
        (Some(b), Some(c)) => std::mem::discriminant(&b) == std::mem::discriminant(c),
        _ => false,
    };

    if is_selected {
        format!("✓ {label}")
    } else {
        label.to_string()
    }
}
