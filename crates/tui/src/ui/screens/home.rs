use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{app::AppState, ui::theme::Theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let block = Block::default()
        .title("Home")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));

    let vault_name = state
        .vault
        .as_ref()
        .and_then(|v| v.name.as_deref())
        .unwrap_or("Main");
    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(|c| format!("{c:?}"))
        .unwrap_or_else(|| "EUR".to_string());

    let (wallets_count, flows_count, unallocated_name) = state
        .snapshot
        .as_ref()
        .map(|snap| {
            let unallocated = snap
                .flows
                .iter()
                .find(|flow| flow.is_unallocated)
                .map(|flow| flow.name.as_str())
                .unwrap_or("Non in flow");
            (snap.wallets.len(), snap.flows.len(), unallocated)
        })
        .unwrap_or((0, 0, "Non in flow"));

    let content = vec![
        Line::from(vec![
            Span::styled(
                "Sparagne",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" • "),
            Span::raw(format!("Vault: {vault_name}")),
        ]),
        Line::from(vec![Span::raw(format!("Currency: {currency}"))]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Wallets", Style::default().fg(theme.dim)),
            Span::raw(format!(": {wallets_count}")),
            Span::raw("   "),
            Span::styled("Flows", Style::default().fg(theme.dim)),
            Span::raw(format!(": {flows_count}")),
        ]),
        Line::from(vec![
            Span::styled("Unallocated", Style::default().fg(theme.dim)),
            Span::raw(format!(": {unallocated_name}")),
        ]),
    ];

    let content = Paragraph::new(content)
        .alignment(Alignment::Left)
        .block(block);
    frame.render_widget(content, area);
}
