use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use engine::{Currency, Money};

use crate::{app::AppState, ui::theme::Theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    render_header(frame, layout[0], state, &theme);
    render_body(frame, layout[1], state, &theme);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let mut line = vec![Span::styled(
        "Stats",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )];
    if let Some(err) = state.stats.error.as_ref() {
        line.push(Span::raw("   "));
        line.push(Span::styled(err.as_str(), Style::default().fg(theme.error)));
    }

    let block = Block::default().borders(Borders::ALL).title("Stats");
    frame.render_widget(Paragraph::new(Line::from(line)).block(block), area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(stat) = state.stats.data.as_ref() else {
        let block = Block::default()
            .title("Stats")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent));
        frame.render_widget(
            Paragraph::new(Line::from("Nessuna statistica disponibile."))
                .alignment(Alignment::Center)
                .block(block),
            area,
        );
        return;
    };

    let currency = map_currency(&stat.currency);
    let balance = Money::new(stat.balance_minor).format(currency);
    let income = Money::new(stat.total_income_minor).format(currency);
    let expenses = Money::new(stat.total_expenses_minor).format(currency);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    let lines = vec![
        Line::from(vec![
            Span::styled("Balance", Style::default().fg(theme.dim)),
            Span::raw(format!(": {balance}")),
        ]),
        Line::from(vec![
            Span::styled("Total income", Style::default().fg(theme.dim)),
            Span::raw(format!(": {income}")),
        ]),
        Line::from(vec![
            Span::styled("Total expenses", Style::default().fg(theme.dim)),
            Span::raw(format!(": {expenses}")),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("r", Style::default().fg(theme.accent)),
            Span::raw(" refresh"),
        ]),
    ];

    let block = Block::default()
        .title("Vault Stats")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);
}

fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}
