//! Transaction detail view rendering.
//!
//! Displays:
//! - Transaction metadata (kind, date, amount, category, note, voided status)
//! - Legs breakdown (wallet/flow targets with amounts)
//! - Available actions (shown in context)

use api_types::transaction::{LegTarget, TransactionDetailResponse};
use engine::Money;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

use crate::{
    app::AppState,
    text::{TextKey, t},
    ui::{common::themed_block, theme::Theme},
};

use super::common::{kind_chip, leg_amount_span, resolve_flow_name, resolve_wallet_name};
use crate::ui::common::get_currency;

/// Renders the transaction detail panel (right side)
pub fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;

    let Some(detail) = &state.transactions.detail else {
        let block = themed_block(t(locale, TextKey::DialogTransaction), theme.accent, theme);
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
    let currency = get_currency(state);

    let header = &detail.transaction;
    let occurred_at = header.occurred_at.format("%d %b %Y %H:%M").to_string();
    let amount = Money::new(header.amount_minor).format(currency);
    let category = header
        .category
        .as_deref()
        .map(|c| format!("#{c}"))
        .unwrap_or_else(|| "-".to_string());
    let note = header.note.as_deref().unwrap_or("-");
    let locale = state.locale;
    let voided = if header.voided {
        t(locale, TextKey::TxnDetailVoidedYes)
    } else {
        t(locale, TextKey::TxnDetailVoidedNo)
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(t(locale, TextKey::TxnDetailKind), Style::default().fg(theme.text_muted)),
            Span::raw(": "),
            kind_chip(header.kind, theme),
            Span::raw("   "),
            Span::styled(t(locale, TextKey::TxnDetailVoided), Style::default().fg(theme.text_muted)),
            Span::raw(": "),
            Span::styled(
                voided.to_string(),
                Style::default().fg(if header.voided {
                    theme.negative
                } else {
                    theme.text
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(t(locale, TextKey::TxnDetailWhen), Style::default().fg(theme.text_muted)),
            Span::raw(format!(": {occurred_at}")),
        ]),
        Line::from(vec![
            Span::styled(t(locale, TextKey::TxnDetailAmount), Style::default().fg(theme.text_muted)),
            Span::raw(format!(": {amount}")),
        ]),
        Line::from(vec![
            Span::styled(t(locale, TextKey::TxnDetailCategory), Style::default().fg(theme.text_muted)),
            Span::raw(format!(": {category}")),
        ]),
        Line::from(vec![
            Span::styled(t(locale, TextKey::TxnDetailNote), Style::default().fg(theme.text_muted)),
            Span::raw(format!(": {note}")),
        ]),
    ];

    let header_block = themed_block(t(locale, TextKey::TxnDetailTitle), theme.accent, theme);
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
    let currency = get_currency(state);
    let locale = state.locale;

    let legs = detail
        .legs
        .iter()
        .map(|leg| {
            let name = match leg.target {
                LegTarget::Wallet { wallet_id } => resolve_wallet_name(state, wallet_id),
                LegTarget::Flow { flow_id } => resolve_flow_name(state, flow_id),
            };
            let label = match leg.target {
                LegTarget::Wallet { .. } => t(locale, TextKey::TxnDetailLegWallet),
                LegTarget::Flow { .. } => t(locale, TextKey::TxnDetailLegFlow),
            };
            let amount = leg_amount_span(leg.amount_minor, currency, theme);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{label:<6}"), Style::default().fg(theme.text_muted)),
                Span::raw(": "),
                Span::raw(name),
                Span::raw("  "),
                amount,
            ]))
        })
        .collect::<Vec<_>>();

    let legs_block = themed_block(t(locale, TextKey::TxnDetailLegsTitle), theme.accent, theme);
    let list = List::new(legs).block(legs_block);
    frame.render_widget(list, area);
}
