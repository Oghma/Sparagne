use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, CategoriesMode},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state, &theme);

    match state.categories.mode {
        CategoriesMode::Merge => {
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(7)])
                .split(layout[1]);
            render_list(frame, body[0], state, &theme);
            render_merge_info(frame, body[1], state, &theme);
        }
        CategoriesMode::Create | CategoriesMode::Rename => {
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(0)])
                .split(layout[1]);
            render_form(frame, body[0], state, &theme);
            render_list(frame, body[1], state, &theme);
        }
        CategoriesMode::List => render_list(frame, layout[1], state, &theme),
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mode = match state.categories.mode {
        CategoriesMode::List => "List",
        CategoriesMode::Merge => "Merge",
        CategoriesMode::Create => "Create",
        CategoriesMode::Rename => "Rename",
    };
    let mut line = vec![
        Span::styled("Mode", Style::default().fg(theme.dim)),
        Span::raw(format!(": {mode}")),
    ];

    if let CategoriesMode::Merge = state.categories.mode {
        if let (Some(from), Some(into)) = merge_pair(state) {
            line.push(Span::raw("   "));
            line.push(Span::styled("From", Style::default().fg(theme.dim)));
            line.push(Span::raw(format!(": {} -> {}", from, into)));
        }
    }

    if let Some(err) = state.categories.error.as_ref() {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title("Categories");
    frame.render_widget(Paragraph::new(Line::from(line)).block(block), area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    if state.categories.items.is_empty() {
        let empty_msg = Paragraph::new(Line::from("No categories yet."))
            .alignment(Alignment::Center)
            .block(list_block);
        frame.render_widget(empty_msg, area);
        return;
    }

    let (from_index, target_index, selected) = match state.categories.mode {
        CategoriesMode::Merge => (
            Some(state.categories.merge.from_index),
            Some(state.categories.merge.target_index),
            state.categories.merge.target_index,
        ),
        _ => (None, None, state.categories.selected),
    };

    let items = state
        .categories
        .items
        .iter()
        .enumerate()
        .map(|(idx, category)| {
            let mut spans = Vec::new();
            let mut name_style = Style::default().fg(theme.text);
            if category.archived {
                name_style = name_style.fg(theme.dim);
            }
            spans.push(Span::styled(category.name.clone(), name_style));
            spans.push(Span::raw(" "));
            if category.is_system {
                spans.push(status_chip("SYSTEM", theme.accent));
            }
            if category.archived {
                spans.push(status_chip("ARCHIVED", theme.warning));
            }
            if let Some(from_idx) = from_index {
                if idx == from_idx {
                    spans.push(status_chip("FROM", theme.text_muted));
                }
            }
            if let Some(target_idx) = target_index {
                if idx == target_idx {
                    spans.push(status_chip("TO", theme.text_muted));
                }
            }
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    list_state.select(Some(selected.min(items.len().saturating_sub(1))));
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(theme.accent).fg(theme.background));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.categories.form;
    let is_rename = state.categories.mode == CategoriesMode::Rename;

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Name", Style::default().fg(theme.dim)),
        Span::raw(": "),
        Span::styled(
            form.name.as_str(),
            Style::default()
                .fg(theme.text)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        if is_rename {
            "Enter: rename • Esc: cancel"
        } else {
            "Enter: create • Esc: cancel"
        },
        Style::default().fg(theme.dim),
    )));
    if let Some(err) = form.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title(if is_rename {
            "Rename Category"
        } else {
            "New Category"
        })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_merge_info(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Enter", Style::default().fg(theme.accent)),
        Span::raw(" preview/merge  "),
        Span::styled("Esc", Style::default().fg(theme.accent)),
        Span::raw(" cancel"),
    ]));

    if let Some(preview) = state.categories.merge.preview.as_ref() {
        if preview.ok {
            let hint = if state.categories.merge.confirming {
                "Preview ok. Press Enter to merge."
            } else {
                "Preview ok. Press Enter to confirm."
            };
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(theme.accent),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "Conflicts:",
                Style::default().fg(theme.error),
            )));
            for conflict in &preview.conflicts {
                lines.push(Line::from(format!(
                    "- {}",
                    merge_conflict_label(conflict.kind.as_str(), conflict.value.as_str())
                )));
            }
        }
    } else {
        lines.push(Line::from("Select a target and press Enter for preview."));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title("Merge");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn merge_pair(state: &AppState) -> Option<(String, String)> {
    let from = state
        .categories
        .items
        .get(state.categories.merge.from_index)?;
    let into = state
        .categories
        .items
        .get(state.categories.merge.target_index)?;
    Some((from.name.clone(), into.name.clone()))
}

fn merge_conflict_label(kind: &str, value: &str) -> String {
    match kind {
        "same_category" => "Same category.".to_string(),
        "source_system" => format!("Source is system: {value}."),
        "target_archived" => format!("Target is archived: {value}."),
        "alias_conflict" => format!("Alias conflict: {value}."),
        "name_conflict" => format!("Name conflict: {value}."),
        _ => format!("Conflict: {kind} ({value})."),
    }
}

fn status_chip(label: &str, color: ratatui::style::Color) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}
