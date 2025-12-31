use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    app::{AppState, Section, TransactionsMode},
    ui::{
        components::{centered_rect, tabs},
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if !state.help.active {
        return;
    }

    let theme = Theme::default();
    let popup = centered_rect(70, 70, area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(popup);

    let title = Line::from(vec![
        Span::styled("Help", Style::default().fg(theme.accent)),
        Span::raw("  "),
        Span::styled("Esc", Style::default().fg(theme.dim)),
        Span::raw(" close"),
    ]);

    frame.render_widget(
        Paragraph::new(title).block(
            Block::default()
                .title("Keybinds")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        layout[0],
    );

    let lines = help_lines(state, &theme);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), layout[1]);
}

fn help_lines(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled("Ctrl+P", Style::default().fg(theme.accent)),
        Span::raw(" command palette  "),
        Span::styled("?", Style::default().fg(theme.accent)),
        Span::raw(" help"),
    ])];
    lines.push(Line::from(tabs::tab_shortcuts(theme)));

    match state.section {
        Section::Home => {
            lines.push(Line::from("Home: usa i tab per navigare."));
        }
        Section::Transactions => {
            lines.push(Line::from("Transactions:"));
            lines.push(Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme.accent)),
                Span::raw(" detail  "),
                Span::styled("a", Style::default().fg(theme.accent)),
                Span::raw(" quick add  "),
                Span::styled("/", Style::default().fg(theme.accent)),
                Span::raw(" filters"),
            ]));
            lines.push(Line::from(vec![
                Span::styled("i", Style::default().fg(theme.accent)),
                Span::raw(" income form  "),
                Span::styled("e", Style::default().fg(theme.accent)),
                Span::raw(" expense form  "),
                Span::styled("R", Style::default().fg(theme.accent)),
                Span::raw(" refund form"),
            ]));
            lines.push(Line::from(vec![
                Span::styled("w", Style::default().fg(theme.accent)),
                Span::raw(" wallet scope  "),
                Span::styled("f", Style::default().fg(theme.accent)),
                Span::raw(" flow scope  "),
                Span::styled("c", Style::default().fg(theme.accent)),
                Span::raw(" clear filters"),
            ]));
            lines.push(Line::from(vec![
                Span::styled("u", Style::default().fg(theme.accent)),
                Span::raw(" undo last  "),
                Span::styled("v", Style::default().fg(theme.accent)),
                Span::raw(" toggle voided  "),
                Span::styled("t", Style::default().fg(theme.accent)),
                Span::raw(" transfers"),
            ]));

            match state.transactions.mode {
                TransactionsMode::Detail => {
                    lines.push(Line::from(vec![
                        Span::styled("e", Style::default().fg(theme.accent)),
                        Span::raw(" edit  "),
                        Span::styled("r", Style::default().fg(theme.accent)),
                        Span::raw(" repeat  "),
                        Span::styled("v", Style::default().fg(theme.accent)),
                        Span::raw(" void"),
                    ]));
                }
                TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                    lines.push(Line::from(vec![
                        Span::styled("Tab", Style::default().fg(theme.accent)),
                        Span::raw(" next field  "),
                        Span::styled("↑/↓", Style::default().fg(theme.accent)),
                        Span::raw(" change  "),
                        Span::styled("Enter", Style::default().fg(theme.accent)),
                        Span::raw(" save"),
                    ]));
                }
                TransactionsMode::Form | TransactionsMode::Edit => {
                    lines.push(Line::from(vec![
                        Span::styled("Tab", Style::default().fg(theme.accent)),
                        Span::raw(" next field  "),
                        Span::styled("↑/↓", Style::default().fg(theme.accent)),
                        Span::raw(" change  "),
                        Span::styled("Enter", Style::default().fg(theme.accent)),
                        Span::raw(" save"),
                    ]));
                }
                TransactionsMode::Filter => {
                    lines.push(Line::from(vec![
                        Span::styled("i/e/r/w/f", Style::default().fg(theme.accent)),
                        Span::raw(" toggle kinds  "),
                        Span::styled("Enter", Style::default().fg(theme.accent)),
                        Span::raw(" apply"),
                    ]));
                }
                _ => {}
            }
        }
        Section::Wallets => {
            lines.push(Line::from(vec![
                Span::styled("c", Style::default().fg(theme.accent)),
                Span::raw(" create  "),
                Span::styled("e", Style::default().fg(theme.accent)),
                Span::raw(" rename  "),
                Span::styled("a", Style::default().fg(theme.accent)),
                Span::raw(" archive"),
            ]));
        }
        Section::Flows => {
            lines.push(Line::from(vec![
                Span::styled("c", Style::default().fg(theme.accent)),
                Span::raw(" create  "),
                Span::styled("e", Style::default().fg(theme.accent)),
                Span::raw(" rename  "),
                Span::styled("a", Style::default().fg(theme.accent)),
                Span::raw(" archive  "),
                Span::styled("m", Style::default().fg(theme.accent)),
                Span::raw(" mode"),
            ]));
        }
        Section::Vault => {
            lines.push(Line::from(vec![
                Span::styled("c", Style::default().fg(theme.accent)),
                Span::raw(" create"),
            ]));
        }
        Section::Stats => {
            lines.push(Line::from(vec![
                Span::styled("r", Style::default().fg(theme.accent)),
                Span::raw(" refresh"),
            ]));
        }
    }

    lines.push(Line::from(vec![
        Span::styled("Esc", Style::default().fg(theme.accent)),
        Span::raw(" back/close"),
    ]));

    lines
}
