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
    let mut parts = Vec::new();
    parts.extend(components::hints::hints_to_spans(
        &components::hints::common::section_shortcuts(),
        theme,
    ));
    parts.push(components::hints::hint_separator(theme));
    parts.extend(components::hints::hints_to_spans(
        &components::hints::common::global_shortcuts(),
        theme,
    ));

    let context_hints = get_context_hints(state);
    if !context_hints.is_empty() {
        parts.push(components::hints::hint_separator(theme));
        parts.extend(components::hints::hints_to_spans(&context_hints, theme));
    }

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
fn get_context_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.section {
        crate::app::Section::Home => vec![
            components::hints::KeyHint::new("a", "add expense"),
            components::hints::KeyHint::new("i", "add income"),
            components::hints::KeyHint::new("r", "refresh"),
        ],
        crate::app::Section::Transactions => get_transactions_hints(state),
        crate::app::Section::Wallets => get_wallets_hints(state),
        crate::app::Section::Flows => get_flows_hints(state),
        crate::app::Section::Vault => get_vault_hints(state),
        crate::app::Section::Stats => vec![
            components::hints::KeyHint::new("r", "refresh"),
            components::hints::KeyHint::new("←/→", "month"),
        ],
    }
}

fn get_transactions_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.transactions.mode {
        crate::app::TransactionsMode::List => vec![
            components::hints::KeyHint::new("a", "quick add"),
            components::hints::KeyHint::new("i", "income"),
            components::hints::KeyHint::new("e", "expense"),
            components::hints::KeyHint::new("R", "refund"),
            components::hints::KeyHint::new("/", "filters"),
            components::hints::KeyHint::new("w", "wallet scope"),
            components::hints::KeyHint::new("f", "flow scope"),
            components::hints::KeyHint::new("c", "clear"),
            components::hints::KeyHint::new("u", "undo"),
        ]
        .into_iter()
        .chain(components::hints::common::list_navigation())
        .collect(),
        crate::app::TransactionsMode::Detail => {
            let mut hints = components::hints::common::detail_view();
            hints.push(components::hints::KeyHint::new("e", "edit"));
            hints.push(components::hints::KeyHint::new("v", "void"));
            hints.push(components::hints::KeyHint::new("r", "repeat"));
            hints
        }
        crate::app::TransactionsMode::PickWallet | crate::app::TransactionsMode::PickFlow => vec![
            components::hints::KeyHint::new("Enter", "save"),
            components::hints::KeyHint::new("Esc", "cancel"),
        ],
        crate::app::TransactionsMode::TransferWallet
        | crate::app::TransactionsMode::TransferFlow
        | crate::app::TransactionsMode::Filter => vec![
            components::hints::KeyHint::new("Tab", "next"),
            components::hints::KeyHint::new("Enter", "apply"),
            components::hints::KeyHint::new("Esc", "cancel"),
        ],
        crate::app::TransactionsMode::Form | crate::app::TransactionsMode::Edit => {
            components::hints::common::form_editing()
        }
    }
}

fn get_wallets_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.wallets.mode {
        crate::app::WalletsMode::List => {
            let mut hints = components::hints::common::list_navigation();
            hints.push(components::hints::KeyHint::new("c", "create"));
            hints.push(components::hints::KeyHint::new("e", "rename"));
            hints.push(components::hints::KeyHint::new("a", "archive"));
            hints
        }
        crate::app::WalletsMode::Detail => components::hints::common::detail_view(),
        crate::app::WalletsMode::Create | crate::app::WalletsMode::Rename => {
            components::hints::common::form_editing()
        }
    }
}

fn get_flows_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.flows.mode {
        crate::app::FlowsMode::List => {
            let mut hints = components::hints::common::list_navigation();
            hints.push(components::hints::KeyHint::new("c", "create"));
            hints.push(components::hints::KeyHint::new("e", "rename"));
            hints.push(components::hints::KeyHint::new("a", "archive"));
            hints
        }
        crate::app::FlowsMode::Detail => components::hints::common::detail_view(),
        crate::app::FlowsMode::Create | crate::app::FlowsMode::Rename => {
            let mut hints = components::hints::common::form_editing();
            hints.insert(1, components::hints::KeyHint::new("m", "mode"));
            hints
        }
    }
}

fn get_vault_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.vault_ui.mode {
        crate::app::VaultMode::View => vec![
            components::hints::KeyHint::new("c", "create"),
            components::hints::KeyHint::new("d", "defaults"),
        ],
        crate::app::VaultMode::Create => components::hints::common::form_editing(),
        crate::app::VaultMode::Defaults => {
            let mut hints = components::hints::common::form_editing();
            hints.insert(1, components::hints::KeyHint::new("↑/↓", "change"));
            hints
        }
    }
}
