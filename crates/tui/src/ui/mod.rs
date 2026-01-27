pub mod components;
pub mod forms;
pub mod keymap;
pub mod screens;

mod terminal;
mod theme;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::AppState;

pub use terminal::{AppTerminal as Terminal, restore_terminal, setup_terminal};
pub use theme::Theme;

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    let theme = Theme::default();
    frame.render_widget(
        ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(theme.background)),
        area,
    );
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
    frame.render_widget(
        ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(theme.surface)),
        content_inner,
    );

    match state.section {
        crate::app::Section::Home => screens::home::render(frame, content_inner, state),
        crate::app::Section::Transactions => {
            screens::transactions::render(frame, content_inner, state)
        }
        crate::app::Section::Wallets => screens::wallets::render(frame, content_inner, state),
        crate::app::Section::Flows => screens::accounts::render(frame, content_inner, state),
        crate::app::Section::Categories => screens::categories::render(frame, content_inner, state),
        crate::app::Section::Members => screens::members::render(frame, content_inner, state),
        crate::app::Section::Vault => screens::vault::render(frame, content_inner, state),
        crate::app::Section::Stats => screens::stats::render(frame, content_inner, state),
    }

    render_bottom_bar(frame, layout[2], state, &theme);
    components::help_overlay::render(frame, area, state);
    components::command_palette::render(frame, area, state);
    components::confirm_dialog::render(frame, area, state.overlays.confirm.as_ref());
    components::error_dialog::render(frame, area, state.overlays.error.as_ref());
    components::bulk_category_dialog::render(frame, area, state.overlays.bulk_category.as_ref());
    components::grouping_dialog::render(
        frame,
        area,
        state.overlays.grouping.as_ref(),
        state.transactions.grouping_mode,
    );
    components::toast::render(frame, area, state.toast.as_ref());
    if state.screen == crate::app::Screen::Home
        && state.snapshot.is_none()
        && state.overlays.error.is_none()
    {
        components::loading::render_fullscreen(
            frame,
            area,
            components::loading::spinner_frame(state.spinner.index()),
            "Loading...",
            Some("Fetching vault data"),
            &theme,
        );
    }
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
    if state.section == crate::app::Section::Transactions
        && state.transactions.mode == crate::app::TransactionsMode::List
        && state.transactions.visual_mode
    {
        let mut spans = Vec::new();
        let selected = state.transactions.visual_selected.len();
        spans.push(Span::styled(
            format!("VISUAL ({selected} selected)"),
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw("   "));
        spans.push(Span::styled("[Space]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            " toggle",
            Style::default().fg(theme.text_muted),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[d]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            " delete",
            Style::default().fg(theme.text_muted),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[c]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            " categorize",
            Style::default().fg(theme.text_muted),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[Esc]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(" exit", Style::default().fg(theme.text_muted)));
        frame.render_widget(Paragraph::new(Line::from(spans)), layout[0]);
    } else {
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
    }

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
            components::hints::KeyHint::new("j/k", "select"),
            components::hints::KeyHint::new("Enter", "details"),
            components::hints::KeyHint::new("t", "transactions"),
            components::hints::KeyHint::new("n", "quick add"),
        ],
        crate::app::Section::Transactions => get_transactions_hints(state),
        crate::app::Section::Wallets => get_wallets_hints(state),
        crate::app::Section::Flows => get_flows_hints(state),
        crate::app::Section::Categories => get_categories_hints(state),
        crate::app::Section::Members => get_members_hints(state),
        crate::app::Section::Vault => get_vault_hints(state),
        crate::app::Section::Stats => vec![
            components::hints::KeyHint::new("r", "refresh"),
            components::hints::KeyHint::new("←/→", "tabs"),
            components::hints::KeyHint::new("1/2/3", "views"),
            components::hints::KeyHint::new("[/]", "month"),
        ],
    }
}

fn get_transactions_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.transactions.mode {
        crate::app::TransactionsMode::List => vec![
            components::hints::KeyHint::new("n", "quick add"),
            components::hints::KeyHint::new("i", "income"),
            components::hints::KeyHint::new("e", "expense"),
            components::hints::KeyHint::new("R", "refund"),
            components::hints::KeyHint::new("/", "filters"),
            components::hints::KeyHint::new("g", "group"),
            components::hints::KeyHint::new("w", "wallet scope"),
            components::hints::KeyHint::new("f", "flow scope"),
            components::hints::KeyHint::new("c", "clear"),
            components::hints::KeyHint::new("d", "delete"),
            components::hints::KeyHint::new("u", "undo"),
            components::hints::KeyHint::new("v", "visual"),
        ]
        .into_iter()
        .chain(components::hints::common::list_navigation())
        .collect(),
        crate::app::TransactionsMode::Detail => {
            let mut hints = components::hints::common::detail_view();
            hints.push(components::hints::KeyHint::new("e", "edit"));
            hints.push(components::hints::KeyHint::new("d", "delete"));
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
            hints.push(components::hints::KeyHint::new("d", "delete"));
            hints
        }
        crate::app::WalletsMode::Detail => components::hints::common::detail_view(),
        crate::app::WalletsMode::Create | crate::app::WalletsMode::Rename => {
            components::hints::common::form_editing()
        }
    }
}

