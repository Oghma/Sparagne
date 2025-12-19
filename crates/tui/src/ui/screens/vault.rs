use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    app::{AppState, VaultMode},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state, &theme);

    match state.vault_ui.mode {
        VaultMode::View => render_view(frame, layout[1], state, &theme),
        VaultMode::Create => render_create(frame, layout[1], state, &theme),
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mode = match state.vault_ui.mode {
        VaultMode::View => "View",
        VaultMode::Create => "Create",
    };
    let mut line = vec![
        Span::styled("Mode", Style::default().fg(theme.dim)),
        Span::raw(format!(": {mode}")),
    ];
    if let Some(err) = state.vault_ui.error.as_ref() {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }
    let block = Block::default().borders(Borders::ALL).title("Vault");
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
        Line::from(""),
        Line::from(vec![
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" create vault"),
        ]),
    ];

    let block = Block::default()
        .title("Vault Overview")
        .borders(Borders::ALL)
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
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
