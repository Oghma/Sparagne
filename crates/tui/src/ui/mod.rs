pub mod components;
pub mod keymap;
pub mod screens;

mod terminal;
mod theme;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::AppState;

pub use terminal::{AppTerminal as Terminal, restore_terminal, setup_terminal};
pub use theme::Theme;

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    match state.screen {
        crate::app::Screen::Login => screens::login::render(frame, area, state),
        crate::app::Screen::Home => render_shell(frame, area, state),
    }
}

fn render_shell(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    // Main layout: header, content, bottom bar
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header (tabs + status)
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Bottom bar
        ])
        .split(area);

    render_header(frame, layout[0], state, &theme);

    // Content area
    let content_inner = layout[1];

    match state.section {
        crate::app::Section::Home => screens::home::render(frame, content_inner, state),
        crate::app::Section::Transactions => {
            screens::transactions::render(frame, content_inner, state)
        }
        crate::app::Section::Wallets => screens::wallets::render(frame, content_inner, state),
        crate::app::Section::Flows => screens::flows::render(frame, content_inner, state),
        crate::app::Section::Vault => screens::vault::render(frame, content_inner, state),
        crate::app::Section::Stats => screens::stats::render(frame, content_inner, state),
    }

    render_bottom_bar(frame, layout[2], state, &theme);
    components::command_palette::render(frame, area, state);
    components::help_overlay::render(frame, area, state);
    components::toast::render(frame, area, state.toast.as_ref());
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    components::tabs::render_tabs(frame, layout[0], state.section, theme);
    render_status_bar(frame, layout[1], state, theme);
}

fn render_status_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let vault = state
        .vault
        .as_ref()
        .and_then(|v| v.name.as_deref())
        .unwrap_or("Main");
    let user = state.login.username.as_str();
    let line = Line::from(vec![
        Span::styled("Vault", Style::default().fg(theme.text_muted)),
        Span::raw(format!(": {vault} | ")),
        Span::styled("User", Style::default().fg(theme.text_muted)),
        Span::raw(format!(": {user}")),
    ]);

    frame.render_widget(
        Paragraph::new(line).alignment(ratatui::layout::Alignment::Right),
        area,
    );
}

fn render_bottom_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Left: shortcuts + context hints
    let mut parts = components::tabs::tab_shortcuts(theme);

    parts.push(Span::styled("  │  ", Style::default().fg(theme.border)));
    parts.push(Span::styled("Ctrl+P", Style::default().fg(theme.accent)));
    parts.push(Span::raw(" cmd"));

    let context_hints = get_context_hints(state, theme);
    if !context_hints.is_empty() {
        parts.push(Span::styled("  │  ", Style::default().fg(theme.border)));
        parts.extend(context_hints);
    }

    parts.push(Span::styled("  │  ", Style::default().fg(theme.border)));
    parts.push(Span::styled("q", Style::default().fg(theme.accent)));
    parts.push(Span::raw(" quit"));

    frame.render_widget(Paragraph::new(Line::from(parts)), layout[0]);

    // Right: refresh status
    let refresh = state
        .last_refresh
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| "-".to_string());
    let status = if state.connection.ok { "OK" } else { "ERR" };
    let status_style = if state.connection.ok {
        Style::default().fg(theme.positive)
    } else {
        Style::default().fg(theme.error)
    };
    let right_line = Line::from(vec![
        Span::styled("⟳", Style::default().fg(theme.text_muted)),
        Span::raw(format!(" {refresh} ")),
        Span::styled(status, status_style),
    ]);
    frame.render_widget(
        Paragraph::new(right_line).alignment(ratatui::layout::Alignment::Right),
        layout[1],
    );
}

/// Returns context-specific keyboard hints based on current section and mode.
fn get_context_hints(state: &AppState, theme: &Theme) -> Vec<Span<'static>> {
    match state.section {
        crate::app::Section::Home => vec![
            Span::styled("t", Style::default().fg(theme.accent)),
            Span::raw(" transactions  "),
            Span::styled("w", Style::default().fg(theme.accent)),
            Span::raw(" wallets  "),
            Span::styled("f", Style::default().fg(theme.accent)),
            Span::raw(" flows"),
        ],
        crate::app::Section::Transactions => get_transactions_hints(state, theme),
        crate::app::Section::Wallets => get_wallets_hints(state, theme),
        crate::app::Section::Flows => get_flows_hints(state, theme),
        crate::app::Section::Vault => get_vault_hints(state, theme),
        crate::app::Section::Stats => vec![
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(" refresh"),
        ],
    }
}

