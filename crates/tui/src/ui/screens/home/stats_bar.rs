//! Stats bar rendering for home screen.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use engine::Money;

use crate::{
    app::AppState,
    text::{TextKey, t},
    ui::{
        components::{
            card::Card,
            charts::braille_sparkline_filled,
            money::styled_percentage_change,
        },
        theme::Theme,
    },
};

use super::common::{get_currency, ICON_EXPENSE, ICON_INCOME};

/// Renders the full stats bar with 3 cards.
pub fn render_stats_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    render_net_worth_card(frame, layout[0], state, theme);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    render_stat_card(
        frame,
        layout[1],
        state,
        theme,
        TextKey::HomeCardIncome,
        ICON_INCOME,
        income,
        theme.income,
    );
    render_stat_card(
        frame,
        layout[2],
        state,
        theme,
        TextKey::HomeCardExpenses,
        ICON_EXPENSE,
        expenses,
        theme.expense,
    );
}

/// Renders a compact single-line stats bar.
pub fn render_stats_bar_compact(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);

    let net_worth_minor: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|w| !w.archived)
                .map(|w| w.balance_minor)
                .sum()
        })
        .unwrap_or(0);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    let net_worth = Money::new(net_worth_minor).format(currency);
    let income_str = Money::new(income).format(currency);
    let expenses_str = Money::new(expenses).format(currency);

    let line = Line::from(vec![
        Span::styled("Net Worth: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            net_worth,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(theme.border)),
        Span::styled("In: ", Style::default().fg(theme.text_muted)),
        Span::styled(income_str, Style::default().fg(theme.income)),
        Span::styled("  │  ", Style::default().fg(theme.border)),
        Span::styled("Out: ", Style::default().fg(theme.text_muted)),
        Span::styled(expenses_str, Style::default().fg(theme.expense)),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn render_net_worth_card(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Net Worth", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let net_worth_minor: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|w| !w.archived)
                .map(|w| w.balance_minor)
                .sum()
        })
        .unwrap_or(0);

    let net_worth = Money::new(net_worth_minor).format(currency);

    let (income, expenses) = state
        .stats
        .data
        .as_ref()
        .map(|s| (s.total_income_minor, s.total_expenses_minor))
        .unwrap_or((0, 0));

    let net = income - expenses;
    let pct_change = if expenses > 0 {
        (net as f64 / expenses as f64) * 100.0
    } else {
        0.0
    };

    let trend = braille_sparkline_filled(&state.stats.sparkline);

    // Amount and percentage centered
    let centered_lines = vec![
        Line::from(Span::styled(
            net_worth,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(styled_percentage_change(pct_change, theme)),
    ];

    // Split inner area: centered content on top, sparkline at bottom
    if !trend.is_empty() && inner.height >= 3 {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Length(1)])
            .split(inner);

        frame.render_widget(
            Paragraph::new(centered_lines).alignment(Alignment::Center),
            layout[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                trend,
                Style::default().fg(theme.info),
            ))),
            layout[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(centered_lines).alignment(Alignment::Center),
            inner,
        );
    }
}

fn render_stat_card(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    title: TextKey,
    icon: &str,
    amount: i64,
    color: ratatui::style::Color,
) {
    let currency = get_currency(state);
    let card = Card::new(t(state.locale, title), theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let amount_str = Money::new(amount).format(currency);

    let lines = vec![
        Line::from(vec![
            Span::styled(icon, Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(
                amount_str,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            t(state.locale, TextKey::HomeCardThisMonth),
            Style::default().fg(theme.text_muted),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}
