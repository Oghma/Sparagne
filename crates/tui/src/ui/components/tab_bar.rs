use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::ui::theme::Theme;

pub struct TabBarItem<'a> {
    pub label: &'a str,
}

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    items: &[TabBarItem<'_>],
    active_index: usize,
    theme: &Theme,
) {
    if items.is_empty() || area.height == 0 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    let mut label_spans = Vec::new();
    let mut underline_spans = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            label_spans.push(Span::raw("  "));
            underline_spans.push(Span::raw("  "));
        }

        let label = format!(" {} ", item.label);
        let is_active = idx == active_index;

        let label_style = if is_active {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted)
        };
        label_spans.push(Span::styled(label.clone(), label_style));

        // VSCode-like underline under the active tab.
        let underline = if is_active {
            "━".repeat(label.len())
        } else {
            " ".repeat(label.len())
        };
        underline_spans.push(Span::styled(underline, Style::default().fg(theme.accent)));
    }

    frame.render_widget(Paragraph::new(Line::from(label_spans)), layout[0]);
    frame.render_widget(Paragraph::new(Line::from(underline_spans)), layout[1]);
}
