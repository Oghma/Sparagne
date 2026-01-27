use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    app::{ConfirmDialogKind, ConfirmDialogState},
    ui::{components::centered_rect, theme::Theme},
};

/// Renders the active confirmation dialog overlay.
pub fn render(frame: &mut Frame<'_>, area: Rect, dialog: Option<&ConfirmDialogState>) {
    let Some(dialog) = dialog else {
        return;
    };

    let theme = Theme::default();
    let popup = centered_rect(70, 40, area);
    frame.render_widget(Clear, popup);

    let (icon, border_color) = match dialog.kind {
        ConfirmDialogKind::Delete => ("✗", theme.negative),
        ConfirmDialogKind::Archive => ("⚠", theme.warning),
        ConfirmDialogKind::DiscardChanges => ("⚠", theme.accent),
    };

    let block = Block::default()
        .title(Span::styled(
            format!(" {icon} {} ", dialog.title),
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            dialog.message.as_str(),
            Style::default().fg(theme.text),
        )),
    ];

    if let Some(detail) = dialog.detail.as_ref() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            detail.as_str(),
            Style::default().fg(theme.text_muted),
        )));
    }

    if !dialog.preview.is_empty() {
        lines.push(Line::from(""));
        for preview in &dialog.preview {
            lines.push(Line::from(Span::styled(
                format!("  {preview}"),
                Style::default().fg(theme.text),
            )));
        }
    }

    if let Some(warning) = dialog.warning.as_ref() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            warning.as_str(),
            Style::default().fg(theme.negative),
        )));
    }

    frame.render_widget(Paragraph::new(lines), layout[0]);

    let mut actions = vec![
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(
            format!(" {}", dialog.cancel_label),
            Style::default().fg(theme.text_muted),
        ),
    ];

    if let Some(extra_label) = dialog.extra_label.as_ref() {
        actions.push(Span::raw("    "));
        actions.push(Span::styled("[d]", Style::default().fg(theme.warning)));
        actions.push(Span::styled(
            format!(" {extra_label}"),
            Style::default().fg(theme.text_muted),
        ));
    }

    actions.push(Span::raw("    "));
    actions.push(Span::styled("[Enter]", Style::default().fg(theme.accent)));
    actions.push(Span::styled(
        format!(" {}", dialog.confirm_label),
        Style::default().fg(theme.text_muted),
    ));

    frame.render_widget(
        Paragraph::new(Line::from(actions)).alignment(Alignment::Center),
        layout[1],
    );
}
