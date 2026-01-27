use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    app::{ErrorDialogKind, ErrorDialogState},
    ui::{components::centered_rect, theme::Theme},
};

/// Renders the active error dialog overlay.
pub fn render(frame: &mut Frame<'_>, area: Rect, dialog: Option<&ErrorDialogState>) {
    let Some(dialog) = dialog else {
        return;
    };

    let theme = Theme::default();
    let popup = centered_rect(70, 40, area);
    frame.render_widget(Clear, popup);

    let (icon, border_color) = match dialog.kind {
        ErrorDialogKind::Error => ("✗", theme.negative),
        ErrorDialogKind::Connection => ("⚠", theme.warning),
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
            "Technical details:",
            Style::default().fg(theme.text_muted),
        )));
        lines.push(Line::from(Span::styled(
            detail.as_str(),
            Style::default().fg(theme.text_muted),
        )));
    }

    frame.render_widget(Paragraph::new(lines), layout[0]);

    let actions = match dialog.kind {
        ErrorDialogKind::Connection => {
            let cancel = dialog.cancel_label.as_deref().unwrap_or("Cancel");
            vec![
                Span::styled("[Esc]", Style::default().fg(theme.accent)),
                Span::styled(format!(" {cancel}"), Style::default().fg(theme.text_muted)),
                Span::raw("    "),
                Span::styled("[r]", Style::default().fg(theme.warning)),
                Span::styled(
                    format!(" {}", dialog.confirm_label),
                    Style::default().fg(theme.text_muted),
                ),
            ]
        }
        ErrorDialogKind::Error => vec![
            Span::styled("[Enter]", Style::default().fg(theme.accent)),
            Span::styled(
                format!(" {}", dialog.confirm_label),
                Style::default().fg(theme.text_muted),
            ),
        ],
    };

    frame.render_widget(
        Paragraph::new(Line::from(actions)).alignment(Alignment::Center),
        layout[1],
    );
}
