use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::{
    i18n::{self, TextKey},
    parsing::QuickKind,
    state::TransactionTemplate,
};

/// Renders the template list.
pub(crate) fn render_template_list(
    locale: i18n::Locale,
    currency: EngineCurrency,
    templates: &[TransactionTemplate],
) -> (String, InlineKeyboardMarkup) {
    let title = i18n::t(locale, TextKey::TemplateListTitle);

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    if templates.is_empty() {
        let text = format!("{title}\n\n{}", i18n::t(locale, TextKey::TemplateEmpty));

        // Create button
        rows.push(vec![InlineKeyboardButton::callback(
            format!("➕ {}", i18n::t(locale, TextKey::TemplateBtnCreate)),
            "tpl:create",
        )]);

        // Home button
        rows.push(vec![InlineKeyboardButton::callback(
            format!("🏠 {}", i18n::t(locale, TextKey::TemplateBtnHome)),
            "nav:home",
        )]);

        return (text, InlineKeyboardMarkup::new(rows));
    }

    // Build text with numbered list
    let mut text = format!("{title}\n");

    for (idx, tpl) in templates.iter().enumerate() {
        let sign = match tpl.kind {
            QuickKind::Expense => "-",
            QuickKind::Income => "+",
        };
        let amount = Money::new(tpl.amount_minor).format(currency);

        let mut line = format!("\n{}. {} {}{}", idx + 1, tpl.name, sign, amount);
        if let Some(cat) = &tpl.category {
            line.push_str(&format!(" #{cat}"));
        }
        if let Some(note) = &tpl.note {
            line.push_str(&format!(" {note}"));
        }
        text.push_str(&line);
    }

    // For each template, create use/delete buttons
    for (idx, tpl) in templates.iter().enumerate() {
        let use_label = format!("[{}] {}", idx + 1, i18n::t(locale, TextKey::TemplateBtnUse));
        let delete_label = format!("🗑️ {}", tpl.name);

        rows.push(vec![
            InlineKeyboardButton::callback(use_label, format!("tpl:use:{}", idx)),
            InlineKeyboardButton::callback(delete_label, format!("tpl:delete:{}", idx)),
        ]);
    }

    // Create button (if not at max)
    rows.push(vec![InlineKeyboardButton::callback(
        format!("➕ {}", i18n::t(locale, TextKey::TemplateBtnCreate)),
        "tpl:create",
    )]);

    // Home button
    rows.push(vec![InlineKeyboardButton::callback(
        format!("🏠 {}", i18n::t(locale, TextKey::TemplateBtnHome)),
        "nav:home",
    )]);

    (text, InlineKeyboardMarkup::new(rows))
}
