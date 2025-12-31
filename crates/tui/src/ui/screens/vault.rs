use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, DefaultsField, VaultMode},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state, &theme);

    match state.vault_ui.mode {
        VaultMode::View => render_view(frame, layout[1], state, &theme),
        VaultMode::Create => render_create(frame, layout[1], state, &theme),
        VaultMode::Defaults => render_defaults(frame, layout[1], state, &theme),
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mode = match state.vault_ui.mode {
        VaultMode::View => "View",
        VaultMode::Create => "Create",
        VaultMode::Defaults => "Defaults",
    };
    let mut line = vec![
        Span::styled("Mode", Style::default().fg(theme.dim)),
        Span::raw(format!(": {mode}")),
    ];
    if let Some(err) = state.vault_ui.error.as_ref() {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title("Vault");
    frame.render_widget(Paragraph::new(Line::from(line)).block(block), area);
}

fn render_view(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let vault_name = state
        .vault
        .as_ref()
        .and_then(|v| v.name.as_deref())
        .unwrap_or("Main");
    let vault_id = state
        .vault
        .as_ref()
        .and_then(|v| v.id.as_deref())
        .unwrap_or("-");
    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(|c| format!("{c:?}"))
        .unwrap_or_else(|| "EUR".to_string());
    let (wallets_count, flows_count) = state
        .snapshot
        .as_ref()
        .map(|snap| (snap.wallets.len(), snap.flows.len()))
        .unwrap_or((0, 0));

    let default_wallet_name = state
        .default_wallet_id
        .map(|id| resolve_wallet_name(state, id))
        .unwrap_or_else(|| "None".to_string());
    let default_flow_name = state
        .default_flow_id
        .map(|id| resolve_flow_name(state, id))
        .unwrap_or_else(|| "None".to_string());

    let lines = vec![
        Line::from(vec![
            Span::styled("Vault", Style::default().fg(theme.dim)),
            Span::raw(format!(": {vault_name}")),
        ]),
        Line::from(vec![
            Span::styled("ID", Style::default().fg(theme.dim)),
            Span::raw(format!(": {vault_id}")),
        ]),
        Line::from(vec![
            Span::styled("Currency", Style::default().fg(theme.dim)),
            Span::raw(format!(": {currency}")),
        ]),
        Line::from(vec![
            Span::styled("Wallets", Style::default().fg(theme.dim)),
            Span::raw(format!(": {wallets_count}")),
            Span::raw("   "),
            Span::styled("Flows", Style::default().fg(theme.dim)),
            Span::raw(format!(": {flows_count}")),
        ]),
        Line::from(vec![
            Span::styled("Default wallet", Style::default().fg(theme.dim)),
            Span::raw(format!(": {default_wallet_name}")),
        ]),
        Line::from(vec![
            Span::styled("Default flow", Style::default().fg(theme.dim)),
            Span::raw(format!(": {default_flow_name}")),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" create vault  "),
            Span::styled("d", Style::default().fg(theme.accent)),
            Span::raw(" defaults"),
        ]),
    ];

    let block = Block::default()
        .title("Vault Overview")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_create(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.vault_ui.form;
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Name", Style::default().fg(theme.accent)),
            Span::raw(format!(": {}", form.name)),
        ]),
        Line::from(vec![
            Span::styled("Currency", Style::default().fg(theme.dim)),
            Span::raw(": EUR"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter: create • Esc: cancel",
            Style::default().fg(theme.dim),
        )),
    ];
    if let Some(err) = form.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title("Create Vault")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_defaults(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        let block = Block::default()
            .title("Defaults")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent));
        frame.render_widget(
            Paragraph::new(Line::from("Snapshot non disponibile."))
                .alignment(Alignment::Center)
                .block(block),
            area,
        );
        return;
    };

    let wallet_names = snapshot
        .wallets
        .iter()
        .filter(|wallet| !wallet.archived)
        .map(|wallet| wallet.name.clone())
        .collect::<Vec<_>>();
    let flow_names = snapshot
        .flows
        .iter()
        .filter(|flow| !flow.archived)
        .map(|flow| flow.name.clone())
        .collect::<Vec<_>>();

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
        .constraints([Constraint::Length(6), Constraint::Min(0)])
        .split(area);

    let mut lines = vec![
        render_default_field(
            "Default wallet",
            wallet_label,
            defaults.focus == DefaultsField::Wallet,
            theme,
        ),
        render_default_field(
            "Default flow",
            flow_label,
            defaults.focus == DefaultsField::Flow,
            theme,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "Tab: next • ↑/↓: change • Enter: save • Esc: cancel",
            Style::default().fg(theme.dim),
        )),
    ];
    if let Some(err) = defaults.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title("Defaults")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);

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
        defaults.focus == DefaultsField::Wallet,
        theme,
    );
    render_defaults_list(
        frame,
        lists[1],
        "Flows",
        &flow_names,
        defaults.flow_index,
        defaults.focus == DefaultsField::Flow,
        theme,
    );
}

fn render_default_field(label: &str, value: &str, focused: bool, theme: &Theme) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };
    let value_style = if focused {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    Line::from(vec![
        Span::styled(format!("{label:<15}"), label_style),
        Span::raw(": "),
        Span::styled(value.to_string(), value_style),
    ])
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
    list_items.push(ListItem::new(Line::from("None")));
    list_items.extend(
        items
            .iter()
            .map(|name| ListItem::new(Line::from(name.clone()))),
    );

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

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let list = List::new(list_items)
        .block(block)
        .highlight_style(highlight_style)
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn resolve_wallet_name(state: &AppState, wallet_id: uuid::Uuid) -> String {
    state
        .snapshot
        .as_ref()
        .and_then(|snap| {
            snap.wallets
                .iter()
                .find(|wallet| wallet.id == wallet_id)
                .map(|wallet| wallet.name.clone())
        })
        .unwrap_or_else(|| wallet_id.to_string())
}

fn resolve_flow_name(state: &AppState, flow_id: uuid::Uuid) -> String {
    state
        .snapshot
        .as_ref()
        .and_then(|snap| {
            snap.flows
                .iter()
                .find(|flow| flow.id == flow_id)
                .map(|flow| flow.name.clone())
        })
        .unwrap_or_else(|| flow_id.to_string())
}
