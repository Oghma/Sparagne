//! Shared "Recent Transactions" panel used by wallet and flow detail views.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem},
};

use engine::{Currency, Money};

use crate::ui::{
    common::{
        render_empty_state, render_error_state, themed_block, tx_amount_color, tx_icon_color,
    },
    theme::Theme,
};

/// Renders a "Recent Transactions" panel inside the given `area`.
///
/// Handles three cases:
/// 1. An error message (red border with warning icon)
/// 2. An empty list (centered placeholder text)
/// 3. A populated list of transactions with date, icon, amount, and note
pub(crate) fn render_recent_transactions(
    frame: &mut Frame<'_>,
    area: Rect,
    transactions: &[api_types::transaction::TransactionView],
    error: Option<&str>,
    empty_message: &str,
    currency: Currency,
    theme: &Theme,
) {
    if let Some(err) = error {
        render_error_state(frame, area, "Recent Transactions", err, theme);
        return;
    }

    let items = transactions
        .iter()
        .map(|tx| {
            let when = tx.occurred_at.format("%d %b %H:%M").to_string();
            let note = tx.note.as_deref().unwrap_or("-");

            let (icon, icon_color) = tx_icon_color(tx.kind, theme);
            let amount_color = tx_amount_color(tx.kind, theme);

            let line = Line::from(vec![
                Span::raw("  "),
                Span::styled(when, Style::default().fg(theme.text_muted)),
                Span::raw("  "),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(
                    format!("{:>10}", Money::new(tx.amount_minor).format(currency)),
                    Style::default().fg(amount_color),
                ),
                Span::raw("  "),
                Span::styled(note, Style::default().fg(theme.text)),
            ]);
            ListItem::new(line)
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        render_empty_state(frame, area, "Recent Transactions", empty_message, theme);
        return;
    }

    let list = List::new(items).block(themed_block("Recent Transactions", theme.border, theme));
    frame.render_widget(list, area);
}
