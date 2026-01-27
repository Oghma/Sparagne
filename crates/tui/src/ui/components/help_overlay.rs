use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    app::{AppState, Section, TransactionsMode},
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if !state.help.active {
        return;
    }

    let theme = Theme::default();
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

    render_header(frame, layout[0], state, &theme);
    render_content(frame, layout[1], state, &theme);
    render_footer(frame, layout[2], &theme);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let section_name = match state.section {
        Section::Home => "Home",
        Section::Transactions => "Transactions",
        Section::Wallets => "Wallets",
        Section::Flows => "Accounts",
        Section::Categories => "Categories",
        Section::Members => "Members",
        Section::Vault => "Vault",
        Section::Stats => "Statistics",
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Keyboard Shortcuts  ",
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

fn render_content(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
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

    let left_lines = global_shortcuts(theme);
    let right_lines = context_shortcuts(state, theme);

    frame.render_widget(Paragraph::new(left_lines), columns[0]);
    frame.render_widget(Paragraph::new(right_lines), columns[1]);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let line = Line::from(vec![
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(" close help", Style::default().fg(theme.text_muted)),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}

fn global_shortcuts(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header("Global", theme),
        Line::from(""),
        shortcut_line("n", "Quick add transaction", theme),
        shortcut_line("N", "New transaction (modal)", theme),
        shortcut_line("Ctrl+P", "Command palette", theme),
        shortcut_line("Ctrl+F", "Search", theme),
        shortcut_line("?", "Show/hide help", theme),
        Line::from(""),
        section_header("Navigation", theme),
        Line::from(""),
        shortcut_line("h", "Home", theme),
        shortcut_line("t", "Transactions", theme),
        shortcut_line("w", "Wallets", theme),
        shortcut_line("a", "Accounts", theme),
        shortcut_line("g", "Categories", theme),
        shortcut_line("s", "Statistics", theme),
        shortcut_line("↑/↓ j/k", "Navigate list", theme),
        shortcut_line("Enter", "Open details", theme),
        shortcut_line("Esc", "Back / Close", theme),
        Line::from(""),
        section_header("Common Actions", theme),
        Line::from(""),
        shortcut_line("e", "Edit selected", theme),
        shortcut_line("d", "Delete selected", theme),
    ]
}

fn context_shortcuts(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    match state.section {
        Section::Home => home_shortcuts(theme),
        Section::Transactions => transactions_shortcuts(state, theme),
        Section::Wallets => wallets_shortcuts(theme),
        Section::Flows => flows_shortcuts(theme),
        Section::Categories => categories_shortcuts(theme),
        Section::Members => members_shortcuts(theme),
        Section::Vault => vault_shortcuts(theme),
        Section::Stats => stats_shortcuts(theme),
    }
}

fn home_shortcuts(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header("Home", theme),
        Line::from(""),
        shortcut_line("j/k", "Navigate feed", theme),
        shortcut_line("Enter", "Open details", theme),
        shortcut_line("n", "Quick add transaction", theme),
        shortcut_line("N", "New transaction (modal)", theme),
        shortcut_line("t", "Go to Transactions", theme),
        shortcut_line("w", "Go to Wallets", theme),
        shortcut_line("a", "Go to Accounts", theme),
    ]
}

fn transactions_shortcuts(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![
        section_header("Transactions", theme),
        Line::from(""),
        shortcut_line("n", "Quick add", theme),
        shortcut_line("N", "New transaction (modal)", theme),
        shortcut_line("i", "New income", theme),
        shortcut_line("R", "New refund", theme),
        shortcut_line("/", "Toggle filters", theme),
        shortcut_line("g", "Group transactions", theme),
        Line::from(""),
        shortcut_line("1", "Scope to wallet", theme),
        shortcut_line("2", "Scope to flow", theme),
        shortcut_line("c", "Clear filters", theme),
        shortcut_line("d", "Delete transaction", theme),
        shortcut_line("u", "Undo delete (when shown)", theme),
        shortcut_line("z", "Toggle voided", theme),
        shortcut_line("]/[", "Next/prev page", theme),
    ];

    lines.push(Line::from(""));
    lines.push(section_header("Visual Mode", theme));
    lines.push(Line::from(""));
    lines.push(shortcut_line("v", "Toggle visual mode", theme));
    lines.push(shortcut_line("Space", "Select transaction", theme));
    lines.push(shortcut_line("Esc", "Exit visual mode", theme));

    match state.transactions.mode {
        TransactionsMode::Detail => {
            lines.push(Line::from(""));
            lines.push(section_header("Detail View", theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line("e", "Edit transaction", theme));
            lines.push(shortcut_line("d", "Delete transaction", theme));
            lines.push(shortcut_line("r", "Repeat transaction", theme));
            lines.push(shortcut_line("v", "Void transaction", theme));
        }
        TransactionsMode::Form | TransactionsMode::Edit => {
            lines.push(Line::from(""));
            lines.push(section_header("Form", theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line("Tab", "Next field", theme));
            lines.push(shortcut_line("↑/↓", "Change value", theme));
            lines.push(shortcut_line("Enter", "Save", theme));
        }
        TransactionsMode::Filter => {
            lines.push(Line::from(""));
            lines.push(section_header("Filters", theme));
            lines.push(Line::from(""));
            lines.push(shortcut_line("i/e/r", "Toggle type", theme));
            lines.push(shortcut_line("w/f", "Toggle scope", theme));
            lines.push(shortcut_line("Enter", "Apply", theme));
        }
        _ => {}
    }

    lines
}

fn wallets_shortcuts(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header("Wallets", theme),
        Line::from(""),
        shortcut_line("c", "Create wallet", theme),
        shortcut_line("e", "Rename wallet", theme),
        shortcut_line("d", "Delete (archive)", theme),
        shortcut_line("Enter", "View details", theme),
        Line::from(""),
        section_header("Detail View", theme),
        Line::from(""),
        shortcut_line("Esc", "Back to list", theme),
    ]
}

fn flows_shortcuts(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header("Accounts (Sources / Envelopes / Goals)", theme),
        Line::from(""),
        shortcut_line("←/→", "Switch tab", theme),
        shortcut_line("1/2/3", "Jump tab", theme),
        shortcut_line("c", "Create (current tab)", theme),
        shortcut_line("e", "Rename (current tab)", theme),
        shortcut_line("d", "Delete (current tab)", theme),
        shortcut_line("m", "Change envelope mode", theme),
        shortcut_line("Enter", "View details", theme),
    ]
}

fn categories_shortcuts(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header("Categories", theme),
        Line::from(""),
        shortcut_line("c", "Create category", theme),
        shortcut_line("e", "Rename category", theme),
        shortcut_line("d", "Delete (archive)", theme),
        shortcut_line("l", "Manage aliases", theme),
        shortcut_line("m", "Merge categories", theme),
        Line::from(""),
        section_header("Aliases", theme),
        Line::from(""),
        shortcut_line("Tab", "Switch focus", theme),
        shortcut_line("x", "Delete alias", theme),
        shortcut_line("Enter", "Add/Save", theme),
    ]
}

fn members_shortcuts(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header("Members", theme),
        Line::from(""),
        shortcut_line("a", "Add member", theme),
        shortcut_line("e", "Edit member", theme),
        shortcut_line("x", "Remove member", theme),
        shortcut_line("v", "Vault members", theme),
        shortcut_line("f", "Flow sharing", theme),
        Line::from(""),
        shortcut_line("[/]", "Change flow", theme),
        shortcut_line("↑/↓", "Change role", theme),
    ]
}

fn vault_shortcuts(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header("Vault", theme),
        Line::from(""),
        shortcut_line("c", "Create vault", theme),
        shortcut_line("Enter", "Select vault", theme),
    ]
}

fn stats_shortcuts(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section_header("Statistics", theme),
        Line::from(""),
        shortcut_line("r", "Refresh data", theme),
        shortcut_line("←/→", "Switch view", theme),
        shortcut_line("1/2/3", "Cash/Spend/Worth", theme),
        shortcut_line("[/]", "Change period", theme),
    ]
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
