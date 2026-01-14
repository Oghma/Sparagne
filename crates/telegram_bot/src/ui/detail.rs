use api_types::transaction::TransactionDetailResponse;
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use uuid::Uuid;

use crate::i18n::{self, TextKey};

pub(crate) fn render_detail(
    currency: EngineCurrency,
    detail: &TransactionDetailResponse,
) -> (String, InlineKeyboardMarkup) {
    let tx = &detail.transaction;
    let locale = i18n::default_locale();
    let text = i18n::format(
        locale,
        TextKey::DetailHeader,
        &[
            ("kind", &format!("{:?}", tx.kind)),
            ("when", &format!("{}", tx.occurred_at)),
            ("amount", &Money::new(tx.amount_minor).format(currency)),
            ("category", tx.category.as_deref().unwrap_or("-")),
            ("note", tx.note.as_deref().unwrap_or("-")),
            (
                "voided",
                if tx.voided {
                    i18n::t(locale, TextKey::DetailYes)
                } else {
                    i18n::t(locale, TextKey::DetailNo)
                },
            ),
        ],
    );

    let mut text = text;
    text.push_str(&format!("\n\n{}:", i18n::t(locale, TextKey::DetailLegs)));
    for leg in &detail.legs {
        text.push_str(&format!(
            "\n- {:?}: {}",
            leg.target,
            Money::new(leg.amount_minor).format(currency)
        ));
    }

    let kb = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                format!("↩ {}", i18n::t(locale, TextKey::DetailBtnVoid)),
                format!("tx:void:{id}", id = tx.id),
            ),
            InlineKeyboardButton::callback(
                format!("✏️ {}", i18n::t(locale, TextKey::DetailBtnEdit)),
                format!("tx:edit:{id}", id = tx.id),
            ),
            InlineKeyboardButton::callback(
                format!("📌 {}", i18n::t(locale, TextKey::DetailBtnRepeat)),
                format!("tx:repeat:{id}", id = tx.id),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            format!("⬅️ {}", i18n::t(locale, TextKey::DetailBtnBack)),
            "nav:list",
        )],
    ]);

    (text, kb)
}

pub(crate) fn render_edit_menu(tx_id: Uuid) -> (String, InlineKeyboardMarkup) {
    let locale = i18n::default_locale();
    (
        i18n::t(locale, TextKey::EditMenuTitle).to_string(),
        InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::callback(
                    format!("💶 {}", i18n::t(locale, TextKey::EditMenuAmount)),
                    format!("tx:edit_amount:{tx_id}"),
                ),
                InlineKeyboardButton::callback(
                    format!("📝 {}", i18n::t(locale, TextKey::EditMenuNote)),
                    format!("tx:edit_note:{tx_id}"),
                ),
            ],
            vec![InlineKeyboardButton::callback(
                format!("⬅️ {}", i18n::t(locale, TextKey::DetailBtnBack)),
                format!("tx:detail:{tx_id}"),
            )],
        ]),
    )
}
