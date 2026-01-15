use api_types::transaction::{TransactionDetailResponse, TransactionKind};
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use uuid::Uuid;

use crate::i18n::{self, TextKey};

/// Returns the localized name for a transaction kind.
fn localized_kind(locale: i18n::Locale, kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Expense => i18n::t(locale, TextKey::TxKindExpense),
        TransactionKind::Income => i18n::t(locale, TextKey::TxKindIncome),
        TransactionKind::Refund => i18n::t(locale, TextKey::TxKindRefund),
        TransactionKind::TransferWallet => i18n::t(locale, TextKey::TxKindTransferWallet),
        TransactionKind::TransferFlow => i18n::t(locale, TextKey::TxKindTransferFlow),
    }
}

/// Formats a datetime in a user-friendly format (date only).
fn format_date(dt: &chrono::DateTime<chrono::FixedOffset>) -> String {
    dt.format("%d/%m/%Y").to_string()
}

pub(crate) fn render_detail(
    locale: i18n::Locale,
    currency: EngineCurrency,
    detail: &TransactionDetailResponse,
) -> (String, InlineKeyboardMarkup) {
    let tx = &detail.transaction;

    // Localize kind and format date nicely
    let kind_str = localized_kind(locale, tx.kind);
    let date_str = format_date(&tx.occurred_at);

    let text = i18n::format(
        locale,
        TextKey::DetailHeader,
        &[
            ("kind", kind_str),
            ("when", &date_str),
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

    // Note: Legs section removed as it's technical detail not useful for users

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

pub(crate) fn render_edit_menu(
    locale: i18n::Locale,
    tx_id: Uuid,
) -> (String, InlineKeyboardMarkup) {
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
