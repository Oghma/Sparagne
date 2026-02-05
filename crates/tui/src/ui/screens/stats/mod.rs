//! Stats screen module: displays financial statistics across multiple tabs.
//!
//! This module is organized into:
//! - `tabs`: Tab bar and tab routing
//! - `cash_flow`: Cash flow tab with stat cards, summaries, trends
//! - `spending`: Spending tab with expense breakdown
//! - `net_worth`: Net worth tab with balance tracking
//! - `components`: Shared components (stat cards, rows, helpers)

mod cash_flow;
mod components;
mod net_worth;
mod spending;
mod tabs;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::AppState,
    ui::{components::card::Card, theme::Theme},
};

/// Main render function for the stats screen.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {

    // Show error state if stats loading failed
    if let Some(error) = &state.stats.error {
        let card = Card::new("Stats", theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(error.as_str(), Style::default().fg(theme.error)),
                Span::raw(" Press "),
                Span::styled("r", Style::default().fg(theme.accent)),
                Span::raw(" to refresh."),
            ]))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Show empty state if no data
    if state.stats.data.is_none() {
        let card = Card::new("Stats", theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("No data. Press "),
                Span::styled("r", Style::default().fg(theme.accent)),
                Span::raw(" to refresh."),
            ]))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Layout: tab bar + tab content
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    tabs::render_tab_bar(frame, layout[0], state, theme);
    tabs::render_tab_content(frame, layout[1], state, theme);
}
