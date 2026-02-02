use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    app::{AccountsTab, AppState, Section, SettingsTab, TransactionsMode},
    text::{Locale, TextKey, t},
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if !state.help.active {
        return;
    }

    let theme = Theme::default();
    let locale = state.locale;
    let popup = centered_rect(75, 70, area);

    // Clear the background
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(2), // Footer
        ])
        .split(popup);

    render_header(frame, layout[0], state, locale, &theme);
    render_content(frame, layout[1], state, locale, &theme);
    render_footer(frame, layout[2], locale, &theme);
}

fn render_header(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    locale: Locale,
    theme: &Theme,
) {
    let section_name = match state.section {
        Section::Home => t(locale, TextKey::SectionHome).to_string(),
        Section::Transactions => t(locale, TextKey::SectionTransactions).to_string(),
        Section::Accounts => match state.accounts_tab {
            AccountsTab::Sources => format!(
                "{} > {}",
                t(locale, TextKey::HintAccounts),
                t(locale, TextKey::HelpSourcesWallets)
            ),
            AccountsTab::Envelopes => format!(
                "{} > {}",
                t(locale, TextKey::HintAccounts),
                t(locale, TextKey::HelpEnvelopesFlows)
            ),
            AccountsTab::Goals => format!(
                "{} > {}",
                t(locale, TextKey::HintAccounts),
                t(locale, TextKey::HelpGoals)
            ),
        },
        Section::Analytics => t(locale, TextKey::HintAnalytics).to_string(),
        Section::Settings => match state.settings_tab {
            SettingsTab::Categories => format!(
                "{} > {}",
                t(locale, TextKey::HintSettings),
                t(locale, TextKey::SectionCategories)
            ),
            SettingsTab::Vault => format!(
                "{} > {}",
                t(locale, TextKey::HintSettings),
                t(locale, TextKey::SectionVault)
            ),
            SettingsTab::Members => format!(
                "{} > {}",
                t(locale, TextKey::HintSettings),
                t(locale, TextKey::SectionMembers)
            ),
            SettingsTab::Preferences => format!(
                "{} > {}",
                t(locale, TextKey::HintSettings),
                t(locale, TextKey::SectionPreferences)
            ),
        },
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {}  ", t(locale, TextKey::HelpTitle)),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("─ ", Style::default().fg(theme.border)),
            Span::styled(section_name, Style::default().fg(theme.accent)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_content(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    locale: Locale,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Two-column layout
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let left_lines = global_shortcuts(locale, theme);
    let right_lines = context_shortcuts(state, locale, theme);

    frame.render_widget(Paragraph::new(left_lines), columns[0]);
    frame.render_widget(Paragraph::new(right_lines), columns[1]);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, locale: Locale, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let line = Line::from(vec![
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(
            format!(" {}", t(locale, TextKey::HelpCloseHelp)),
            Style::default().fg(theme.text_muted),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}

fn global_shortcuts(locale: Locale, theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header(t(locale, TextKey::HelpGlobal), theme),
        Line::from(""),
        shortcut_line("n", t(locale, TextKey::HelpQuickAddTxn), theme),
        shortcut_line("N", t(locale, TextKey::HelpNewTxnModal), theme),
        shortcut_line("Ctrl+P", t(locale, TextKey::HelpCommandPalette), theme),
        shortcut_line("Ctrl+F", t(locale, TextKey::HelpSearch), theme),
        shortcut_line("?", t(locale, TextKey::HelpShowHelp), theme),
        Line::from(""),
        section_header(t(locale, TextKey::HelpNavigation), theme),
        Line::from(""),
        shortcut_line("h", t(locale, TextKey::SectionHome), theme),
        shortcut_line("t", t(locale, TextKey::SectionTransactions), theme),
        shortcut_line("a", t(locale, TextKey::HintAccounts), theme),
        shortcut_line("y", t(locale, TextKey::HintAnalytics), theme),
        shortcut_line("s", t(locale, TextKey::HintSettings), theme),
        shortcut_line("Tab", t(locale, TextKey::HelpNextSubTab), theme),
        shortcut_line("Shift+Tab", t(locale, TextKey::HelpPrevSubTab), theme),
        shortcut_line("↑/↓ j/k", t(locale, TextKey::HelpNavigateList), theme),
        shortcut_line("Enter", t(locale, TextKey::HelpOpenDetails), theme),
        shortcut_line("Esc", t(locale, TextKey::HelpBackClose), theme),
        Line::from(""),
        section_header(t(locale, TextKey::HelpCommonActions), theme),
        Line::from(""),
        shortcut_line("e", t(locale, TextKey::HelpEditSelected), theme),
        shortcut_line("d", t(locale, TextKey::HelpDeleteSelected), theme),
    ]
}

fn context_shortcuts(state: &AppState, locale: Locale, theme: &Theme) -> Vec<Line<'static>> {
    match state.section {
        Section::Home => home_shortcuts(locale, theme),
        Section::Transactions => transactions_shortcuts(state, locale, theme),
        Section::Accounts => accounts_shortcuts(state, locale, theme),
        Section::Analytics => analytics_shortcuts(locale, theme),
        Section::Settings => settings_shortcuts(state, locale, theme),
    }
}

fn home_shortcuts(locale: Locale, theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header(t(locale, TextKey::SectionHome), theme),
        Line::from(""),
        shortcut_line("j/k", t(locale, TextKey::HelpNavigateFeed), theme),
        shortcut_line("Enter", t(locale, TextKey::HelpOpenDetails), theme),
        shortcut_line("n", t(locale, TextKey::HelpQuickAddTxn), theme),
        shortcut_line("N", t(locale, TextKey::HelpNewTxnModal), theme),
        shortcut_line("t", t(locale, TextKey::HelpGoToTransactions), theme),
        shortcut_line("a", t(locale, TextKey::HelpGoToAccounts), theme),
        shortcut_line("y", t(locale, TextKey::HelpGoToAnalytics), theme),
        shortcut_line("s", t(locale, TextKey::HelpGoToSettings), theme),
    ]
}

fn transactions_shortcuts(state: &AppState, locale: Locale, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![
        section_header(t(locale, TextKey::SectionTransactions), theme),
        Line::from(""),
        shortcut_line("n", t(locale, TextKey::HintQuickAdd), theme),
        shortcut_line("N", t(locale, TextKey::HelpNewTxnModal), theme),
        shortcut_line("i", t(locale, TextKey::HelpNewIncome), theme),
        shortcut_line("R", t(locale, TextKey::HelpNewRefund), theme),
        shortcut_line("/", t(locale, TextKey::HelpToggleFilters), theme),
        shortcut_line("g", t(locale, TextKey::HelpGroupTxns), theme),
        Line::from(""),
        shortcut_line("1", t(locale, TextKey::HelpScopeWallet), theme),
        shortcut_line("2", t(locale, TextKey::HelpScopeFlow), theme),
        shortcut_line("c", t(locale, TextKey::HelpClearFilters), theme),
        shortcut_line("d", t(locale, TextKey::HelpDeleteTxn), theme),
        shortcut_line("u", t(locale, TextKey::HelpUndoDelete), theme),
        shortcut_line("z", t(locale, TextKey::HelpToggleVoided), theme),
        shortcut_line("]/[", t(locale, TextKey::HelpNextPrevPage), theme),
    ];

    lines.push(Line::from(""));
    lines.push(section_header(t(locale, TextKey::HelpVisualMode), theme));
    lines.push(Line::from(""));
    lines.push(shortcut_line(
        "v",
        t(locale, TextKey::HelpToggleVisual),
        theme,
    ));
    lines.push(shortcut_line(
        "Space",
        t(locale, TextKey::HelpSelectTxn),
        theme,
    ));
    lines.push(shortcut_line(
        "Esc",
        t(locale, TextKey::HelpExitVisual),
        theme,
    ));

    match state.transactions.mode {
        TransactionsMode::Detail => {
            lines.push(Line::from(""));
            lines.push(section_header(t(locale, TextKey::HelpDetailView), theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line("e", t(locale, TextKey::HelpEditTxn), theme));
            lines.push(shortcut_line("d", t(locale, TextKey::HelpDeleteTxn), theme));
            lines.push(shortcut_line("r", t(locale, TextKey::HelpRepeatTxn), theme));
            lines.push(shortcut_line("v", t(locale, TextKey::HelpVoidTxn), theme));
        }
        TransactionsMode::Form | TransactionsMode::Edit => {
            lines.push(Line::from(""));
            lines.push(section_header(t(locale, TextKey::HelpForm), theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "Tab",
                t(locale, TextKey::HelpNextField),
                theme,
            ));
            lines.push(shortcut_line(
                "↑/↓",
                t(locale, TextKey::HelpChangeValue),
                theme,
            ));
            lines.push(shortcut_line(
                "Enter",
                t(locale, TextKey::ActionSave),
                theme,
            ));
        }
        TransactionsMode::Filter => {
            lines.push(Line::from(""));
            lines.push(section_header(t(locale, TextKey::HelpFilters), theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "i/e/r",
                t(locale, TextKey::HelpToggleType),
                theme,
            ));
            lines.push(shortcut_line(
                "w/f",
                t(locale, TextKey::HelpToggleScope),
                theme,
            ));
            lines.push(shortcut_line("Enter", t(locale, TextKey::HelpApply), theme));
        }
        _ => {}
    }

    lines
}

fn accounts_shortcuts(state: &AppState, locale: Locale, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![
        section_header(t(locale, TextKey::HintAccounts), theme),
        Line::from(""),
        shortcut_line("Tab", t(locale, TextKey::HelpNextSubTab), theme),
        shortcut_line("Shift+Tab", t(locale, TextKey::HelpPrevSubTab), theme),
        shortcut_line("1/2/3", t(locale, TextKey::HelpJumpSubTab), theme),
        Line::from(""),
    ];

    match state.accounts_tab {
        AccountsTab::Sources => {
            lines.push(section_header(
                t(locale, TextKey::HelpSourcesWallets),
                theme,
            ));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "c",
                t(locale, TextKey::HelpCreateWallet),
                theme,
            ));
            lines.push(shortcut_line(
                "e",
                t(locale, TextKey::HelpRenameWallet),
                theme,
            ));
            lines.push(shortcut_line(
                "d",
                t(locale, TextKey::HelpDeleteArchive),
                theme,
            ));
            lines.push(shortcut_line(
                "Enter",
                t(locale, TextKey::HelpViewDetails),
                theme,
            ));
        }
        AccountsTab::Envelopes => {
            lines.push(section_header(
                t(locale, TextKey::HelpEnvelopesFlows),
                theme,
            ));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "c",
                t(locale, TextKey::HelpCreateEnvelope),
                theme,
            ));
            lines.push(shortcut_line(
                "e",
                t(locale, TextKey::HelpRenameEnvelope),
                theme,
            ));
            lines.push(shortcut_line(
                "d",
                t(locale, TextKey::HelpDeleteArchive),
                theme,
            ));
            lines.push(shortcut_line(
                "m",
                t(locale, TextKey::HelpChangeMode),
                theme,
            ));
            lines.push(shortcut_line(
                "Enter",
                t(locale, TextKey::HelpViewDetails),
                theme,
            ));
        }
        AccountsTab::Goals => {
            lines.push(section_header(t(locale, TextKey::HelpGoals), theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line(t(locale, TextKey::HelpComingSoon), "", theme));
        }
    }

    lines
}

fn analytics_shortcuts(locale: Locale, theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header(t(locale, TextKey::HintAnalytics), theme),
        Line::from(""),
        shortcut_line("r", t(locale, TextKey::HelpRefreshData), theme),
        shortcut_line("←/→", t(locale, TextKey::HelpSwitchView), theme),
        shortcut_line("1/2/3", t(locale, TextKey::HelpCashSpendWorth), theme),
        shortcut_line("[/]", t(locale, TextKey::HelpChangePeriod), theme),
    ]
}

