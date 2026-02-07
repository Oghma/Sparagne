use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{
    app::{AccountsTab, AppState, Section, SettingsTab, TransactionsMode},
    text::{Locale, TextKey, t},
    ui::theme::Theme,
};

pub(super) fn global_shortcuts(locale: Locale, theme: &Theme) -> Vec<Line<'static>> {
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

pub(super) fn context_shortcuts(
    state: &AppState,
    locale: Locale,
    theme: &Theme,
) -> Vec<Line<'static>> {
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
        shortcut_line("T", t(locale, TextKey::HelpNewTransfer), theme),
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
        shortcut_line("←/→", t(locale, TextKey::HelpSwitchPanel), theme),
        Line::from(""),
    ];

    match state.accounts_tab {
        AccountsTab::Wallets => {
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
        AccountsTab::Budget => {
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
