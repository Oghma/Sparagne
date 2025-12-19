use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::{app::AppState, ui::theme::Theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let title = state.section.label();
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));

    let content = Paragraph::new(Line::from("Sezione in costruzione."))
        .alignment(Alignment::Center)
        .block(block);
    frame.render_widget(content, area);
}
