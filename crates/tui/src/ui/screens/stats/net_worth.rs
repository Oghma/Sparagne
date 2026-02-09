//! Net worth tab rendering: month summary, sparkline, trends.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::{app::AppState, ui::theme::Theme};

use super::cash_flow::{render_month_summary, render_monthly_trend, render_sparkline};

/// Render the net worth tab.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // Summary
            Constraint::Length(6), // Sparkline
            Constraint::Min(6),    // Trend
        ])
        .split(area);

    render_month_summary(frame, layout[0], state, theme);
    render_sparkline(frame, layout[1], state, theme);
    render_monthly_trend(frame, layout[2], state, theme);
}
