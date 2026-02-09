//! Flow detail panel rendering (right side).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use engine::{Currency, Money};

use crate::{
    app::AppState,
    text::{TextKey, t},
    ui::{
        common::{balance_color, get_currency, render_empty_state, themed_block},
        components::{
            money::{flow_cap_line_gauge, styled_amount_no_sign, styled_progress_bar},
            recent_transactions::render_recent_transactions,
        },
        theme::Theme,
    },
};

/// Render the flow detail panel.
pub fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let title = t(state.locale, TextKey::FlowDetailTitle);
    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty_state(
            frame,
            area,
            title,
            t(state.locale, TextKey::LoadingGeneric),
            theme,
        );
        return;
    };
    let Some(detail_id) = state.flows.detail.flow_id else {
        render_empty_state(
            frame,
            area,
            title,
            t(state.locale, TextKey::FlowSelectPrompt),
            theme,
        );
        return;
    };
    let Some(flow) = snapshot.flows.iter().find(|flow| flow.id == detail_id) else {
        render_empty_state(
            frame,
            area,
            title,
            t(state.locale, TextKey::FlowNotFound),
            theme,
        );
        return;
    };

    let currency = get_currency(state);

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

    let bal_color = balance_color(flow.balance_minor, theme);

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
                Style::default().fg(bal_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    if let Some(line) = cap_line {
        header_lines.push(line);
    }

    let header_block = themed_block(title, theme.accent, theme);
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
    render_recent_transactions(
        frame,
        layout[1],
        &state.flows.detail.transactions,
        state.flows.detail.error.as_deref(),
        t(state.locale, TextKey::FlowNoTransactions),
        currency,
        theme,
    );
}

/// Extracts the cap label, current usage, and cap value from a flow detail.
///
/// Returns `None` when the flow has no cap or the cap is non-positive.
fn cap_values(detail: &engine::CashFlow) -> Option<(&'static str, i64, i64)> {
    let cap = detail.max_balance?;
    if cap <= 0 {
        return None;
    }
    let (label, current) = match detail.income_balance {
        Some(income) => ("Income cap", income),
        None => ("Net cap", detail.balance),
    };
    Some((label, current.max(0), cap))
}

/// Create a cap progress line showing current vs cap.
fn cap_progress_line(
    detail: &engine::CashFlow,
    currency: Currency,
    theme: &Theme,
) -> Option<Line<'static>> {
    let (label, current, cap) = cap_values(detail)?;
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
    let (_, current, cap) = cap_values(detail)?;
    flow_cap_line_gauge(current, Some(cap), theme)
}
