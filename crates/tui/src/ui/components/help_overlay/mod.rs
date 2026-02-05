mod content;
mod data;
mod footer;
mod header;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::Clear,
};

use crate::{
    app::AppState,
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    if !state.help.active {
        return;
    }
    let locale = state.locale;
    let popup = centered_rect(75, 70, area);

    // Clear the background
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(2), // Footer
        ])
        .split(popup);

    header::render_header(frame, layout[0], state, locale, theme);
    content::render_content(frame, layout[1], state, locale, theme);
    footer::render_footer(frame, layout[2], locale, theme);
}
