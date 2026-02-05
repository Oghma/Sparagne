//! Wallet detail panel rendering.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use api_types::transaction::TransactionKind;
use engine::{Currency, Money};

use crate::{app::AppState, ui::theme::Theme};

use super::common::{map_currency, ICON_EXPENSE, ICON_INCOME, ICON_REFUND, ICON_TRANSFER};

/// Renders the wallet detail panel.
pub fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty(frame, area, theme, "Loading...");
        return;
    };
    let Some(detail_id) = state.wallets.detail.wallet_id else {
        render_empty(frame, area, theme, "Select a wallet to view details");
        return;
    };
    let Some(wallet) = snapshot
        .wallets
        .iter()
        .find(|wallet| wallet.id == detail_id)
    else {
        render_empty(frame, area, theme, "Wallet not found");
        return;
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let balance_color = if wallet.balance_minor >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    let status = if wallet.archived {
        Span::styled("[archived]", Style::default().fg(theme.warning))
    } else {
        Span::styled("[active]", Style::default().fg(theme.positive))
    };

    let header_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  💰 ", Style::default()),
            Span::styled(
                &wallet.name,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            status,
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Balance: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                Money::new(wallet.balance_minor).format(currency),
                Style::default()
                    .fg(balance_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    let header_block = Block::default()
        .title(Span::styled(
            " Wallet Detail ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(header_lines).block(header_block), layout[0]);

    // Recent transactions
    render_transactions(frame, layout[1], state, theme, currency);
}

fn render_transactions(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    currency: Currency,
) {
    if let Some(err) = state.wallets.detail.error.as_ref() {
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
                format!("⚠ {err}"),
                Style::default().fg(theme.negative),
            )))
            .alignment(Alignment::Center)
            .block(block),
            area,
        );
        return;
    }

    let items = state
        .wallets
        .detail
        .transactions
        .iter()
        .map(|tx| {
            let when = tx.occurred_at.format("%d %b %H:%M").to_string();
            let note = tx.note.as_deref().unwrap_or("-");

            let (icon, icon_color) = match tx.kind {
                TransactionKind::Income => (ICON_INCOME, theme.income),
                TransactionKind::Expense => (ICON_EXPENSE, theme.expense),
                TransactionKind::Refund => (ICON_REFUND, theme.refund),
                TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                    (ICON_TRANSFER, theme.transfer)
                }
            };

            let amount_color = match tx.kind {
                TransactionKind::Income | TransactionKind::Refund => theme.positive,
                TransactionKind::Expense => theme.negative,
                _ => theme.text,
            };

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
                    "No transactions for this wallet",
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

fn render_empty(frame: &mut Frame<'_>, area: Rect, theme: &Theme, message: &str) {
    let block = Block::default()
        .title(Span::styled(
            " Wallet Detail ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(message, Style::default().fg(theme.text_muted))),
        ])
        .alignment(Alignment::Center)
        .block(block),
        area,
    );
}
