use api_types::stats::Statistic;
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::i18n::{self, TextKey};

/// Renders statistics with optional category breakdown.
pub(crate) fn render_stats(
    locale: i18n::Locale,
    currency: EngineCurrency,
    stats: &Statistic,
    month_name: &str,
    category_breakdown: &[(String, i64)], // (category_name, amount_minor)
) -> (String, InlineKeyboardMarkup) {
    let breadcrumb = i18n::t(locale, TextKey::NavBreadcrumbStats);
    let summary = i18n::format(
        locale,
        TextKey::StatsSummary,
        &[
            ("month", month_name),
            ("balance", &Money::new(stats.balance_minor).format(currency)),
            (
                "income",
                &Money::new(stats.total_income_minor).format(currency),
            ),
            (
                "expenses",
                &Money::new(stats.total_expenses_minor).format(currency),
            ),
        ],
    );
    let mut text = format!("{breadcrumb}\n\n{summary}");

    // Add category breakdown if available
    if !category_breakdown.is_empty() {
        text.push_str(i18n::t(locale, TextKey::StatsCategoryHeader));
        for (category, amount) in category_breakdown {
            let formatted = Money::new(*amount).format(currency);
            text.push_str(&format!("\n  {category}: {formatted}"));
        }
    } else if stats.total_expenses_minor == 0 && stats.total_income_minor == 0 {
        text.push_str(&format!("\n\n{}", i18n::t(locale, TextKey::StatsNoData)));
    }

    let kb = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        format!("🏠 {}", i18n::t(locale, TextKey::StatsBtnHome)),
        "nav:home",
    )]]);

    (text, kb)
}
