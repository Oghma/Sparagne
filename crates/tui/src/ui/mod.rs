pub mod components;
pub mod keymap;
pub mod screens;

mod terminal;
mod theme;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::AppState;

pub use terminal::{AppTerminal as Terminal, restore_terminal, setup_terminal};

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    match state.screen {
        crate::app::Screen::Login => screens::login::render(frame, area, state),
        crate::app::Screen::Home => render_shell(frame, area, state),
    }
}

fn render_shell(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = theme::Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_top_bar(frame, layout[0], state);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(0)])
        .split(layout[1]);
    render_nav(frame, body[0], state);

    match state.section {
        crate::app::Section::Home => screens::home::render(frame, body[1], state),
        crate::app::Section::Transactions => screens::transactions::render(frame, body[1], state),
        _ => screens::placeholder::render(frame, body[1], state),
    }

    render_bottom_bar(frame, layout[2], state, &theme);
}

fn render_top_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = theme::Theme::default();
    let vault_name = state
        .vault
        .as_ref()
        .and_then(|v| v.name.as_deref())
        .unwrap_or("Main");
    let username = state.login.username.as_str();
    let base = format!(
        "Vault: {vault_name}  •  User: {username}  •  {base_url}",
        base_url = state.base_url
    );
    let bar = Paragraph::new(Line::from(Span::styled(
        base,
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Left);
    frame.render_widget(bar, area);
}

fn render_nav(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = theme::Theme::default();
    let sections = [
        crate::app::Section::Home,
        crate::app::Section::Transactions,
        crate::app::Section::Wallets,
        crate::app::Section::Flows,
        crate::app::Section::Vault,
        crate::app::Section::Stats,
    ];
    let items = sections
        .iter()
        .map(|section| {
            let style = if *section == state.section {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            ListItem::new(Line::from(Span::styled(section.label(), style)))
        })
        .collect::<Vec<_>>();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Menu"));
    frame.render_widget(list, area);
}

fn render_bottom_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &theme::Theme) {
    let mut parts = vec![
        Span::styled("h", Style::default().fg(theme.accent)),
        Span::raw(" home  "),
        Span::styled("t", Style::default().fg(theme.accent)),
        Span::raw(" transazioni  "),
        Span::styled("w", Style::default().fg(theme.accent)),
        Span::raw(" wallet  "),
        Span::styled("f", Style::default().fg(theme.accent)),
        Span::raw(" flows  "),
        Span::styled("v", Style::default().fg(theme.accent)),
        Span::raw(" vault  "),
        Span::styled("s", Style::default().fg(theme.accent)),
        Span::raw(" stats  "),
    ];

    if state.section == crate::app::Section::Transactions {
        parts.push(Span::raw(" | "));
        parts.push(Span::styled("r", Style::default().fg(theme.accent)));
        parts.push(Span::raw(" refresh "));
        parts.push(Span::styled("n", Style::default().fg(theme.accent)));
        parts.push(Span::raw(" next "));
        parts.push(Span::styled("p", Style::default().fg(theme.accent)));
        parts.push(Span::raw(" prev "));
        parts.push(Span::styled("v", Style::default().fg(theme.accent)));
        parts.push(Span::raw(" voided "));
        parts.push(Span::styled("t", Style::default().fg(theme.accent)));
        parts.push(Span::raw(" transfers "));
        parts.push(Span::styled("↑/↓", Style::default().fg(theme.accent)));
        parts.push(Span::raw(" select "));
    }

    parts.push(Span::styled("q", Style::default().fg(theme.accent)));
    parts.push(Span::raw(" quit"));

    let bar = Paragraph::new(Line::from(parts));
    frame.render_widget(bar, area);
}
