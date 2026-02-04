//! Flow list rendering with items and stats header.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use engine::{Currency, Money};

use super::common::map_currency;
use super::form::render_form;
use crate::{
    app::{AppState, FlowsMode, flows_visible_indices},
    ui::{components::loading, theme::Theme},
};

/// Render the flow list view.
pub fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let show_form = state.flows.mode == FlowsMode::Create;

    // Calculate stats for header
    let (total_balance, flow_count, archived_count) = state
        .snapshot
        .as_ref()
        .map(|snap| {
            let balance: i64 = snap
                .flows
                .iter()
                .filter(|f| !f.archived)
                .map(|f| f.balance_minor)
                .sum();
            let count = snap.flows.iter().filter(|f| !f.archived).count();
            let archived = snap.flows.iter().filter(|f| f.archived).count();
            (balance, count, archived)
        })
        .unwrap_or((0, 0, 0));

    let constraints = if show_form {
        vec![Constraint::Length(2), Constraint::Length(8), Constraint::Min(0)]
    } else {
        vec![Constraint::Length(2), Constraint::Min(0)]
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Render stats header
    render_stats_header(frame, layout[0], total_balance, flow_count, archived_count, state, theme);

    let list_area = if show_form {
        render_form(frame, layout[1], state, theme);
        layout[2]
    } else {
        layout[1]
    };

    // Search bar in header
    let search_active = state.flows.search_active;
    let search_query = state.flows.search_query.trim();
    let show_archived = state.flows.show_archived;

    let mut header_spans = if search_active || !search_query.is_empty() {
        vec![
            Span::styled("Search: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                if search_query.is_empty() {
                    "..."
                } else {
                    search_query
                },
                Style::default().fg(if search_active {
                    theme.accent
                } else {
                    theme.text
                }),
            ),
            Span::styled("  [Esc] clear", Style::default().fg(theme.text_muted)),
        ]
    } else {
        vec![
            Span::styled("[c]", Style::default().fg(theme.accent)),
            Span::styled(" create  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Ctrl+F]", Style::default().fg(theme.accent)),
            Span::styled(" search  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Enter]", Style::default().fg(theme.accent)),
            Span::styled(" details", Style::default().fg(theme.text_muted)),
        ]
    };

    // Add archived indicator
    if show_archived {
        header_spans.push(Span::styled("  ", Style::default()));
        header_spans.push(Span::styled(
            "Archived: On",
            Style::default().fg(theme.warning),
        ));
    }

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " Budgets & Goals ",
            Style::default().fg(theme.accent),
        ))
        .title_bottom(Line::from(header_spans).centered());

    let Some(snapshot) = state.snapshot.as_ref() else {
        loading::render_inline_block(
            frame,
            list_area,
            list_block,
            loading::spinner_frame(state.spinner.index()),
            "Loading...",
            None,
            theme,
        );
        return;
    };

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let visible = flows_visible_indices(state);

    // Calculate max balance for progress bars
    let max_balance = snapshot
        .flows
        .iter()
        .map(|f| f.balance_minor.unsigned_abs())
        .max()
        .unwrap_or(1) as i64;

    let selected_idx = state.flows.selected;
    let items = visible
        .iter()
        .enumerate()
        .filter_map(|(list_idx, idx)| snapshot.flows.get(*idx).map(|f| (list_idx, f)))
        .map(|(list_idx, flow)| {
            let is_selected = list_idx == selected_idx;
            let emoji = if flow.is_unallocated { "📦" } else { "🎯" };
            let name_style = if flow.archived {
                Style::default().fg(theme.text_muted)
            } else {
                Style::default().fg(theme.text)
            };

            let balance_color = if flow.balance_minor >= 0 {
                theme.positive
            } else {
                theme.negative
            };

            // Progress bar
            let bar = progress_bar(flow.balance_minor.unsigned_abs() as i64, max_balance, 10);

            let mut spans = vec![
                Span::raw(format!("  {emoji} ")),
                Span::styled(format!("{:<16}", flow.name), name_style),
                Span::styled(
                    format!("{:>12}", Money::new(flow.balance_minor).format(currency)),
                    Style::default().fg(balance_color),
                ),
                Span::raw("  "),
                Span::styled(bar, Style::default().fg(theme.accent)),
            ];

            if flow.is_unallocated {
                spans.push(Span::raw("  "));
                spans.push(Span::styled("[default]", Style::default().fg(theme.info)));
            }

            if flow.archived {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    "[archived]",
                    Style::default().fg(theme.warning),
                ));
            }

            // Build item with optional action hints for selected item
            if is_selected && state.flows.mode == FlowsMode::List {
                let hints = vec![
                    Span::raw("     "),
                    Span::styled("[e]", Style::default().fg(theme.accent)),
                    Span::styled("dit ", Style::default().fg(theme.dim)),
                    Span::styled("[m]", Style::default().fg(theme.accent)),
                    Span::styled("ode ", Style::default().fg(theme.dim)),
                    Span::styled("[a]", Style::default().fg(theme.accent)),
                    Span::styled("rchive ", Style::default().fg(theme.dim)),
                    Span::styled("[Enter]", Style::default().fg(theme.accent)),
                    Span::styled(" details", Style::default().fg(theme.dim)),
                ];
                ListItem::new(vec![Line::from(spans), Line::from(hints)])
            } else {
                ListItem::new(Line::from(spans))
            }
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        let query = state.flows.search_query.trim();
        let lines = if !query.is_empty() {
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("No results for "),
                    Span::styled(format!("\"{query}\""), Style::default().fg(theme.accent)),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    "[Esc] to clear search",
                    Style::default().fg(theme.text_muted),
                )),
            ]
        } else {
            // Rich empty state with welcome message
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "📦 Budget with Envelopes",
                    Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Create envelopes to organize and track",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(Span::styled(
                    "spending by category or goal.",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[c]", Style::default().fg(theme.accent)),
                    Span::styled(" Quick create  ", Style::default().fg(theme.text_muted)),
                    Span::styled("[n]", Style::default().fg(theme.accent)),
                    Span::styled(" Create with cap", Style::default().fg(theme.text_muted)),
                ]),
            ]
        };

        let empty_msg = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(list_block);
        frame.render_widget(empty_msg, list_area);
        return;
    }

    let mut list_state = ListState::default();
    list_state.select(Some(
        state.flows.selected.min(items.len().saturating_sub(1)),
    ));

    let list = List::new(items)
        .block(list_block)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, list_area, &mut list_state);
}

/// Render the stats header showing total balance and counts.
fn render_stats_header(
    frame: &mut Frame<'_>,
    area: Rect,
    total_balance: i64,
    flow_count: usize,
    archived_count: usize,
    state: &AppState,
    theme: &Theme,
) {
    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let balance_color = if total_balance >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    let mut spans = vec![
        Span::styled(" Allocated: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            Money::new(total_balance).format(currency),
            Style::default().fg(balance_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({flow_count} envelopes)"),
            Style::default().fg(theme.dim),
        ),
    ];

    if archived_count > 0 {
        spans.push(Span::styled("  │  ", Style::default().fg(theme.border)));
        spans.push(Span::styled(
            format!("Archived: {archived_count}"),
            Style::default().fg(theme.warning),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Create a simple text progress bar.
fn progress_bar(value: i64, max: i64, width: usize) -> String {
    if max == 0 {
        return "░".repeat(width);
    }

    let ratio = (value.unsigned_abs() as f64 / max.unsigned_abs() as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64) as usize).min(width);
    let empty = width.saturating_sub(filled);

    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}
