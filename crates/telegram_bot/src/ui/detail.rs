use api_types::transaction::TransactionDetailResponse;
use engine::{Currency as EngineCurrency, Money};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use uuid::Uuid;

pub(crate) fn render_detail(
    currency: EngineCurrency,
    detail: &TransactionDetailResponse,
) -> (String, InlineKeyboardMarkup) {
    let tx = &detail.transaction;
    let mut text = format!(
        "Dettaglio\n\nKind: {:?}\nQuando: {}\nImporto: {}\nCategoria: {}\nNota: {}\nVoided: {}",
        tx.kind,
        tx.occurred_at,
        Money::new(tx.amount_minor).format(currency),
        tx.category.as_deref().unwrap_or("-"),
        tx.note.as_deref().unwrap_or("-"),
        if tx.voided { "si" } else { "no" }
    );

    text.push_str("\n\nLegs:");
    for leg in &detail.legs {
        text.push_str(&format!(
            "\n- {:?}: {}",
            leg.target,
            Money::new(leg.amount_minor).format(currency)
        ));
    }

    let kb = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("↩ Void", format!("tx:void:{id}", id = tx.id)),
            InlineKeyboardButton::callback("✏️ Edit", format!("tx:edit:{id}", id = tx.id)),
            InlineKeyboardButton::callback("📌 Ripeti", format!("tx:repeat:{id}", id = tx.id)),
        ],
        vec![InlineKeyboardButton::callback("⬅️ Indietro", "nav:list")],
    ]);

    (text, kb)
}

pub(crate) fn render_edit_menu(tx_id: Uuid) -> (String, InlineKeyboardMarkup) {
    (
        "Cosa vuoi modificare?".to_string(),
        InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::callback("💶 Importo", format!("tx:edit_amount:{tx_id}")),
                InlineKeyboardButton::callback("📝 Nota", format!("tx:edit_note:{tx_id}")),
            ],
            vec![InlineKeyboardButton::callback(
                "⬅️ Indietro",
                format!("tx:detail:{tx_id}"),
            )],
        ]),
    )
}
