use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, MemberFormField, MembersMode, MembersScope},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // List
            Constraint::Length(6), // Form
            Constraint::Length(2), // Footer
        ])
        .split(area);

    render_list(frame, layout[0], state, &theme);
    render_form(frame, layout[1], state, &theme);
    render_footer(frame, layout[2], state, &theme);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Build title with scope info
    let scope_label = match state.members.scope {
        MembersScope::Vault => "Vault Members".to_string(),
        MembersScope::Flow => {
            let flow_name = member_flow_name(state).unwrap_or_else(|| "Flow".to_string());
            format!("👥 {} Members", flow_name)
        }
    };

    let is_focused = state.members.mode == MembersMode::List;
    let border_color = if is_focused {
        theme.border_focused
    } else {
        theme.border
    };

    let list_block = Block::default()
        .title(Span::styled(
            format!(" {} ", scope_label),
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    if state.members.items.is_empty() {
        let msg = if state.members.scope == MembersScope::Flow {
            "No members in this flow"
        } else {
            "No vault members"
        };
        let empty_msg = Paragraph::new(Line::from(vec![Span::styled(
            msg,
            Style::default().fg(theme.text_muted),
        )]))
        .alignment(Alignment::Center)
        .block(list_block);
        frame.render_widget(empty_msg, area);
        return;
    }

    let items = state
        .members
        .items
        .iter()
        .map(|member| {
            let (label, color) = role_chip(member.role, theme);
            let spans = vec![
                Span::raw("  "),
                Span::styled("👤 ", Style::default().fg(theme.text_muted)),
                Span::styled(member.username.clone(), Style::default().fg(theme.text)),
                Span::raw("  "),
                role_badge(label, color),
            ];
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    let selected = state.members.selected.min(items.len().saturating_sub(1));
    list_state.select(Some(selected));
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

fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let is_focused = state.members.mode == MembersMode::Form;
    let border_color = if is_focused {
        theme.border_focused
    } else {
        theme.border
    };

    let title = if state.members.form.editing {
        " Edit Member "
    } else {
        " Add Member "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(title, Style::default().fg(theme.accent)));

    let mut lines = Vec::new();

    // Username field
    let username_focused = state.members.form.focus == MemberFormField::Username;
    let username_label_style = if username_focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };
    let username_value = if state.members.form.username.is_empty() && username_focused {
        "_"
    } else {
        state.members.form.username.as_str()
    };
    lines.push(Line::from(vec![
        Span::styled("  Username  ", username_label_style),
        Span::styled(username_value.to_string(), Style::default().fg(theme.text)),
        if username_focused {
            Span::styled("_", Style::default().fg(theme.accent))
        } else {
            Span::raw("")
        },
    ]));

    // Role field
    let role_focused = state.members.form.focus == MemberFormField::Role;
    let role_label_style = if role_focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };
    let (role_text, role_color) = role_chip(state.members.form.role, theme);
    lines.push(Line::from(vec![
        Span::styled("  Role      ", role_label_style),
        role_badge(role_text, role_color),
        if role_focused {
            Span::styled("  ↑↓ change", Style::default().fg(theme.text_muted))
        } else {
            Span::raw("")
        },
    ]));

    // Error message
    if let Some(err) = state.members.form.error.as_ref() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ✗ ", Style::default().fg(theme.negative)),
            Span::styled(err.clone(), Style::default().fg(theme.negative)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let hints = match state.members.mode {
        MembersMode::List => vec![
            ("[a]", "add"),
            ("[e]", "edit"),
            ("[x]", "remove"),
            ("[v]", "vault"),
            ("[f]", "flow"),
            ("[↑↓]", "select"),
        ],
        MembersMode::Form => vec![
            ("[Tab]", "next"),
            ("[↑↓]", "change"),
            ("[Enter]", "save"),
            ("[Esc]", "cancel"),
        ],
    };

    let mut spans = Vec::new();
    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(*key, Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            format!(" {action}"),
            Style::default().fg(theme.text_muted),
        ));
    }

    // Add error from main state if present
    if let Some(err) = state.members.error.as_ref() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            err.clone(),
            Style::default().fg(theme.negative),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn member_flow_name(state: &AppState) -> Option<String> {
    let snapshot = state.snapshot.as_ref()?;
    let flows = snapshot
        .flows
        .iter()
        .filter(|flow| !flow.archived && !flow.is_unallocated)
        .collect::<Vec<_>>();
    flows
        .get(state.members.flow_index)
        .map(|flow| flow.name.clone())
}

fn role_chip(
    role: api_types::membership::MembershipRole,
    theme: &Theme,
) -> (&'static str, ratatui::style::Color) {
    match role {
        api_types::membership::MembershipRole::Owner => ("OWNER", theme.accent),
        api_types::membership::MembershipRole::Editor => ("EDITOR", theme.positive),
        api_types::membership::MembershipRole::Viewer => ("VIEWER", theme.text_muted),
    }
}

fn role_badge(label: &str, color: ratatui::style::Color) -> Span<'static> {
    Span::styled(format!("[{label}]"), Style::default().fg(color))
}
