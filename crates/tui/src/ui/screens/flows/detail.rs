//! Flow detail panel rendering (right side).

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use api_types::transaction::TransactionKind;
use engine::{Currency, Money};

use crate::{
    app::AppState,
    ui::{
        common::{ICON_EXPENSE, ICON_INCOME, ICON_REFUND, ICON_TRANSFER, map_currency},
        components::money::{flow_cap_line_gauge, styled_amount_no_sign, styled_progress_bar},
        theme::Theme,
    },
};

/// Render the flow detail panel.
pub fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty(frame, area, theme, "Loading...");
        return;
    };
    let Some(detail_id) = state.flows.detail.flow_id else {
        render_empty(frame, area, theme, "Select a flow to view details");
        return;
    };
    let Some(flow) = snapshot.flows.iter().find(|flow| flow.id == detail_id) else {
        render_empty(frame, area, theme, "Flow not found");
        return;
    };

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let cap_line = state
        .flows
        .detail
        .detail
        .as_ref()
        .and_then(|detail| cap_progress_line(detail, currency, theme));
    let cap_gauge = state
        .flows
        .detail
        .detail
        .as_ref()
        .and_then(|detail| cap_line_gauge(detail, theme));
    let header_height = if cap_line.is_some() || cap_gauge.is_some() {
        8
    } else {
        7
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(header_height), Constraint::Min(0)])
        .split(area);

    let balance_color = if flow.balance_minor >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    let emoji = if flow.is_unallocated { "📦" } else { "🎯" };

    let mut status_spans = vec![];
    if flow.is_unallocated {
        status_spans.push(Span::styled("[default]", Style::default().fg(theme.info)));
        status_spans.push(Span::raw("  "));
    }
    if flow.archived {
        status_spans.push(Span::styled(
            "[archived]",
            Style::default().fg(theme.warning),
        ));
    } else {
        status_spans.push(Span::styled(
            "[active]",
            Style::default().fg(theme.positive),
        ));
    }

    let mut header_lines = vec![
        Line::from(""),
        Line::from(
            vec![
                Span::raw(format!("  {emoji} ")),
                Span::styled(
                    &flow.name,
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
            ]
            .into_iter()
            .chain(status_spans)
            .collect::<Vec<_>>(),
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Balance: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                Money::new(flow.balance_minor).format(currency),
                Style::default()
                    .fg(balance_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    if let Some(line) = cap_line {
        header_lines.push(line);
    }

    let header_block = Block::default()
        .title(Span::styled(
            " Flow Detail ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let header_inner = header_block.inner(layout[0]);
    frame.render_widget(header_block, layout[0]);

    if let Some(gauge) = cap_gauge {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(header_inner);
        frame.render_widget(Paragraph::new(header_lines), split[0]);
        frame.render_widget(gauge, split[1]);
    } else {
        frame.render_widget(Paragraph::new(header_lines), header_inner);
    }

    // Recent transactions
    if let Some(err) = state.flows.detail.error.as_ref() {
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
            layout[1],
        );
        return;
    }

    let items = state
        .flows
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
                    "No transactions for this flow",
                    Style::default().fg(theme.text_muted),
                )),
            ])
            .alignment(Alignment::Center)
            .block(block),
            layout[1],
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
    frame.render_widget(list, layout[1]);
}

/// Render an empty detail panel with a message.
fn render_empty(frame: &mut Frame<'_>, area: Rect, theme: &Theme, message: &str) {
    let block = Block::default()
        .title(Span::styled(
            " Flow Detail ",
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

/// Create a cap progress line showing current vs cap.
fn cap_progress_line(
    detail: &engine::CashFlow,
    currency: Currency,
    theme: &Theme,
) -> Option<Line<'static>> {
    let cap = detail.max_balance?;
    if cap <= 0 {
        return None;
    }

    let (label, current) = if let Some(income_total_minor) = detail.income_balance {
        ("Income cap", income_total_minor)
    } else {
        ("Net cap", detail.balance)
    };

    let current = current.max(0);
    let bar = styled_progress_bar(current, Some(cap), 20, theme);
    let current_fmt = styled_amount_no_sign(current, currency, theme);
    let cap_fmt = styled_amount_no_sign(cap, currency, theme);

    Some(Line::from(vec![
        Span::styled(format!("  {label}"), Style::default().fg(theme.text_muted)),
        Span::raw(": "),
        current_fmt,
        Span::raw(" / "),
        cap_fmt,
        Span::raw(" "),
        bar,
    ]))
}

/// Create a line gauge widget for cap progress.
fn cap_line_gauge(
    detail: &engine::CashFlow,
    theme: &Theme,
) -> Option<ratatui::widgets::LineGauge<'static>> {
    let cap = detail.max_balance?;
    if cap <= 0 {
        return None;
    }
    let current = if let Some(income_total_minor) = detail.income_balance {
        income_total_minor
    } else {
        detail.balance
    };
    flow_cap_line_gauge(current.max(0), Some(cap), theme)
}
