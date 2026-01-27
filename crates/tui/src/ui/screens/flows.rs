use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use api_types::transaction::TransactionKind;
use engine::{Currency, Money};

use crate::{
    app::{AppState, FlowFormField, FlowModeChoice, FlowsMode, flows_visible_indices},
    ui::{
        components::{
            input_dialog::InputDialog,
            loading,
            money::{flow_cap_line_gauge, styled_amount_no_sign, styled_progress_bar},
        },
        theme::Theme,
    },
};

/// Transaction type icons
const ICON_INCOME: &str = "▲";
const ICON_EXPENSE: &str = "▼";
const ICON_REFUND: &str = "↩";
const ICON_TRANSFER: &str = "⇄";

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    match state.flows.mode {
        FlowsMode::Detail => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_list(frame, columns[0], state, &theme);
            render_detail(frame, columns[1], state, &theme);
        }
        FlowsMode::Create | FlowsMode::Rename | FlowsMode::List => {
            render_list(frame, area, state, &theme)
        }
    }

    if state.flows.mode == FlowsMode::Rename {
        render_rename_dialog(frame, area, state, &theme);
    }
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let show_form = state.flows.mode == FlowsMode::Create;

    let constraints = if show_form {
        vec![Constraint::Length(8), Constraint::Min(0)]
    } else {
        vec![Constraint::Min(0)]
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let list_area = if show_form {
        render_form(frame, layout[0], state, theme);
        layout[1]
    } else {
        layout[0]
    };

    // Search bar in header
    let search_active = state.flows.search_active;
    let search_query = state.flows.search_query.trim();

    let header_spans = if search_active || !search_query.is_empty() {
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

    let items = visible
        .iter()
        .filter_map(|idx| snapshot.flows.get(*idx))
        .map(|flow| {
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

            ListItem::new(Line::from(spans))
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
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No budgets or goals yet",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[c]", Style::default().fg(theme.accent)),
                    Span::styled(
                        " to create your first budget",
                        Style::default().fg(theme.text_muted),
                    ),
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

fn render_rename_dialog(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return;
    };
    let indices = flows_visible_indices(state);
    let Some(index) = indices.get(state.flows.selected).copied() else {
        return;
    };
    let Some(flow) = snapshot.flows.get(index) else {
        return;
    };

    let dialog = InputDialog {
        title: "Rename Flow",
        current_label: Some("Current:"),
        current_value: Some(flow.name.as_str()),
        prompt: "New name:",
        value: state.flows.form.name.as_str(),
        focused: state.flows.form.focus == FlowFormField::Name,
        error: state.flows.form.error.as_deref(),
        confirm_label: "Save",
        cancel_label: "Cancel",
    };

    crate::ui::components::input_dialog::render(frame, area, dialog, theme);
}

fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.flows.form;
    let is_rename = state.flows.mode == FlowsMode::Rename;

    let title = if is_rename {
        " Rename Flow "
    } else {
        " New Budget/Goal "
    };

    let mut lines = vec![
        Line::from(""),
        render_field(
            "Name",
            form.name.as_str(),
            form.focus == FlowFormField::Name,
            theme,
        ),
    ];

    if !is_rename {
        lines.push(render_field(
            "Type",
            form.mode.label(),
            form.focus == FlowFormField::Mode,
            theme,
        ));
        lines.push(render_field(
            "Cap",
            if matches!(form.mode, FlowModeChoice::Unlimited) {
                "-"
            } else {
                form.cap.as_str()
            },
            form.focus == FlowFormField::Cap,
            theme,
        ));
        lines.push(render_field(
            "Opening",
            form.opening.as_str(),
            form.focus == FlowFormField::Opening,
            theme,
        ));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(theme.accent)),
        Span::styled(
            if is_rename { " save  " } else { " create  " },
            Style::default().fg(theme.text_muted),
        ),
        Span::styled("[Tab]", Style::default().fg(theme.accent)),
        Span::styled(" next  ", Style::default().fg(theme.text_muted)),
        if !is_rename {
            Span::styled("[M]", Style::default().fg(theme.accent))
        } else {
            Span::raw("")
        },
        if !is_rename {
            Span::styled(" toggle type  ", Style::default().fg(theme.text_muted))
        } else {
            Span::raw("")
        },
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(" cancel", Style::default().fg(theme.text_muted)),
    ]));

    if let Some(err) = form.error.as_ref() {
        lines.push(Line::from(Span::styled(
            format!("⚠ {err}"),
            Style::default().fg(theme.negative),
        )));
    }

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(theme.accent)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty(frame, area, theme, "Loading...");
        return;
    };
    let Some(detail_id) = state.flows.detail.flow_id else {
        render_empty(frame, area, theme, "Select a flow to view details");
        return;
    };
    let Some(flow) = snapshot.flows.iter().find(|flow| flow.id == detail_id) else {
        render_empty(frame, area, theme, "Flow not found");
        return;
    };

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let cap_line = state
        .flows
        .detail
        .detail
        .as_ref()
        .and_then(|detail| cap_progress_line(detail, currency, theme));
    let cap_gauge = state
        .flows
        .detail
        .detail
        .as_ref()
        .and_then(|detail| cap_line_gauge(detail, theme));
    let header_height = if cap_line.is_some() || cap_gauge.is_some() {
        8
    } else {
        7
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(header_height), Constraint::Min(0)])
        .split(area);

    let balance_color = if flow.balance_minor >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    let emoji = if flow.is_unallocated { "📦" } else { "🎯" };

    let mut status_spans = vec![];
    if flow.is_unallocated {
        status_spans.push(Span::styled("[default]", Style::default().fg(theme.info)));
        status_spans.push(Span::raw("  "));
    }
    if flow.archived {
        status_spans.push(Span::styled(
            "[archived]",
            Style::default().fg(theme.warning),
        ));
    } else {
        status_spans.push(Span::styled(
            "[active]",
            Style::default().fg(theme.positive),
        ));
    }

    let mut header_lines = vec![
        Line::from(""),
        Line::from(
            vec![
                Span::raw(format!("  {emoji} ")),
                Span::styled(
                    &flow.name,
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
            ]
            .into_iter()
            .chain(status_spans)
            .collect::<Vec<_>>(),
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Balance: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                Money::new(flow.balance_minor).format(currency),
                Style::default()
                    .fg(balance_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    if let Some(line) = cap_line {
        header_lines.push(line);
    }

    let header_block = Block::default()
        .title(Span::styled(
            " Flow Detail ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let header_inner = header_block.inner(layout[0]);
    frame.render_widget(header_block, layout[0]);

    if let Some(gauge) = cap_gauge {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(header_inner);
        frame.render_widget(Paragraph::new(header_lines), split[0]);
        frame.render_widget(gauge, split[1]);
    } else {
        frame.render_widget(Paragraph::new(header_lines), header_inner);
    }

    // Recent transactions
    if let Some(err) = state.flows.detail.error.as_ref() {
        let block = Block::default()
            .title(Span::styled(
                " Recent Transactions ",
                Style::default().fg(theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.negative));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("⚠ {err}"),
                Style::default().fg(theme.negative),
            )))
            .alignment(Alignment::Center)
            .block(block),
            layout[1],
        );
        return;
    }

    let items = state
        .flows
        .detail
        .transactions
        .iter()
        .map(|tx| {
            let when = tx.occurred_at.format("%d %b %H:%M").to_string();
            let note = tx.note.as_deref().unwrap_or("-");

            let (icon, icon_color) = match tx.kind {
                TransactionKind::Income => (ICON_INCOME, theme.income),
                TransactionKind::Expense => (ICON_EXPENSE, theme.expense),
                TransactionKind::Refund => (ICON_REFUND, theme.refund),
                TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                    (ICON_TRANSFER, theme.transfer)
                }
            };

            let amount_color = match tx.kind {
                TransactionKind::Income | TransactionKind::Refund => theme.positive,
                TransactionKind::Expense => theme.negative,
                _ => theme.text,
            };

            let line = Line::from(vec![
                Span::raw("  "),
                Span::styled(when, Style::default().fg(theme.text_muted)),
                Span::raw("  "),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(
                    format!("{:>10}", Money::new(tx.amount_minor).format(currency)),
                    Style::default().fg(amount_color),
                ),
                Span::raw("  "),
                Span::styled(note, Style::default().fg(theme.text)),
            ]);
            ListItem::new(line)
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        let block = Block::default()
            .title(Span::styled(
                " Recent Transactions ",
                Style::default().fg(theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border));
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No transactions for this flow",
                    Style::default().fg(theme.text_muted),
                )),
            ])
            .alignment(Alignment::Center)
            .block(block),
            layout[1],
        );
        return;
    }

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " Recent Transactions ",
                Style::default().fg(theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border)),
    );
    frame.render_widget(list, layout[1]);
}

