use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::ui::{Theme, components::centered_rect};

/// Input dialog data for rendering a single-value modal.
pub struct InputDialog<'a> {
    pub title: &'a str,
    pub current_label: Option<&'a str>,
    pub current_value: Option<&'a str>,
    pub prompt: &'a str,
    pub value: &'a str,
    pub focused: bool,
    pub error: Option<&'a str>,
    pub confirm_label: &'a str,
    pub cancel_label: &'a str,
}

/// Renders a centered input dialog.
pub fn render(frame: &mut Frame<'_>, area: Rect, dialog: InputDialog<'_>, theme: &Theme) {
    let popup = centered_rect(60, 40, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", dialog.title),
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut constraints = vec![Constraint::Length(1)];
    if dialog.current_value.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Length(3));
    if dialog.error.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Min(0));

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut row = 1;

    if let Some(current_value) = dialog.current_value {
        let label = dialog.current_label.unwrap_or("Current:");
        let line = Line::from(vec![
            Span::styled(format!("{label} "), Style::default().fg(theme.text_muted)),
            Span::styled(current_value, Style::default().fg(theme.text)),
        ]);
        frame.render_widget(Paragraph::new(line), layout[row]);
        row += 1;
    }

    let prompt_line = Line::from(Span::styled(
        dialog.prompt,
        Style::default().fg(theme.text_muted),
    ));
    frame.render_widget(Paragraph::new(prompt_line), layout[row]);
    row += 1;

    let cursor = if dialog.focused { "_" } else { "" };
    let input_value = format!("{}{}", dialog.value, cursor);
    let input_border = if dialog.focused {
        theme.border_focused
    } else {
        theme.border
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(input_border));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            input_value,
            Style::default().fg(theme.text),
        )))
        .block(input_block),
        layout[row],
    );
    row += 1;

    if let Some(error) = dialog.error {
        let line = Line::from(Span::styled(
            format!("⚠ {error}"),
            Style::default().fg(theme.negative),
        ));
        frame.render_widget(Paragraph::new(line), layout[row]);
        row += 1;
    }

    let actions = Line::from(vec![
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(
            format!(" {}", dialog.cancel_label),
            Style::default().fg(theme.text_muted),
        ),
        Span::raw("    "),
        Span::styled("[Enter]", Style::default().fg(theme.accent)),
        Span::styled(
            format!(" {}", dialog.confirm_label),
            Style::default().fg(theme.text_muted),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        layout[row],
    );
}
