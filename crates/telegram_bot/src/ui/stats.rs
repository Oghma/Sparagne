use api_types::stats::Statistic;
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

pub(crate) fn render_stats(
    currency: EngineCurrency,
    stats: &Statistic,
) -> (String, InlineKeyboardMarkup) {
    let text = format!(
        "Stats\n\nBilancio: {}\nTotale entrate: {}\nTotale uscite: {}",
        Money::new(stats.balance_minor).format(currency),
        Money::new(stats.total_income_minor).format(currency),
        Money::new(stats.total_expenses_minor).format(currency),
    );
    let kb = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "⬅️ Home",
        "nav:home",
    )]]);
    (text, kb)
}
