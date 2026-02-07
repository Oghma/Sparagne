//! Spending tab rendering: expense sparkline, category breakdown, trends.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::Span,
    widgets::Paragraph,
};

use crate::{
    app::AppState,
    text::{TextKey, t},
    ui::{
        components::{card::Card, charts::render_braille_sparkline},
        theme::Theme,
    },
};

use super::cash_flow::{render_category_breakdown, render_monthly_trend};

/// Render the spending tab.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Sparkline for expense trend
            Constraint::Length(12), // Category breakdown
            Constraint::Min(6),     // Trend chart
        ])
        .split(area);

    render_expense_sparkline(frame, layout[0], state, theme);
    render_category_breakdown(frame, layout[1], state, theme);
    render_monthly_trend(frame, layout[2], state, theme);
}

/// Render the expense sparkline showing 6-month trend.
fn render_expense_sparkline(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;
    let card = Card::new(t(locale, TextKey::StatsExpenseTrend), theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    // Convert monthly trend data (last 6 months) to sparkline format
    let expense_data: Vec<u64> = state
        .stats
        .monthly_trend
        .iter()
        .map(|(_, value)| (*value).max(0) as u64)
        .collect();

    if expense_data.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                t(locale, TextKey::StatsNoExpenseData),
                Style::default().fg(theme.text_muted),
            ))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    render_braille_sparkline(frame, inner, &expense_data, theme, true);
}
