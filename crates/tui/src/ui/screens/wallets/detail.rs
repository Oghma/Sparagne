//! Wallet detail panel rendering.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use engine::Money;

use crate::{
    app::AppState,
    ui::{
        common::{get_currency, render_empty_state, themed_block},
        components::recent_transactions::render_recent_transactions,
        theme::Theme,
    },
};

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

    let currency = get_currency(state);

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

    frame.render_widget(Paragraph::new(header_lines).block(themed_block("Wallet Detail", theme.accent, theme)), layout[0]);

    // Recent transactions
    render_recent_transactions(
        frame,
        layout[1],
        &state.wallets.detail.transactions,
        state.wallets.detail.error.as_deref(),
        "No transactions for this wallet",
        currency,
        theme,
    );
}

fn render_empty(frame: &mut Frame<'_>, area: Rect, theme: &Theme, message: &str) {
    render_empty_state(frame, area, "Wallet Detail", message, theme);
}
