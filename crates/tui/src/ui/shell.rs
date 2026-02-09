use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{AppState, Section, SettingsTab},
    text::{TextKey, t},
    ui::{Theme, components, screens},
};

pub(crate) fn render_shell(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Main layout: header, content, bottom bar
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header (tabs + status)
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Bottom bar
        ])
        .split(area);

    render_header(frame, layout[0], state, theme);

    // Content area
    let content_inner = layout[1];
    frame.render_widget(
        ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(theme.background)),
        content_inner,
    );

    match state.section {
        Section::Home => screens::home::render(frame, content_inner, state, theme),
        Section::Transactions => {
            if state.transactions.recurring_mode {
                screens::recurring::render(frame, content_inner, state, theme);
            } else {
                screens::transactions::render(frame, content_inner, state, theme);
            }
        }
        Section::Accounts => screens::accounts::render(frame, content_inner, state, theme),
        Section::Analytics => screens::analytics::render(frame, content_inner, state, theme),
        Section::Settings => screens::settings::render(frame, content_inner, state, theme),
    }

    render_bottom_bar(frame, layout[2], state, theme);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // Pass sub-tabs for breadcrumb display
    let settings_tab = if state.section == Section::Settings {
        Some(state.settings_tab)
    } else {
        None
    };

    components::tabs::render_tabs(
        frame,
        layout[0],
        state.section,
        settings_tab,
        state.locale,
        theme,
    );
    render_status_bar(frame, layout[1], state, theme);
}

fn render_status_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;
    let vault = state
        .vault
        .as_ref()
        .and_then(|v| v.name.as_deref())
        .unwrap_or(t(locale, TextKey::ShellVaultFallback));
    let user = state.login.username.as_str();
    let line = Line::from(vec![
        Span::styled(
            t(locale, TextKey::ShellVaultLabel),
            Style::default().fg(theme.text_muted),
        ),
        Span::raw(format!(": {vault} | ")),
        Span::styled(
            t(locale, TextKey::ShellUserLabel),
            Style::default().fg(theme.text_muted),
        ),
        Span::raw(format!(": {user}")),
    ]);

    frame.render_widget(
        Paragraph::new(line).alignment(ratatui::layout::Alignment::Right),
        area,
    );
}

fn render_bottom_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    // Fill footer background to match the overall theme.
    frame.render_widget(
        ratatui::widgets::Block::default().style(
            ratatui::style::Style::default()
                .bg(theme.background)
                .fg(theme.text),
        ),
        area,
    );
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(area);

    // Left: shortcuts + context hints
    let locale = state.locale;
    if state.section == Section::Transactions
        && state.transactions.mode == crate::app::TransactionsMode::List
        && state.transactions.visual_mode
    {
        let mut spans = Vec::new();
        let selected = state.transactions.visual_selected.len();
        spans.push(Span::styled(
            format!("VISUAL ({selected} selected)"),
            Style::default()
                .fg(theme.background)
                .bg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[Space]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            format!(" {}", t(locale, TextKey::HintToggle)),
            Style::default().fg(theme.text_muted),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[d]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            format!(" {}", t(locale, TextKey::HintDelete)),
            Style::default().fg(theme.text_muted),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[c]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            format!(" {}", t(locale, TextKey::HintCategorize)),
            Style::default().fg(theme.text_muted),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled("[Esc]", Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            format!(" {}", t(locale, TextKey::HintExit)),
            Style::default().fg(theme.text_muted),
        ));
        frame.render_widget(Paragraph::new(Line::from(spans)), layout[0]);
    } else {
        let mut parts = Vec::new();
        parts.extend(components::hints::hints_to_spans(
            &components::hints::common::section_shortcuts(locale),
            theme,
        ));

        let context_hints = get_context_hints(state);
        if !context_hints.is_empty() {
            parts.push(components::hints::hint_separator(theme));
            parts.extend(components::hints::hints_to_spans(&context_hints, theme));
        }
        parts.push(components::hints::hint_separator(theme));
        parts.extend(components::hints::hints_to_spans(
            &[components::hints::help_hint(locale)],
            theme,
        ));

        frame.render_widget(Paragraph::new(Line::from(parts)), layout[0]);
    }

    // Right: refresh status
    let refresh = state
        .last_refresh
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| "-".to_string());
    let status = if state.connection.ok {
        t(locale, TextKey::StatusOnline)
    } else {
        t(locale, TextKey::StatusOffline)
    };
    let status_style = if state.connection.ok {
        Style::default().fg(theme.positive)
    } else {
        Style::default()
            .fg(theme.negative)
            .add_modifier(Modifier::BOLD)
    };
    let conn_msg = state.connection.message.as_deref().unwrap_or_default();
    let right_line = Line::from(vec![
        Span::styled("⟳", Style::default().fg(theme.text_muted)),
        Span::raw(" "),
        Span::styled(refresh, Style::default().fg(theme.text)),
        Span::raw(" "),
        Span::styled("│", Style::default().fg(theme.border)),
        Span::raw(" "),
        Span::styled(status, status_style),
        if conn_msg.is_empty() {
            Span::raw("")
        } else {
            Span::styled(
                format!(" · {conn_msg}"),
                Style::default().fg(theme.text_muted),
            )
        },
    ]);
    frame.render_widget(
        Paragraph::new(right_line).alignment(ratatui::layout::Alignment::Right),
        layout[1],
    );
}

