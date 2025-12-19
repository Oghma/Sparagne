use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::{app::AppState, ui::theme::Theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, _state: &AppState) {
    let theme = Theme::default();
    let block = Block::default()
        .title("Home")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let content = Paragraph::new(Line::from("Home screen placeholder (TUI scaffold)."))
        .alignment(Alignment::Center)
        .block(block);
    frame.render_widget(content, area);
}
