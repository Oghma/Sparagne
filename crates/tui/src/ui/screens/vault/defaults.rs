use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, DefaultsField},
    ui::theme::Theme,
};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        let block = Block::default()
            .title(Span::styled(
                " Quick Defaults ",
                Style::default().fg(theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_focused));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Snapshot not available",
                Style::default().fg(theme.text_muted),
            )))
            .alignment(Alignment::Center)
            .block(block),
            area,
        );
        return;
    };

    let wallet_names: Vec<String> = snapshot
        .wallets
        .iter()
        .filter(|wallet| !wallet.archived)
        .map(|wallet| wallet.name.clone())
        .collect();
    let flow_names: Vec<String> = snapshot
        .flows
        .iter()
        .filter(|flow| !flow.archived)
        .map(|flow| flow.name.clone())
        .collect();

    let defaults = &state.vault_ui.defaults;
    let wallet_label = if defaults.wallet_index == 0 {
        "None"
    } else {
        wallet_names
            .get(defaults.wallet_index - 1)
            .map(|name| name.as_str())
            .unwrap_or("None")
    };
    let flow_label = if defaults.flow_index == 0 {
        "None"
    } else {
        flow_names
            .get(defaults.flow_index - 1)
            .map(|name| name.as_str())
            .unwrap_or("None")
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    // Form section
    let form_block = Block::default()
        .title(Span::styled(
            " Quick Defaults ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused));
    let form_inner = form_block.inner(layout[0]);
    frame.render_widget(form_block, layout[0]);

    let form_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Wallet
            Constraint::Length(1), // Flow
        ])
        .split(form_inner);

    // Wallet field
    let wallet_focused = defaults.focus == DefaultsField::Wallet;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  Default Wallet  ",
                if wallet_focused {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
            Span::styled(wallet_label.to_string(), Style::default().fg(theme.text)),
            if wallet_focused {
                Span::styled("  ↑↓", Style::default().fg(theme.text_muted))
            } else {
                Span::raw("")
            },
        ])),
        form_layout[0],
    );

    // Flow field
    let flow_focused = defaults.focus == DefaultsField::Flow;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  Default Flow    ",
                if flow_focused {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
            Span::styled(flow_label.to_string(), Style::default().fg(theme.text)),
            if flow_focused {
                Span::styled("  ↑↓", Style::default().fg(theme.text_muted))
            } else {
                Span::raw("")
            },
        ])),
        form_layout[1],
    );

    // Lists section
    let lists = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[1]);

    render_defaults_list(
        frame,
        lists[0],
        "Wallets",
        &wallet_names,
        defaults.wallet_index,
        wallet_focused,
        theme,
    );
    render_defaults_list(
        frame,
        lists[1],
        "Flows",
        &flow_names,
        defaults.flow_index,
        flow_focused,
        theme,
    );

    // Error
    if let Some(err) = defaults.error.as_ref() {
        let error_area = Rect {
            y: area.y + area.height.saturating_sub(1),
            height: 1,
            ..area
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("✗ ", Style::default().fg(theme.negative)),
                Span::styled(err.clone(), Style::default().fg(theme.negative)),
            ])),
            error_area,
        );
    }
}

fn render_defaults_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
    focused: bool,
    theme: &Theme,
) {
    let mut list_items = Vec::with_capacity(items.len() + 1);
    list_items.push(ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled("None", Style::default().fg(theme.text_muted)),
    ])));
    list_items.extend(items.iter().map(|name| {
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(name.clone(), Style::default().fg(theme.text)),
        ]))
    }));

    let mut list_state = ListState::default();
    if !list_items.is_empty() {
        list_state.select(Some(selected.min(list_items.len() - 1)));
    }

    let highlight_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let border_color = if focused {
        theme.border_focused
    } else {
        theme.border
    };

    let block = Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    let list = List::new(list_items)
        .block(block)
        .highlight_style(highlight_style)
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}
