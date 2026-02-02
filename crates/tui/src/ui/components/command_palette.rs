use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, filter_commands},
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if !state.palette.active {
        return;
    }

    let theme = Theme::default();
    let popup = centered_rect(60, 50, area);

    // Clear the background
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(popup);

    render_input(frame, layout[0], state, &theme);
    render_list(frame, layout[1], state, &theme);
    render_footer(frame, layout[2], &theme);
}

fn render_input(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let query = state.palette.query.as_str();
    let placeholder = "Type to search commands...";
    let (text, style) = if query.is_empty() {
        (placeholder, Style::default().fg(theme.text_muted))
    } else {
        (query, Style::default().fg(theme.text))
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  > ", Style::default().fg(theme.accent)),
            Span::styled(text.to_string(), style),
            Span::styled("_", Style::default().fg(theme.accent)),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled(
            " Command Palette ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let commands = filter_commands(state.palette.query.as_str(), &state.palette.mru);

    if commands.is_empty() {
        let block = Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent));

        let message = if state.palette.query.is_empty() {
            "Type to search commands"
        } else {
            "No matching commands"
        };

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(message, Style::default().fg(theme.text_muted))),
            ])
            .alignment(Alignment::Center)
            .block(block),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = commands
        .iter()
        .enumerate()
        .map(|(idx, cmd)| {
            let label = cmd.label();

            let is_selected = idx == state.palette.selected.min(commands.len().saturating_sub(1));

            let line = if is_selected {
                // Highlight matched characters
                let highlighted = highlight_matches(label, &state.palette.query, theme);
                Line::from(
                    vec![Span::raw("  ")]
                        .into_iter()
                        .chain(highlighted)
                        .collect::<Vec<_>>(),
                )
            } else {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(label, Style::default().fg(theme.text)),
                ])
            };

            ListItem::new(line)
        })
        .collect();

    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.palette.selected.min(items.len() - 1)));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.accent)),
        )
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let line = Line::from(vec![
        Span::styled("[↑↓]", Style::default().fg(theme.accent)),
        Span::styled(" navigate  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Enter]", Style::default().fg(theme.accent)),
        Span::styled(" select  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(" close", Style::default().fg(theme.text_muted)),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}

/// Highlight matching characters in the label
fn highlight_matches<'a>(label: &str, query: &str, theme: &Theme) -> Vec<Span<'a>> {
    if query.is_empty() {
        return vec![Span::styled(
            label.to_string(),
            Style::default().fg(theme.text),
        )];
    }

    let query_lower = query.to_lowercase();
    let label_lower = label.to_lowercase();

    let mut spans = Vec::new();
    let mut last_end = 0;
    let mut query_chars = query_lower.chars().peekable();

    for (i, c) in label_lower.char_indices() {
        if query_chars.peek() == Some(&c) {
            // Add non-matching prefix
            if i > last_end {
                spans.push(Span::styled(
                    label[last_end..i].to_string(),
                    Style::default().fg(theme.text),
                ));
            }
            // Add matching character with highlight
            let char_end = i + c.len_utf8();
            spans.push(Span::styled(
                label[i..char_end].to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            last_end = char_end;
            query_chars.next();
        }
    }

    // Add remaining non-matching suffix
    if last_end < label.len() {
        spans.push(Span::styled(
            label[last_end..].to_string(),
            Style::default().fg(theme.text),
        ));
    }

    spans
}
