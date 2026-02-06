//! Wallet list rendering.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use engine::Money;

use crate::{
    app::{AppState, WalletsMode, wallets_visible_indices},
    ui::{components::loading, theme::Theme},
};

use super::common::progress_bar;
use super::form::render_form;
use crate::ui::common::get_currency;

/// Renders the wallet list view.
pub fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let show_form = state.wallets.mode == WalletsMode::Create;

    // Calculate stats for header
    let (total_balance, wallet_count, archived_count) = state
        .snapshot
        .as_ref()
        .map(|snap| {
            let active: i64 = snap
                .wallets
                .iter()
                .filter(|w| !w.archived)
                .map(|w| w.balance_minor)
                .sum();
            let count = snap.wallets.iter().filter(|w| !w.archived).count();
            let archived = snap.wallets.iter().filter(|w| w.archived).count();
            (active, count, archived)
        })
        .unwrap_or((0, 0, 0));

    let constraints = if show_form {
        vec![Constraint::Length(2), Constraint::Length(7), Constraint::Min(0)]
    } else {
        vec![Constraint::Length(2), Constraint::Min(0)]
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Render stats header
    render_stats_header(frame, layout[0], total_balance, wallet_count, archived_count, state, theme);

    let list_area = if show_form {
        render_form(frame, layout[1], state, theme);
        layout[2]
    } else {
        layout[1]
    };

    // Search bar in header
    let search_active = state.wallets.search.active;
    let search_query = state.wallets.search.query.trim();
    let show_archived = state.wallets.show_archived;

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
        .title(Span::styled(" Wallets ", Style::default().fg(theme.accent)))
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

    let currency = get_currency(state);

    let visible = wallets_visible_indices(state);

    // Calculate max balance for progress bars
    let max_balance = snapshot
        .wallets
        .iter()
        .map(|w| w.balance_minor.unsigned_abs())
        .max()
        .unwrap_or(1) as i64;

    let selected_idx = state.wallets.selected;
    let items = visible
        .iter()
        .enumerate()
        .filter_map(|(list_idx, idx)| snapshot.wallets.get(*idx).map(|w| (list_idx, w)))
        .map(|(list_idx, wallet)| {
            let is_selected = list_idx == selected_idx;
            let emoji = "💰";
            let name_style = if wallet.archived {
                Style::default().fg(theme.text_muted)
            } else {
                Style::default().fg(theme.text)
            };

            let balance_color = if wallet.balance_minor >= 0 {
                theme.positive
            } else {
                theme.negative
            };

            // Progress bar
            let bar = progress_bar(wallet.balance_minor.unsigned_abs() as i64, max_balance, 10);

            let mut spans = vec![
                Span::raw(format!("  {emoji} ")),
                Span::styled(format!("{:<16}", wallet.name), name_style),
                Span::styled(
                    format!("{:>12}", Money::new(wallet.balance_minor).format(currency)),
                    Style::default().fg(balance_color),
                ),
                Span::raw("  "),
                Span::styled(bar, Style::default().fg(theme.accent)),
            ];

            if wallet.archived {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    "[archived]",
                    Style::default().fg(theme.warning),
                ));
            }

            // Build item with optional action hints for selected item
            if is_selected && state.wallets.mode == WalletsMode::List {
                let hints = vec![
                    Span::raw("     "),
                    Span::styled("[e]", Style::default().fg(theme.accent)),
                    Span::styled("dit ", Style::default().fg(theme.text_muted)),
                    Span::styled("[a]", Style::default().fg(theme.accent)),
                    Span::styled("rchive ", Style::default().fg(theme.text_muted)),
                    Span::styled("[d]", Style::default().fg(theme.accent)),
                    Span::styled("elete ", Style::default().fg(theme.text_muted)),
                    Span::styled("[Enter]", Style::default().fg(theme.accent)),
                    Span::styled(" details", Style::default().fg(theme.text_muted)),
                ];
                ListItem::new(vec![Line::from(spans), Line::from(hints)])
            } else {
                ListItem::new(Line::from(spans))
            }
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        render_empty_list(frame, list_area, list_block, state, theme);
        return;
    }

    let mut list_state = ListState::default();
    list_state.select(Some(
        state.wallets.selected.min(items.len().saturating_sub(1)),
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

fn render_empty_list(
    frame: &mut Frame<'_>,
    area: Rect,
    block: Block<'_>,
    state: &AppState,
    theme: &Theme,
) {
    let query = state.wallets.search.query.trim();
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
                "💰 Welcome!",
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Let's create your first wallet to start",
                Style::default().fg(theme.text_muted),
            )),
            Line::from(Span::styled(
                "tracking your finances.",
                Style::default().fg(theme.text_muted),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("[c]", Style::default().fg(theme.accent)),
                Span::styled(" Quick create  ", Style::default().fg(theme.text_muted)),
                Span::styled("[n]", Style::default().fg(theme.accent)),
                Span::styled(" Create with details", Style::default().fg(theme.text_muted)),
            ]),
        ]
    };

    let empty_msg = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(block);
    frame.render_widget(empty_msg, area);
}

fn render_stats_header(
    frame: &mut Frame<'_>,
    area: Rect,
    total_balance: i64,
    wallet_count: usize,
    archived_count: usize,
    state: &AppState,
    theme: &Theme,
) {
    let currency = get_currency(state);

    let balance_color = if total_balance >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    let mut spans = vec![
        Span::styled(" Total: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            Money::new(total_balance).format(currency),
            Style::default().fg(balance_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({wallet_count} wallets)"),
            Style::default().fg(theme.text_muted),
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
