use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{app::AppState, text::Locale, ui::theme::Theme};

use super::data::{context_shortcuts, global_shortcuts};

pub(super) fn render_content(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    locale: Locale,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Two-column layout
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let left_lines = global_shortcuts(locale, theme);
    let right_lines = context_shortcuts(state, locale, theme);

    frame.render_widget(Paragraph::new(left_lines), columns[0]);
    frame.render_widget(Paragraph::new(right_lines), columns[1]);
}
