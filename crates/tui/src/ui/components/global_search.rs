use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, SearchResultKind},
    ui::{common::highlight_matches, components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    if !state.global_search.active {
        return;
    }
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

    render_input(frame, layout[0], state, theme);
    render_list(frame, layout[1], state, theme);
    render_footer(frame, layout[2], theme);
}

fn render_input(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let query = state.global_search.query.as_str();
    let placeholder = "Search transactions, wallets, envelopes, categories...";
    let (text, style) = if query.is_empty() {
        (placeholder, Style::default().fg(theme.text_muted))
    } else {
        (query, Style::default().fg(theme.text))
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  / ", Style::default().fg(theme.accent)),
            Span::styled(text.to_string(), style),
            Span::styled("_", Style::default().fg(theme.accent)),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled(
            " Global Search ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let results = &state.global_search.results;

    if results.is_empty() {
        let block = Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent));

        let message = if state.global_search.query.is_empty() {
            "Type to search across all data"
        } else {
            "No matching results"
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

    let items: Vec<ListItem> = results
        .iter()
        .enumerate()
        .map(|(idx, result)| {
            let icon = match result.kind {
                SearchResultKind::Transaction => "󰄬",
                SearchResultKind::Wallet => "󰆦",
                SearchResultKind::Flow => "󰁫",
                SearchResultKind::Category => "󰷏",
            };

            let kind_label = match result.kind {
                SearchResultKind::Transaction => "TXN",
                SearchResultKind::Wallet => "WAL",
                SearchResultKind::Flow => "ENV",
                SearchResultKind::Category => "CAT",
            };

            let is_selected = idx
                == state
                    .global_search
                    .selected
                    .min(results.len().saturating_sub(1));

            let mut spans = vec![
                Span::raw("  "),
                Span::styled(
                    format!("{icon} "),
                    Style::default().fg(if is_selected {
                        theme.accent
                    } else {
                        theme.text_muted
                    }),
                ),
                Span::styled(format!("[{kind_label}] "), Style::default().fg(theme.text_muted)),
            ];

            if is_selected {
                spans.extend(highlight_matches(
                    &result.label,
                    &state.global_search.query,
                    theme,
                ));
            } else {
                spans.push(Span::styled(
                    result.label.clone(),
                    Style::default().fg(theme.text),
                ));
            }

            if let Some(detail) = &result.detail {
                spans.push(Span::styled(
                    format!(" - {detail}"),
                    Style::default().fg(theme.text_muted),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.global_search.selected.min(items.len() - 1)));
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
        Span::styled(" go to  ", Style::default().fg(theme.text_muted)),
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

