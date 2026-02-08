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
    text::{TextKey, t},
    ui::{components::card::Card, theme::Theme},
};

/// Builds the "press r to refresh" hint spans used across stats screens.
fn refresh_hint_spans(locale: crate::text::Locale, theme: &Theme) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            t(locale, TextKey::StatsNoData),
            Style::default().fg(theme.text_muted),
        ),
        Span::styled("r", Style::default().fg(theme.accent)),
        Span::styled(
            t(locale, TextKey::StatsRefreshHint),
            Style::default().fg(theme.text_muted),
        ),
    ]
}

/// Main render function for the stats screen.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;

    // Show error state if stats loading failed
    if let Some(error) = &state.stats.error {
        let card = Card::new(t(locale, TextKey::StatsTitle), theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);

        let mut spans = vec![
            Span::styled(
                error.as_str().to_string(),
                Style::default().fg(theme.negative),
            ),
            Span::raw(" "),
        ];
        spans.extend(refresh_hint_spans(locale, theme));
        frame.render_widget(
            Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Show empty state if no data
    if state.stats.data.is_none() {
        let card = Card::new(t(locale, TextKey::StatsTitle), theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);

        frame.render_widget(
            Paragraph::new(Line::from(refresh_hint_spans(locale, theme)))
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
