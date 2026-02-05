use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AliasFocus, AppState, CategoriesMode},
    ui::{components::input_dialog::InputDialog, forms::FormFieldRenderer, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {

    match state.categories.mode {
        CategoriesMode::Merge => {
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(8)])
                .split(area);
            render_list(frame, body[0], state, theme);
            render_merge_info(frame, body[1], state, theme);
        }
        CategoriesMode::Aliases => {
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(7)])
                .split(area);
            render_alias_list(frame, body[0], state, theme);
            render_alias_input(frame, body[1], state, theme);
        }
        CategoriesMode::Create => {
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(7),
                    Constraint::Min(0),
                    Constraint::Length(6),
                ])
                .split(area);
            render_form(frame, body[0], state, theme);
            render_list(frame, body[1], state, theme);
            render_alias_preview(frame, body[2], state, theme);
        }
        CategoriesMode::Rename | CategoriesMode::List => {
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(6)])
                .split(area);
            render_list(frame, body[0], state, theme);
            render_alias_preview(frame, body[1], state, theme);
        }
    }

    if state.categories.mode == CategoriesMode::Rename {
        render_rename_dialog(frame, area, state, theme);
    }
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let header_spans = vec![
        Span::styled("[c]", Style::default().fg(theme.accent)),
        Span::styled(" create  ", Style::default().fg(theme.text_muted)),
        Span::styled("[e]", Style::default().fg(theme.accent)),
        Span::styled(" rename  ", Style::default().fg(theme.text_muted)),
        Span::styled("[l]", Style::default().fg(theme.accent)),
        Span::styled(" aliases  ", Style::default().fg(theme.text_muted)),
        Span::styled("[m]", Style::default().fg(theme.accent)),
        Span::styled(" merge", Style::default().fg(theme.text_muted)),
    ];

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " Categories ",
            Style::default().fg(theme.accent),
        ))
        .title_bottom(Line::from(header_spans).centered());

    if state.categories.items.is_empty() {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No categories yet",
                Style::default().fg(theme.text_muted),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("[c]", Style::default().fg(theme.accent)),
                Span::styled(
                    " to create your first category",
                    Style::default().fg(theme.text_muted),
                ),
            ]),
        ];
        let empty_msg = Paragraph::new(lines)
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
            let emoji = "🏷️";
            let name_style = if category.archived {
                Style::default().fg(theme.text_muted)
            } else {
                Style::default().fg(theme.text)
            };

            let mut spans = vec![
                Span::raw(format!("  {emoji} ")),
                Span::styled(format!("{:<20}", category.name), name_style),
            ];

            if category.is_system {
                spans.push(Span::styled("[system]", Style::default().fg(theme.info)));
                spans.push(Span::raw(" "));
            }
            if category.archived {
                spans.push(Span::styled(
                    "[archived]",
                    Style::default().fg(theme.warning),
                ));
                spans.push(Span::raw(" "));
            }
            if let Some(from_idx) = from_index
                && idx == from_idx
            {
                spans.push(Span::styled("[FROM]", Style::default().fg(theme.negative)));
                spans.push(Span::raw(" "));
            }
            if let Some(target_idx) = target_index
                && idx == target_idx
            {
                spans.push(Span::styled("[TO]", Style::default().fg(theme.positive)));
            }

            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    list_state.select(Some(selected.min(items.len().saturating_sub(1))));

    let list = List::new(items)
        .block(list_block)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_rename_dialog(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(category) = state.categories.items.get(state.categories.selected) else {
        return;
    };

    let error = state.categories.form.name.state.validation.error_message();

    let dialog = InputDialog {
        title: "Rename Category",
        current_label: Some("Current:"),
        current_value: Some(category.name.as_str()),
        prompt: "New name:",
        value: state.categories.form.name.value(),
        focused: true,
        error,
        confirm_label: "Save",
        cancel_label: "Cancel",
    };

    crate::ui::components::input_dialog::render(frame, area, dialog, theme);
}