/// Returns context-specific keyboard hints based on current section and mode.
/// Limited to 1-2 most important hints per context.
fn get_context_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let locale = state.locale;
    match state.section {
        Section::Home => vec![components::hints::common::quick_add(locale)],
        Section::Transactions => get_transactions_hints(state),
        Section::Accounts => get_accounts_hints(state),
        Section::Analytics => vec![components::hints::KeyHint::new(
            "r",
            t(locale, TextKey::HintRefresh),
        )],
        Section::Settings => get_settings_hints(state),
    }
}

fn get_transactions_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let locale = state.locale;
    if state.transactions.recurring_mode {
        return get_recurring_hints(state);
    }
    match state.transactions.mode {
        crate::app::TransactionsMode::List => vec![components::hints::common::quick_add(locale)],
        crate::app::TransactionsMode::Detail => vec![
            components::hints::KeyHint::new("e", t(locale, TextKey::HintEdit)),
            components::hints::KeyHint::new("d", t(locale, TextKey::HintDelete)),
        ],
        crate::app::TransactionsMode::PickWallet
        | crate::app::TransactionsMode::PickFlow
        | crate::app::TransactionsMode::TransferPicker
        | crate::app::TransactionsMode::TransferWallet
        | crate::app::TransactionsMode::TransferFlow
        | crate::app::TransactionsMode::Filter => components::hints::common::form_editing(locale),
        crate::app::TransactionsMode::Form | crate::app::TransactionsMode::Edit => {
            components::hints::common::form_editing(locale)
        }
    }
}

fn get_accounts_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let locale = state.locale;
    match state.accounts_tab {
        crate::app::AccountsTab::Wallets => match state.wallets.mode {
            crate::app::EntityListMode::List => {
                vec![components::hints::KeyHint::new(
                    "c",
                    t(locale, TextKey::HintCreate),
                )]
            }
            crate::app::EntityListMode::Detail => components::hints::common::detail_view(locale),
            crate::app::EntityListMode::Create | crate::app::EntityListMode::Rename => {
                components::hints::common::form_editing(locale)
            }
        },
        crate::app::AccountsTab::Budget => match state.flows.mode {
            crate::app::EntityListMode::List => {
                vec![components::hints::KeyHint::new(
                    "c",
                    t(locale, TextKey::HintCreate),
                )]
            }
            crate::app::EntityListMode::Detail => components::hints::common::detail_view(locale),
            crate::app::EntityListMode::Create | crate::app::EntityListMode::Rename => {
                components::hints::common::form_editing(locale)
            }
        },
    }
}

fn get_settings_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    match state.settings_tab {
        SettingsTab::Categories => get_categories_hints(state),
        SettingsTab::Vault => get_vault_hints(state),
        SettingsTab::Members => get_members_hints(state),
        SettingsTab::Preferences => get_preferences_hints(state),
    }
}

fn get_preferences_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let locale = state.locale;
    vec![
        components::hints::KeyHint::new("Space", t(locale, TextKey::HintToggle)),
        components::hints::KeyHint::new("Esc", t(locale, TextKey::HintBack)),
    ]
}

fn get_categories_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let locale = state.locale;
    match state.categories.mode {
        crate::app::CategoriesMode::List => {
            vec![components::hints::KeyHint::new(
                "c",
                t(locale, TextKey::HintCreate),
            )]
        }
        crate::app::CategoriesMode::Merge
        | crate::app::CategoriesMode::Create
        | crate::app::CategoriesMode::Rename
        | crate::app::CategoriesMode::Aliases => components::hints::common::form_editing(locale),
    }
}

fn get_vault_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let locale = state.locale;
    match state.vault_ui.mode {
        crate::app::VaultMode::View => {
            vec![components::hints::KeyHint::new(
                "c",
                t(locale, TextKey::HintCreate),
            )]
        }
        crate::app::VaultMode::Create
        | crate::app::VaultMode::Defaults
        | crate::app::VaultMode::Select => components::hints::common::form_editing(locale),
    }
}

fn get_members_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let locale = state.locale;
    match state.members.mode {
        crate::app::MembersMode::List => {
            vec![components::hints::KeyHint::new(
                "a",
                t(locale, TextKey::HintAdd),
            )]
        }
        crate::app::MembersMode::Form => components::hints::common::form_editing(locale),
    }
}

fn get_recurring_hints(state: &AppState) -> Vec<components::hints::KeyHint> {
    let locale = state.locale;
    match state.recurring.mode {
        crate::app::RecurringMode::List => {
            vec![
                components::hints::KeyHint::new("c", t(locale, TextKey::HintCreate)),
                components::hints::KeyHint::new("e", t(locale, TextKey::HintEdit)),
                components::hints::KeyHint::new("Esc", t(locale, TextKey::HintBack)),
            ]
        }
        crate::app::RecurringMode::Create | crate::app::RecurringMode::Edit => {
            components::hints::common::form_editing(locale)
        }
    }
}
