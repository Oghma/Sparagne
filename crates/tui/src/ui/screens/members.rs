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
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(6),
        ])
        .split(area);

    render_header(frame, layout[0], state, &theme);
    render_list(frame, layout[1], state, &theme);
    render_form(frame, layout[2], state, &theme);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mode = match state.members.mode {
        MembersMode::List => "List",
        MembersMode::Form => {
            if state.members.form.editing {
                "Edit"
            } else {
                "Add"
            }
        }
    };
    let scope = match state.members.scope {
        MembersScope::Vault => "Vault",
        MembersScope::Flow => "Flow",
    };
    let flow_name = if state.members.scope == MembersScope::Flow {
        member_flow_name(state)
    } else {
        None
    };

    let mut line = vec![
        Span::styled("Mode", Style::default().fg(theme.dim)),
        Span::raw(format!(": {mode}")),
        Span::raw("   "),
        Span::styled("Scope", Style::default().fg(theme.dim)),
        Span::raw(format!(": {scope}")),
    ];
    if let Some(flow) = flow_name {
        line.push(Span::raw("   "));
        line.push(Span::styled("Flow", Style::default().fg(theme.dim)));
        line.push(Span::raw(format!(": {flow}")));
    }

    if let Some(err) = state.members.error.as_ref() {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title("Members");
    frame.render_widget(Paragraph::new(Line::from(line)).block(block), area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    if state.members.items.is_empty() {
        let msg = if state.members.scope == MembersScope::Flow {
            "Nessun membro per questo flow."
        } else {
            "Nessun membro."
        };
        let empty_msg = Paragraph::new(Line::from(msg))
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
                Span::styled(member.username.clone(), Style::default().fg(theme.text)),
                Span::raw(" "),
                status_chip(label, color),
            ];
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    let selected = state.members.selected.min(items.len().saturating_sub(1));
    list_state.select(Some(selected));
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(theme.accent).fg(theme.background));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mut border_style = Style::default().fg(theme.border);
    if state.members.mode == MembersMode::Form {
        border_style = border_style.fg(theme.accent);
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title("Member Form");

    let mut lines = Vec::new();
    let username_style = if state.members.form.focus == MemberFormField::Username {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };
    lines.push(Line::from(vec![
        Span::styled("Username", username_style),
        Span::raw(format!(": {}", state.members.form.username)),
    ]));

    let role_style = if state.members.form.focus == MemberFormField::Role {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };
    lines.push(Line::from(vec![
        Span::styled("Role", role_style),
        Span::raw(format!(": {}", role_label(state.members.form.role))),
    ]));

    if let Some(err) = state.members.form.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
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

fn role_label(role: api_types::membership::MembershipRole) -> &'static str {
    match role {
        api_types::membership::MembershipRole::Owner => "owner",
        api_types::membership::MembershipRole::Editor => "editor",
        api_types::membership::MembershipRole::Viewer => "viewer",
    }
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

fn status_chip(label: &str, color: ratatui::style::Color) -> Span<'static> {
    Span::styled(format!(" {label} "), Style::default().fg(color))
}
