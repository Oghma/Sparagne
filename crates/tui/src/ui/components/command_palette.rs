use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, PaletteCommand},
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if !state.palette.active {
        return;
    }

    let theme = Theme::default();
    let popup = centered_rect(70, 50, area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(popup);

    render_input(frame, layout[0], state, &theme);
    render_list(frame, layout[1], state, &theme);
}

fn render_input(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let query = state.palette.query.as_str();
    let placeholder = "cerca un comando...";
    let (text, style) = if query.is_empty() {
        (placeholder, Style::default().fg(theme.dim))
    } else {
        (query, Style::default().fg(theme.text))
    };

    let line = Line::from(vec![
        Span::styled("Command Palette", Style::default().fg(theme.accent)),
        Span::raw("  "),
        Span::styled(text.to_string(), style),
        Span::styled(" |", Style::default().fg(theme.accent)),
    ]);

    let block = Block::default()
        .title("Ctrl+P")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let commands = filtered_commands(state);
    let items = commands
        .iter()
        .map(|cmd| ListItem::new(Line::from(cmd.label())))
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(state.palette.selected.min(items.len() - 1)));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .title("Comandi")
                .borders(Borders::ALL)
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

fn filtered_commands(state: &AppState) -> Vec<PaletteCommand> {
    let query = state.palette.query.trim().to_lowercase();
    let all = PaletteCommand::all();
    if query.is_empty() {
        return all;
    }
    all.into_iter()
        .filter(|cmd| cmd.label().to_lowercase().contains(&query))
        .collect()
}