fn render_alias_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let is_focused = state.categories.aliases.focus == AliasFocus::List;
    let border_color = if is_focused {
        theme.accent
    } else {
        theme.border
    };

    let category_name = state
        .categories
        .items
        .get(state.categories.selected)
        .map(|c| c.name.as_str())
        .unwrap_or("?");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" Aliases for {} ", category_name),
            Style::default().fg(theme.accent),
        ));

    let items: Vec<ListItem> = state
        .categories
        .aliases
        .items
        .iter()
        .map(|alias| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(&alias.alias, Style::default().fg(theme.text)),
            ]))
        })
        .collect();

    if items.is_empty() {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No aliases for this category",
                Style::default().fg(theme.text_muted),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Type in the input below to add one",
                Style::default().fg(theme.text_muted),
            )),
        ];
        let empty_msg = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(block);
        frame.render_widget(empty_msg, area);
        return;
    }

    let mut list_state = ListState::default();
    let selected = state
        .categories
        .aliases
        .selected
        .min(items.len().saturating_sub(1));
    list_state.select(Some(selected));

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_alias_input(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let is_focused = state.categories.aliases.focus == AliasFocus::Input;
    let border_color = if is_focused {
        theme.accent
    } else {
        theme.border
    };
    let cursor = if is_focused { "_" } else { "" };

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  New alias: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                state.categories.aliases.input.as_str(),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::default().fg(theme.accent)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [Enter]", Style::default().fg(theme.accent)),
            Span::styled(" save  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Tab]", Style::default().fg(theme.accent)),
            Span::styled(" switch focus  ", Style::default().fg(theme.text_muted)),
            Span::styled("[x]", Style::default().fg(theme.accent)),
            Span::styled(" delete  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Esc]", Style::default().fg(theme.accent)),
            Span::styled(" back", Style::default().fg(theme.text_muted)),
        ]),
    ];

    if let Some(err) = state.categories.aliases.error.as_ref() {
        lines.push(Line::from(Span::styled(
            format!("  ⚠ {err}"),
            Style::default().fg(theme.negative),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_alias_preview(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(category) = state.categories.items.get(state.categories.selected) else {
        let block = Block::default()
            .title(Span::styled(" Aliases ", Style::default().fg(theme.accent)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No category selected",
                Style::default().fg(theme.text_muted),
            )))
            .alignment(Alignment::Center)
            .block(block),
            area,
        );
        return;
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Category: ", Style::default().fg(theme.text_muted)),
            Span::styled(&category.name, Style::default().fg(theme.text)),
        ]),
    ];

    if let Some(err) = state.categories.aliases.error.as_ref() {
        lines.push(Line::from(Span::styled(
            format!("  ⚠ {err}"),
            Style::default().fg(theme.negative),
        )));
    } else if state.categories.aliases.category_id != Some(category.id) {
        lines.push(Line::from(Span::styled(
            "  Press [l] to load aliases",
            Style::default().fg(theme.text_muted),
        )));
    } else if state.categories.aliases.items.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No aliases",
            Style::default().fg(theme.text_muted),
        )));
    } else {
        let shown: Vec<_> = state.categories.aliases.items.iter().take(3).collect();
        for alias in &shown {
            lines.push(Line::from(vec![
                Span::raw("    • "),
                Span::styled(&alias.alias, Style::default().fg(theme.text)),
            ]));
        }
        if state.categories.aliases.items.len() > 3 {
            lines.push(Line::from(Span::styled(
                format!("    ... +{} more", state.categories.aliases.items.len() - 3),
                Style::default().fg(theme.text_muted),
            )));
        }
    }

    let block = Block::default()
        .title(Span::styled(" Aliases ", Style::default().fg(theme.accent)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.categories.form;
    let is_rename = state.categories.mode == CategoriesMode::Rename;

    let title = if is_rename {
        " Rename Category "
    } else {
        " New Category "
    };

    let lines = vec![
        Line::from(""),
        FormFieldRenderer::render_input_field(
            &form.name.label,
            form.name.value(),
            &form.name.state,
            theme,
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [Enter]", Style::default().fg(theme.accent)),
            Span::styled(
                if is_rename { " save  " } else { " create  " },
                Style::default().fg(theme.text_muted),
            ),
            Span::styled("[Esc]", Style::default().fg(theme.accent)),
            Span::styled(" cancel", Style::default().fg(theme.text_muted)),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(theme.accent)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_merge_info(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let (from_name, into_name) = merge_pair(state).unwrap_or(("-".to_string(), "-".to_string()));

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Merge: ", Style::default().fg(theme.text_muted)),
            Span::styled(&from_name, Style::default().fg(theme.negative)),
            Span::styled(" → ", Style::default().fg(theme.text_muted)),
            Span::styled(&into_name, Style::default().fg(theme.positive)),
        ]),
        Line::from(""),
    ];

    if let Some(preview) = state.categories.merge.preview.as_ref() {
        if preview.ok {
            let hint = if state.categories.merge.confirming {
                "✓ Preview OK. Press [Enter] to merge."
            } else {
                "✓ Preview OK. Press [Enter] to confirm."
            };
            lines.push(Line::from(Span::styled(
                format!("  {hint}"),
                Style::default().fg(theme.positive),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  ⚠ Conflicts:",
                Style::default().fg(theme.negative),
            )));
            for conflict in &preview.conflicts {
                lines.push(Line::from(Span::styled(
                    format!(
                        "    • {}",
                        merge_conflict_label(conflict.kind.as_str(), conflict.value.as_str())
                    ),
                    Style::default().fg(theme.text),
                )));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  Select target and press [Enter] for preview",
            Style::default().fg(theme.text_muted),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  [Enter]", Style::default().fg(theme.accent)),
        Span::styled(" preview/merge  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(" cancel", Style::default().fg(theme.text_muted)),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " Merge Categories ",
            Style::default().fg(theme.accent),
        ));
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
        "same_category" => "Cannot merge category with itself".to_string(),
        "source_system" => format!("System category: {value}"),
        "target_archived" => format!("Target is archived: {value}"),
        "alias_conflict" => format!("Alias conflict: {value}"),
        "name_conflict" => format!("Name conflict: {value}"),
        _ => format!("Conflict: {kind} ({value})"),
    }
}