fn get_transactions_hints(state: &AppState, theme: &Theme) -> Vec<Span<'static>> {
    match state.transactions.mode {
        crate::app::TransactionsMode::List => vec![
            Span::styled("a", Style::default().fg(theme.accent)),
            Span::raw(" quick add  "),
            Span::styled("i", Style::default().fg(theme.accent)),
            Span::raw(" income  "),
            Span::styled("e", Style::default().fg(theme.accent)),
            Span::raw(" expense  "),
            Span::styled("R", Style::default().fg(theme.accent)),
            Span::raw(" refund  "),
            Span::styled("/", Style::default().fg(theme.accent)),
            Span::raw(" filters  "),
            Span::styled("w", Style::default().fg(theme.accent)),
            Span::raw(" wallet scope  "),
            Span::styled("f", Style::default().fg(theme.accent)),
            Span::raw(" flow scope  "),
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" clear  "),
            Span::styled("u", Style::default().fg(theme.accent)),
            Span::raw(" undo  "),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" detail"),
        ],
        crate::app::TransactionsMode::Detail => vec![
            Span::styled("b", Style::default().fg(theme.accent)),
            Span::raw(" back  "),
            Span::styled("e", Style::default().fg(theme.accent)),
            Span::raw(" edit  "),
            Span::styled("v", Style::default().fg(theme.accent)),
            Span::raw(" void  "),
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(" repeat"),
        ],
        crate::app::TransactionsMode::PickWallet | crate::app::TransactionsMode::PickFlow => vec![
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::raw(" cancel"),
        ],
        crate::app::TransactionsMode::TransferWallet
        | crate::app::TransactionsMode::TransferFlow
        | crate::app::TransactionsMode::Filter => vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(" next  "),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" apply  "),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::raw(" cancel"),
        ],
        crate::app::TransactionsMode::Form | crate::app::TransactionsMode::Edit => vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(" next  "),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::raw(" cancel"),
        ],
    }
}

fn get_wallets_hints(state: &AppState, theme: &Theme) -> Vec<Span<'static>> {
    match state.wallets.mode {
        crate::app::WalletsMode::List => vec![
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" create  "),
            Span::styled("e", Style::default().fg(theme.accent)),
            Span::raw(" rename  "),
            Span::styled("a", Style::default().fg(theme.accent)),
            Span::raw(" archive  "),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" detail"),
        ],
        crate::app::WalletsMode::Detail => vec![
            Span::styled("b", Style::default().fg(theme.accent)),
            Span::raw(" back"),
        ],
        crate::app::WalletsMode::Create | crate::app::WalletsMode::Rename => vec![
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" save  "),
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(" next  "),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::raw(" cancel"),
        ],
    }
}

fn get_flows_hints(state: &AppState, theme: &Theme) -> Vec<Span<'static>> {
    match state.flows.mode {
        crate::app::FlowsMode::List => vec![
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" create  "),
            Span::styled("e", Style::default().fg(theme.accent)),
            Span::raw(" rename  "),
            Span::styled("a", Style::default().fg(theme.accent)),
            Span::raw(" archive  "),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" detail"),
        ],
        crate::app::FlowsMode::Detail => vec![
            Span::styled("b", Style::default().fg(theme.accent)),
            Span::raw(" back"),
        ],
        crate::app::FlowsMode::Create | crate::app::FlowsMode::Rename => vec![
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" save  "),
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(" next  "),
            Span::styled("m", Style::default().fg(theme.accent)),
            Span::raw(" mode  "),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::raw(" cancel"),
        ],
    }
}

fn get_vault_hints(state: &AppState, theme: &Theme) -> Vec<Span<'static>> {
    match state.vault_ui.mode {
        crate::app::VaultMode::View => vec![
            Span::styled("c", Style::default().fg(theme.accent)),
            Span::raw(" create  "),
            Span::styled("d", Style::default().fg(theme.accent)),
            Span::raw(" defaults"),
        ],
        crate::app::VaultMode::Create => vec![
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::raw(" cancel"),
        ],
        crate::app::VaultMode::Defaults => vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(" next  "),
            Span::styled("↑/↓", Style::default().fg(theme.accent)),
            Span::raw(" change  "),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::raw(" cancel"),
        ],
    }
}
