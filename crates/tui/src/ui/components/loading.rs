use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::ui::Theme;

const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Returns the spinner frame for the given index.
pub fn spinner_frame(index: usize) -> char {
    SPINNER_FRAMES[index % SPINNER_FRAMES.len()]
}

/// Renders a full-screen loading overlay with optional detail text.
pub fn render_fullscreen(
    frame: &mut Frame<'_>,
    area: Rect,
    spinner: char,
    message: &str,
    detail: Option<&str>,
    theme: &Theme,
) {
    frame.render_widget(Clear, area);

    let mut lines = vec![Line::from(vec![
        Span::styled(spinner.to_string(), Style::default().fg(theme.accent)),
        Span::styled(format!(" {message}"), Style::default().fg(theme.text)),
    ])];

    if let Some(detail) = detail {
        lines.push(Line::from(Span::styled(
            detail.to_string(),
            Style::default().fg(theme.text_muted),
        )));
    }

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

/// Renders an inline loading message inside an existing block.
pub fn render_inline_block<'a>(
    frame: &mut Frame<'_>,
    area: Rect,
    block: Block<'a>,
    spinner: char,
    message: &str,
    detail: Option<&str>,
    theme: &Theme,
) {
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![Line::from(vec![
        Span::styled(spinner.to_string(), Style::default().fg(theme.accent)),
        Span::styled(format!(" {message}"), Style::default().fg(theme.text_muted)),
    ])];

    if let Some(detail) = detail {
        lines.push(Line::from(Span::styled(
            detail.to_string(),
            Style::default().fg(theme.text_muted),
        )));
    }

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}
