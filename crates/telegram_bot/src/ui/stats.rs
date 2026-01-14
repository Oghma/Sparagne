use api_types::stats::Statistic;
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::i18n::{self, TextKey};

pub(crate) fn render_stats(
    locale: i18n::Locale,
    currency: EngineCurrency,
    stats: &Statistic,
) -> (String, InlineKeyboardMarkup) {
    let text = i18n::format(
        locale,
        TextKey::StatsSummary,
        &[
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
    let kb = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        format!("⬅️ {}", i18n::t(locale, TextKey::StatsBtnHome)),
        "nav:home",
    )]]);
    (text, kb)
}
