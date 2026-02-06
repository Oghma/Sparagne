//! Shared "Recent Transactions" panel used by wallet and flow detail views.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use engine::{Currency, Money};

use crate::ui::{
    common::{tx_amount_color, tx_icon_color},
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
        let block = Block::default()
            .title(Span::styled(
                " Recent Transactions ",
                Style::default().fg(theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.negative));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("\u{26a0} {err}"),
                Style::default().fg(theme.negative),
            )))
            .alignment(Alignment::Center)
            .block(block),
            area,
        );
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
        let block = Block::default()
            .title(Span::styled(
                " Recent Transactions ",
                Style::default().fg(theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border));
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    empty_message.to_string(),
                    Style::default().fg(theme.text_muted),
                )),
            ])
            .alignment(Alignment::Center)
            .block(block),
            area,
        );
        return;
    }

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " Recent Transactions ",
                Style::default().fg(theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border)),
    );
    frame.render_widget(list, area);
}
