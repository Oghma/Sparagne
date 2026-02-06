//! Filter modal rendering.
//!
//! Displays:
//! - Date range filters (from/to)
//! - Transaction kind checkboxes (income, expense, refund, transfers)
//! - Apply/cancel hints

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    app::{AppState, FilterField},
    ui::{common::render_label_value_field, components::centered_rect, theme::Theme},
};

/// Renders the filter modal overlay
pub fn render_filter_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let filter = &state.transactions.filter;
    let popup = centered_rect(75, 60, area);
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(11), Constraint::Min(0)])
        .split(popup);

    let kinds_focused = filter.focus == FilterField::Kinds;
    let kinds_label_style = if kinds_focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let mut lines = vec![
        render_label_value_field(
            "From",
            filter.from_input.as_str(),
            filter.focus == FilterField::From,
            theme,
        ),
        render_label_value_field(
            "To",
            filter.to_input.as_str(),
            filter.focus == FilterField::To,
            theme,
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("Transaction Types ", kinds_label_style),
            Span::styled("(press key to toggle)", Style::default().fg(theme.text_muted)),
        ]),
        // Row 1: Income, Expense, Refund
        Line::from(vec![
            Span::raw("  "),
            filter_toggle_with_icon("▲", "Income", "i", filter.kind_income, theme),
            Span::raw("    "),
            filter_toggle_with_icon("▼", "Expense", "e", filter.kind_expense, theme),
            Span::raw("    "),
            filter_toggle_with_icon("↩", "Refund", "r", filter.kind_refund, theme),
        ]),
        // Row 2: Transfers
        Line::from(vec![
            Span::raw("  "),
            filter_toggle_with_icon("⇄", "Wallet Transfer", "w", filter.kind_transfer_wallet, theme),
            Span::raw("    "),
            filter_toggle_with_icon("⇄", "Flow Transfer", "f", filter.kind_transfer_flow, theme),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Tab]", Style::default().fg(theme.accent)),
            Span::styled(" next  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Enter]", Style::default().fg(theme.accent)),
            Span::styled(" apply  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Esc]", Style::default().fg(theme.accent)),
            Span::styled(" cancel", Style::default().fg(theme.text_muted)),
        ]),
    ];

    if let Some(err) = filter.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.negative),
        )));
    }

    let block = Block::default()
        .title(Span::styled(" Filters ", Style::default().fg(theme.accent)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.background));
    frame.render_widget(Paragraph::new(lines).block(block), layout[0]);
}

/// Renders a filter toggle with icon, label, and key hint
fn filter_toggle_with_icon(
    icon: &str,
    label: &str,
    key: &str,
    enabled: bool,
    theme: &Theme,
) -> Span<'static> {
    let (checkbox, style) = if enabled {
        ("[✓]", Style::default().fg(theme.positive))
    } else {
        ("[✗]", Style::default().fg(theme.text_muted))
    };
    let text = format!("{checkbox} {icon} {label} ({key})");
    Span::styled(text, style)
}
