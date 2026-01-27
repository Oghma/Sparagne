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
    app::{AppState, WalletFormField, WalletsMode, wallets_visible_indices},
    ui::{
        components::{input_dialog::InputDialog, loading},
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

    match state.wallets.mode {
        WalletsMode::Detail => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_list(frame, columns[0], state, &theme);
            render_detail(frame, columns[1], state, &theme);
        }
        WalletsMode::Create | WalletsMode::Rename | WalletsMode::List => {
            render_list(frame, area, state, &theme)
        }
    }

    if state.wallets.mode == WalletsMode::Rename {
        render_rename_dialog(frame, area, state, &theme);
    }
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let show_form = state.wallets.mode == WalletsMode::Create;

    let constraints = if show_form {
        vec![Constraint::Length(7), Constraint::Min(0)]
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
    let search_active = state.wallets.search_active;
    let search_query = state.wallets.search_query.trim();

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

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let visible = wallets_visible_indices(state);

    // Calculate max balance for progress bars
    let max_balance = snapshot
        .wallets
        .iter()
        .map(|w| w.balance_minor.unsigned_abs())
        .max()
        .unwrap_or(1) as i64;

    let items = visible
        .iter()
        .filter_map(|idx| snapshot.wallets.get(*idx))
        .map(|wallet| {
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

            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        let query = state.wallets.search_query.trim();
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
                    "No wallets yet",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[c]", Style::default().fg(theme.accent)),
                    Span::styled(
                        " to create your first wallet",
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

fn render_rename_dialog(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return;
    };
    let indices = wallets_visible_indices(state);
    let Some(index) = indices.get(state.wallets.selected).copied() else {
        return;
    };
    let Some(wallet) = snapshot.wallets.get(index) else {
        return;
    };

    let dialog = InputDialog {
        title: "Rename Wallet",
        current_label: Some("Current:"),
        current_value: Some(wallet.name.as_str()),
        prompt: "New name:",
        value: state.wallets.form.name.as_str(),
        focused: state.wallets.form.focus == WalletFormField::Name,
        error: state.wallets.form.error.as_deref(),
        confirm_label: "Save",
        cancel_label: "Cancel",
    };

    crate::ui::components::input_dialog::render(frame, area, dialog, theme);
}

fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.wallets.form;
    let is_rename = state.wallets.mode == WalletsMode::Rename;

    let title = if is_rename {
        " Rename Wallet "
    } else {
        " New Wallet "
    };

    let mut lines = vec![
        Line::from(""),
        render_field(
            "Name",
            form.name.as_str(),
            form.focus == WalletFormField::Name,
            theme,
        ),
    ];

    if !is_rename {
        lines.push(render_field(
            "Opening",
            form.opening.as_str(),
            form.focus == WalletFormField::Opening,
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
        Span::styled(" next field  ", Style::default().fg(theme.text_muted)),
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
    let Some(detail_id) = state.wallets.detail.wallet_id else {
        render_empty(frame, area, theme, "Select a wallet to view details");
        return;
    };
    let Some(wallet) = snapshot
        .wallets
        .iter()
        .find(|wallet| wallet.id == detail_id)
    else {
        render_empty(frame, area, theme, "Wallet not found");
        return;
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(map_currency)
        .unwrap_or(Currency::Eur);

    let balance_color = if wallet.balance_minor >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    let status = if wallet.archived {
        Span::styled("[archived]", Style::default().fg(theme.warning))
    } else {
        Span::styled("[active]", Style::default().fg(theme.positive))
    };

    let header_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  💰 ", Style::default()),
            Span::styled(
                &wallet.name,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            status,
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Balance: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                Money::new(wallet.balance_minor).format(currency),
                Style::default()
                    .fg(balance_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    let header_block = Block::default()
        .title(Span::styled(
            " Wallet Detail ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(header_lines).block(header_block), layout[0]);

    // Recent transactions
    if let Some(err) = state.wallets.detail.error.as_ref() {
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
        .wallets
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
                    "No transactions for this wallet",
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
            " Wallet Detail ",
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
