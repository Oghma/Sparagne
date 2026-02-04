/// Transaction form rendering (create/edit).
///
/// Displays:
/// - Form fields (amount, wallet, flow, category, note, when)
/// - Field validation and focus states
/// - Wallet and flow picker lists
/// - Recent categories list
/// - Keyboard hints

use api_types::{
    transaction::TransactionKind,
    vault::{FlowView, WalletView},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, TransactionFormField, ordered_flow_ids_from_state, ordered_wallet_ids_from_state},
    ui::{components::centered_rect, theme::Theme},
};

use super::common::recents_line;

/// Renders the transaction form overlay (income/expense/refund)
pub fn render_form_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return;
    };
    let form = &state.transactions.form;
    let wallet_ids = ordered_wallet_ids_from_state(state);
    let flow_ids = ordered_flow_ids_from_state(state);

    let wallets = wallet_ids
        .iter()
        .filter_map(|id| snapshot.wallets.iter().find(|wallet| wallet.id == *id))
        .collect::<Vec<_>>();
    let flows = flow_ids
        .iter()
        .filter_map(|id| snapshot.flows.iter().find(|flow| flow.id == *id))
        .collect::<Vec<_>>();

    let wallet_name = wallets
        .get(form.wallet_index)
        .map(|wallet| wallet.name.as_str())
        .unwrap_or("-");
    let flow_name = flows
        .get(form.flow_index)
        .map(|flow| flow.name.as_str())
        .unwrap_or("-");

    let category_raw = form.category.value().trim().trim_start_matches('#');
    let category = if category_raw.is_empty() {
        "-".to_string()
    } else {
        format!("#{category_raw}")
    };
    let note = if form.note.value().trim().is_empty() {
        "-".to_string()
    } else {
        form.note.value().trim().to_string()
    };
    let occurred_at = if form.occurred_at.value.trim().is_empty() {
        "-".to_string()
    } else {
        form.occurred_at.value.trim().to_string()
    };

    let is_edit = form.editing_id.is_some();
    let title = match form.kind {
        TransactionKind::Income => {
            if is_edit {
                "Edit Income"
            } else {
                "New Income"
            }
        }
        TransactionKind::Expense => {
            if is_edit {
                "Edit Expense"
            } else {
                "New Expense"
            }
        }
        TransactionKind::Refund => {
            if is_edit {
                "Edit Refund"
            } else {
                "New Refund"
            }
        }
        TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
            if is_edit {
                "Edit Transaction"
            } else {
                "New Transaction"
            }
        }
    };

    let popup = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(popup);

    render_form_fields(frame, layout[0], state, title, wallet_name, flow_name, &category, &note, &occurred_at, theme);
    render_form_bottom(frame, layout[1], state, &wallets, &flows, theme);
}

/// Renders the form fields section
fn render_form_fields(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    title: &str,
    wallet_name: &str,
    flow_name: &str,
    category: &str,
    note: &str,
    occurred_at: &str,
    theme: &Theme,
) {
    let form = &state.transactions.form;

    let mut lines = vec![
        render_form_field(
            "Amount",
            form.amount.value(),
            form.focus == TransactionFormField::Amount,
            "Enter numerical amount (required)",
            theme,
        ),
        render_form_field(
            "Wallet",
            wallet_name,
            form.focus == TransactionFormField::Wallet,
            "Source/destination wallet",
            theme,
        ),
        render_form_field(
            "Flow",
            flow_name,
            form.focus == TransactionFormField::Flow,
            "Envelope/budget allocation",
            theme,
        ),
        render_form_field(
            "Category",
            category,
            form.focus == TransactionFormField::Category,
            "Tag for analytics",
            theme,
        ),
        render_form_field(
            "Note",
            note,
            form.focus == TransactionFormField::Note,
            "Optional description",
            theme,
        ),
        render_form_field(
            "When",
            occurred_at,
            form.focus == TransactionFormField::OccurredAt,
            "Date & time (default: now)",
            theme,
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(theme.accent)),
            Span::styled(" Save  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Esc]", Style::default().fg(theme.accent)),
            Span::styled(" Cancel  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Tab]", Style::default().fg(theme.accent)),
            Span::styled(" Next field  ", Style::default().fg(theme.text_muted)),
            Span::styled("[↑↓]", Style::default().fg(theme.accent)),
            Span::styled(" Cycle choices", Style::default().fg(theme.text_muted)),
        ]),
    ];

    if let Some(err) = form.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.background));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Renders the bottom section (wallets, flows, categories, recents)
fn render_form_bottom(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    wallets: &[&WalletView],
    flows: &[&FlowView],
    theme: &Theme,
) {
    let form = &state.transactions.form;

    let bottom_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(5),
            Constraint::Length(2),
        ])
        .split(area);

    let list_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(bottom_layout[0]);

    render_picker_list(
        frame,
        list_layout[0],
        "Wallets",
        wallets
            .iter()
            .map(|wallet| wallet.name.as_str())
            .collect::<Vec<_>>(),
        form.wallet_index,
        form.focus == TransactionFormField::Wallet,
        theme,
    );
    render_picker_list(
        frame,
        list_layout[1],
        "Flows",
        flows
            .iter()
            .map(|flow| flow.name.as_str())
            .collect::<Vec<_>>(),
        form.flow_index,
        form.focus == TransactionFormField::Flow,
        theme,
    );

    render_category_list(frame, bottom_layout[1], state, theme);
    render_recents_footer(frame, bottom_layout[2], state, theme);
}

/// Renders a single form field with label, value, and helper text
fn render_form_field(
    label: &str,
    value: &str,
    focused: bool,
    helper: &str,
    theme: &Theme,
) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    let value_style = if focused {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    let cursor = if focused { "▏" } else { "" };
    let helper_style = if focused {
        Style::default().fg(theme.text_muted)
    } else {
        Style::default().fg(theme.dim)
    };
    Line::from(vec![
        Span::styled(format!("{label:<10}"), label_style),
        Span::styled(format!("[{value}{cursor}]"), value_style),
        Span::raw("  "),
        Span::styled(format!("← {helper}"), helper_style),
    ])
}

/// Renders a picker list (wallets or flows)
fn render_picker_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: Vec<&str>,
    selected: usize,
    focused: bool,
    theme: &Theme,
) {
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background));
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from("Nessun elemento."))
                .alignment(ratatui::layout::Alignment::Center)
                .block(block),
            area,
        );
        return;
    }

    let items = items
        .into_iter()
        .map(|name| ListItem::new(Line::from(name.to_string())))
        .collect::<Vec<_>>();
    let mut list_state = ListState::default();
    list_state.select(Some(selected.min(items.len() - 1)));

    let highlight_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style)
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

/// Renders the recent categories list
fn render_category_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let block = Block::default()
        .title("Categorie recenti")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background));

    if state.transactions.recent_categories.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from("Nessuna categoria recente."))
                .alignment(ratatui::layout::Alignment::Center)
                .block(block),
            area,
        );
        return;
    }

    let items = state
        .transactions
        .recent_categories
        .iter()
        .map(|category| ListItem::new(Line::from(format!("#{category}"))))
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    if let Some(idx) = state.transactions.form.category_index {
        list_state.select(Some(idx.min(items.len() - 1)));
    }

    let highlight_style = if state.transactions.form.focus == TransactionFormField::Category {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style)
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

/// Renders the recents footer line
fn render_recents_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(text) = recents_line(state) else {
        return;
    };
    let line = Line::from(Span::styled(text, Style::default().fg(theme.dim)));
    frame.render_widget(Paragraph::new(line), area);
}
