use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::AppState,
    text::{TextKey, t},
    ui::{
        common::{resolve_flow_name, resolve_wallet_name, themed_block},
        theme::Theme,
    },
};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;
    let vault_name = display_vault_name(state)
        .unwrap_or_else(|| t(locale, TextKey::VaultDefaultName).to_string());
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

    let none_label = t(locale, TextKey::UiNone);
    let default_wallet_name = state
        .default_wallet_id
        .map(|id| resolve_wallet_name(state, id))
        .unwrap_or_else(|| none_label.to_string());
    let default_flow_name = state
        .default_flow_id
        .map(|id| resolve_flow_name(state, id))
        .unwrap_or_else(|| none_label.to_string());

    let block = themed_block(&format!("🏦 {vault_name}"), theme.border_focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let info_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // ID
            Constraint::Length(1), // Currency
            Constraint::Length(1), // Wallets/Flows
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Defaults header
            Constraint::Length(1), // Default wallet
            Constraint::Length(1), // Default flow
            Constraint::Min(0),    // Error/confirmation
        ])
        .split(inner);

    // Vault ID
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<14}", t(locale, TextKey::VaultIdLabel)),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(vault_id.to_string(), Style::default().fg(theme.text)),
        ])),
        info_layout[0],
    );

    // Currency
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<14}", t(locale, TextKey::VaultCurrencyLabel)),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(currency, Style::default().fg(theme.text)),
        ])),
        info_layout[1],
    );

    // Wallets and Flows count
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<14}", t(locale, TextKey::SectionWallets)),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(wallets_count.to_string(), Style::default().fg(theme.text)),
            Span::raw("    "),
            Span::styled(
                format!("{:<7}", t(locale, TextKey::SectionFlows)),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(flows_count.to_string(), Style::default().fg(theme.text)),
        ])),
        info_layout[2],
    );

    // Defaults header
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {}", t(locale, TextKey::VaultQuickDefaults)),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )])),
        info_layout[4],
    );

    // Default wallet
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {}  ", t(locale, TextKey::VaultDefaultWallet)),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(
                default_wallet_name,
                if state.default_wallet_id.is_some() {
                    Style::default().fg(theme.text)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
        ])),
        info_layout[5],
    );

    // Default flow
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {}  ", t(locale, TextKey::VaultDefaultFlow)),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(
                default_flow_name,
                if state.default_flow_id.is_some() {
                    Style::default().fg(theme.text)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
        ])),
        info_layout[6],
    );

    // Error or confirmation
    if let Some(err) = state.vault_ui.error.as_ref() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ✗ ", Style::default().fg(theme.negative)),
                Span::styled(err.clone(), Style::default().fg(theme.negative)),
            ])),
            info_layout[7],
        );
    }
}

fn display_vault_name(state: &AppState) -> Option<String> {
    let vault = state.vault.as_ref()?;
    let name = vault.name.as_deref()?;
    let owner = vault.owner.as_deref();
    let username = state.login.username.trim();

    match owner {
        Some(owner) if !owner.is_empty() && owner != username => Some(format!("{name} ({owner})")),
        _ => Some(name.to_string()),
    }
}
