use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::i18n::{self, TextKey};

pub(crate) fn render_categories(
    locale: i18n::Locale,
    categories: &[api_types::category::CategoryView],
) -> (String, InlineKeyboardMarkup) {
    let mut text = format!("{}\n\n", i18n::t(locale, TextKey::CategoryListHeader));

    if categories.is_empty() {
        text.push_str(i18n::t(locale, TextKey::CategoryListEmpty));
    } else {
        for cat in categories {
            text.push_str(&format!("  - {}\n", cat.name));
        }
    }

    let kb = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        i18n::t(locale, TextKey::ListBtnHome),
        "nav:home",
    )]]);

    (text, kb)
}