fn settings_shortcuts(state: &AppState, locale: Locale, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![
        section_header(t(locale, TextKey::HintSettings), theme),
        Line::from(""),
        shortcut_line("Tab", t(locale, TextKey::HelpNextSubTab), theme),
        shortcut_line("Shift+Tab", t(locale, TextKey::HelpPrevSubTab), theme),
        shortcut_line("1/2/3", t(locale, TextKey::HelpJumpSubTab), theme),
        Line::from(""),
    ];

    match state.settings_tab {
        SettingsTab::Categories => {
            lines.push(section_header(t(locale, TextKey::SectionCategories), theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "c",
                t(locale, TextKey::HelpCreateCategory),
                theme,
            ));
            lines.push(shortcut_line(
                "e",
                t(locale, TextKey::HelpRenameCategory),
                theme,
            ));
            lines.push(shortcut_line(
                "d",
                t(locale, TextKey::HelpDeleteArchive),
                theme,
            ));
            lines.push(shortcut_line(
                "l",
                t(locale, TextKey::HelpManageAliases),
                theme,
            ));
            lines.push(shortcut_line(
                "m",
                t(locale, TextKey::HelpMergeCategories),
                theme,
            ));
            lines.push(Line::from(""));
            lines.push(section_header(t(locale, TextKey::HelpAliases), theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "Tab",
                t(locale, TextKey::HelpSwitchFocus),
                theme,
            ));
            lines.push(shortcut_line(
                "x",
                t(locale, TextKey::HelpDeleteAlias),
                theme,
            ));
            lines.push(shortcut_line(
                "Enter",
                t(locale, TextKey::HelpAddSave),
                theme,
            ));
        }
        SettingsTab::Vault => {
            lines.push(section_header(t(locale, TextKey::HelpVault), theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "c",
                t(locale, TextKey::HelpCreateVault),
                theme,
            ));
            lines.push(shortcut_line(
                "Enter",
                t(locale, TextKey::HelpSelectVault),
                theme,
            ));
        }
        SettingsTab::Members => {
            lines.push(section_header(t(locale, TextKey::HelpMembers), theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line("a", t(locale, TextKey::HelpAddMember), theme));
            lines.push(shortcut_line(
                "e",
                t(locale, TextKey::HelpEditMember),
                theme,
            ));
            lines.push(shortcut_line(
                "x",
                t(locale, TextKey::HelpRemoveMember),
                theme,
            ));
            lines.push(shortcut_line(
                "v",
                t(locale, TextKey::HelpVaultMembers),
                theme,
            ));
            lines.push(shortcut_line(
                "f",
                t(locale, TextKey::HelpFlowSharing),
                theme,
            ));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "[/]",
                t(locale, TextKey::HelpChangeFlow),
                theme,
            ));
            lines.push(shortcut_line(
                "↑/↓",
                t(locale, TextKey::HelpChangeRole),
                theme,
            ));
        }
        SettingsTab::Preferences => {
            lines.push(section_header(
                t(locale, TextKey::SectionPreferences),
                theme,
            ));
            lines.push(Line::from(""));
            lines.push(shortcut_line(
                "Space",
                t(locale, TextKey::HintToggle),
                theme,
            ));
            lines.push(shortcut_line(
                "↑/↓",
                t(locale, TextKey::HelpNavigateList),
                theme,
            ));
            lines.push(shortcut_line(
                "←/→",
                t(locale, TextKey::HelpChangeValue),
                theme,
            ));
        }
    }

    lines
}

fn section_header(title: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("  {title}"),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )])
}

fn shortcut_line(key: &str, description: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(format!("{key:<12}"), Style::default().fg(theme.accent)),
        Span::styled(description.to_string(), Style::default().fg(theme.text)),
    ])
}