fn get_members_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.members.mode {
        crate::app::MembersMode::List => {
            let mut hints = components::hints::common::list_navigation();
            hints.push(components::hints::KeyHint::new("a", "add"));
            hints.push(components::hints::KeyHint::new("e", "edit"));
            hints.push(components::hints::KeyHint::new("x", "remove"));
            hints.push(components::hints::KeyHint::new("v/f", "scope"));
            if state.members.scope == crate::app::MembersScope::Flow {
                hints.push(components::hints::KeyHint::new("[/]", "flow"));
            }
            hints
        }
        crate::app::MembersMode::Form => vec![
            components::hints::KeyHint::new("Tab", "next"),
            components::hints::KeyHint::new("↑/↓", "role"),
            components::hints::KeyHint::new("Enter", "save"),
            components::hints::KeyHint::new("Esc", "cancel"),
        ],
    }
}

fn get_flows_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let mut hints = vec![
        components::hints::KeyHint::new("←/→", "tabs"),
        components::hints::KeyHint::new("1/2/3", "jump"),
    ];

    let mut tab_hints = match state.accounts_tab {
        crate::app::AccountsTab::Sources => match state.wallets.mode {
            crate::app::WalletsMode::List => {
                let mut hints = components::hints::common::list_navigation();
                hints.push(components::hints::KeyHint::new("c", "create"));
                hints.push(components::hints::KeyHint::new("e", "rename"));
                hints.push(components::hints::KeyHint::new("d", "delete"));
                hints
            }
            crate::app::WalletsMode::Detail => components::hints::common::detail_view(),
            crate::app::WalletsMode::Create | crate::app::WalletsMode::Rename => {
                components::hints::common::form_editing()
            }
        },
        crate::app::AccountsTab::Envelopes => match state.flows.mode {
            crate::app::FlowsMode::List => {
                let mut hints = components::hints::common::list_navigation();
                hints.push(components::hints::KeyHint::new("c", "create"));
                hints.push(components::hints::KeyHint::new("e", "rename"));
                hints.push(components::hints::KeyHint::new("d", "delete"));
                hints
            }
            crate::app::FlowsMode::Detail => components::hints::common::detail_view(),
            crate::app::FlowsMode::Create | crate::app::FlowsMode::Rename => {
                let mut hints = components::hints::common::form_editing();
                hints.insert(1, components::hints::KeyHint::new("m", "mode"));
                hints
            }
        },
        crate::app::AccountsTab::Goals => vec![components::hints::KeyHint::new("r", "refresh")],
    };

    hints.append(&mut tab_hints);
    hints
}

fn get_categories_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.categories.mode {
        crate::app::CategoriesMode::List => {
            vec![
                components::hints::KeyHint::new("↑↓", "select"),
                components::hints::KeyHint::new("c", "create"),
                components::hints::KeyHint::new("e", "rename"),
                components::hints::KeyHint::new("d", "delete"),
                components::hints::KeyHint::new("l", "aliases"),
                components::hints::KeyHint::new("m", "merge"),
                components::hints::KeyHint::new("r", "refresh"),
            ]
        }
        crate::app::CategoriesMode::Merge => vec![
            components::hints::KeyHint::new("Enter", "preview/merge"),
            components::hints::KeyHint::new("Esc", "cancel"),
        ],
        crate::app::CategoriesMode::Create | crate::app::CategoriesMode::Rename => vec![
            components::hints::KeyHint::new("Enter", "save"),
            components::hints::KeyHint::new("Esc", "cancel"),
        ],
        crate::app::CategoriesMode::Aliases => vec![
            components::hints::KeyHint::new("Tab", "focus"),
            components::hints::KeyHint::new("Enter", "save"),
            components::hints::KeyHint::new("x", "delete"),
            components::hints::KeyHint::new("Esc", "back"),
        ],
    }
}

fn get_vault_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.vault_ui.mode {
        crate::app::VaultMode::View => {
            vec![
                components::hints::KeyHint::new("c", "create"),
                components::hints::KeyHint::new("d", "defaults"),
                components::hints::KeyHint::new("l", "vaults"),
                components::hints::KeyHint::new("x", "delete"),
            ]
        }
        crate::app::VaultMode::Create => components::hints::common::form_editing(),
        crate::app::VaultMode::Defaults => {
            let mut hints = components::hints::common::form_editing();
            hints.insert(1, components::hints::KeyHint::new("↑/↓", "change"));
            hints
        }
        crate::app::VaultMode::Select => vec![
            components::hints::KeyHint::new("↑/↓", "select"),
            components::hints::KeyHint::new("Enter", "open"),
            components::hints::KeyHint::new("Esc", "back"),
        ],
    }
}