fn render_field(label: &str, value: &str, focused: bool, theme: &Theme) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };
    let value_style = if focused {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    let cursor = if focused { "_" } else { "" };

    Line::from(vec![
        Span::styled(format!("  {label:<10}"), label_style),
        Span::raw(": "),
        Span::styled(value.to_string(), value_style),
        Span::styled(cursor, Style::default().fg(theme.accent)),
    ])
}

fn render_empty(frame: &mut Frame<'_>, area: Rect, theme: &Theme, message: &str) {
    let block = Block::default()
        .title(Span::styled(
            " Flow Detail ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(message, Style::default().fg(theme.text_muted))),
        ])
        .alignment(Alignment::Center)
        .block(block),
        area,
    );
}

fn progress_bar(value: i64, max: i64, width: usize) -> String {
    if max == 0 {
        return "░".repeat(width);
    }

    let ratio = (value.unsigned_abs() as f64 / max.unsigned_abs() as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64) as usize).min(width);
    let empty = width.saturating_sub(filled);

    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}

fn cap_progress_line(
    detail: &engine::CashFlow,
    currency: Currency,
    theme: &Theme,
) -> Option<Line<'static>> {
    let cap = detail.max_balance?;
    if cap <= 0 {
        return None;
    }

    let (label, current) = if let Some(income_total_minor) = detail.income_balance {
        ("Income cap", income_total_minor)
    } else {
        ("Net cap", detail.balance)
    };

    let current = current.max(0);
    let bar = styled_progress_bar(current, Some(cap), 20, theme);
    let current_fmt = styled_amount_no_sign(current, currency, theme);
    let cap_fmt = styled_amount_no_sign(cap, currency, theme);

    Some(Line::from(vec![
        Span::styled(format!("  {label}"), Style::default().fg(theme.text_muted)),
        Span::raw(": "),
        current_fmt,
        Span::raw(" / "),
        cap_fmt,
        Span::raw(" "),
        bar,
    ]))
}

fn cap_line_gauge(
    detail: &engine::CashFlow,
    theme: &Theme,
) -> Option<ratatui::widgets::LineGauge<'static>> {
    let cap = detail.max_balance?;
    if cap <= 0 {
        return None;
    }
    let current = if let Some(income_total_minor) = detail.income_balance {
        income_total_minor
    } else {
        detail.balance
    };
    flow_cap_line_gauge(current.max(0), Some(cap), theme)
}
