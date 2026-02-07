use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{
    text::{Locale, TextKey, t},
    ui::theme::Theme,
};

pub(super) fn render_footer(frame: &mut Frame<'_>, area: Rect, locale: Locale, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let line = Line::from(vec![
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(
            format!(" {}", t(locale, TextKey::HelpCloseHelp)),
            Style::default().fg(theme.text_muted),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}
