/// Transaction detail view rendering.
///
/// Displays:
/// - Transaction metadata (kind, date, amount, category, note, voided status)
/// - Legs breakdown (wallet/flow targets with amounts)
/// - Available actions (shown in context)

use api_types::transaction::{LegTarget, TransactionDetailResponse};
use engine::Money;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use crate::{
    app::AppState,
    text::{TextKey, t},
    ui::theme::Theme,
};

use super::common::{kind_chip, leg_amount_span, map_currency, resolve_flow_name, resolve_wallet_name};

/// Renders the transaction detail panel (right side)
pub fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(detail) = &state.transactions.detail else {
        let block = Block::default()
            .title("Transaction")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent));
        frame.render_widget(
            Paragraph::new(Line::from(t(state.locale, TextKey::UiNoDetailAvailable)))
                .block(block)
                .alignment(ratatui::layout::Alignment::Center),
            area,
        );
        return;
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    render_transaction_info(frame, layout[0], state, detail, theme);
    render_legs(frame, layout[1], state, detail, theme);
}

/// Renders the transaction metadata section
fn render_transaction_info(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    detail: &TransactionDetailResponse,
    theme: &Theme,
) {
    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(engine::Currency::Eur);

    let header = &detail.transaction;
    let occurred_at = header.occurred_at.format("%d %b %Y %H:%M").to_string();
    let amount = Money::new(header.amount_minor).format(currency);
    let category = header
        .category
        .as_deref()
        .map(|c| format!("#{c}"))
        .unwrap_or_else(|| "-".to_string());
    let note = header.note.as_deref().unwrap_or("-");
    let voided = if header.voided { "YES" } else { "NO" };

    let lines = vec![
        Line::from(vec![
            Span::styled("Kind", Style::default().fg(theme.dim)),
            Span::raw(": "),
            kind_chip(header.kind, theme),
            Span::raw("   "),
            Span::styled("Voided", Style::default().fg(theme.dim)),
            Span::raw(": "),
            Span::styled(
                voided.to_string(),
                Style::default().fg(if header.voided {
                    theme.error
                } else {
                    theme.text
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("When", Style::default().fg(theme.dim)),
            Span::raw(format!(": {occurred_at}")),
        ]),
        Line::from(vec![
            Span::styled("Amount", Style::default().fg(theme.dim)),
            Span::raw(format!(": {amount}")),
        ]),
        Line::from(vec![
            Span::styled("Category", Style::default().fg(theme.dim)),
            Span::raw(format!(": {category}")),
        ]),
        Line::from(vec![
            Span::styled("Note", Style::default().fg(theme.dim)),
            Span::raw(format!(": {note}")),
        ]),
    ];

    let header_block = Block::default()
        .title("Transaction Detail")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(header_block), area);
}

/// Renders the legs breakdown section
fn render_legs(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    detail: &TransactionDetailResponse,
    theme: &Theme,
) {
    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(engine::Currency::Eur);

    let legs = detail
        .legs
        .iter()
        .map(|leg| {
            let name = match leg.target {
                LegTarget::Wallet { wallet_id } => resolve_wallet_name(state, wallet_id),
                LegTarget::Flow { flow_id } => resolve_flow_name(state, flow_id),
            };
            let label = match leg.target {
                LegTarget::Wallet { .. } => "Wallet",
                LegTarget::Flow { .. } => "Flow",
            };
            let amount = leg_amount_span(leg.amount_minor, currency, theme);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{label:<6}"), Style::default().fg(theme.dim)),
                Span::raw(": "),
                Span::raw(name),
                Span::raw("  "),
                amount,
            ]))
        })
        .collect::<Vec<_>>();

    let legs_block = Block::default()
        .title("Legs")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let list = List::new(legs).block(legs_block);
    frame.render_widget(list, area);
}
