//! Transaction form rendering (create/edit).
//!
//! Displays:
//! - Form fields (amount, wallet, flow, category, note, when)
//! - Field validation and focus states
//! - Wallet and flow picker lists
//! - Recent categories list
//! - Keyboard hints

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
    text::{TextKey, t},
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

    let locale = state.locale;
    let is_edit = form.editing_id.is_some();
    let title = match form.kind {
        TransactionKind::Income => {
            if is_edit {
                t(locale, TextKey::FormEditIncome)
            } else {
                t(locale, TextKey::FormNewIncome)
            }
        }
        TransactionKind::Expense => {
            if is_edit {
                t(locale, TextKey::FormEditExpense)
            } else {
                t(locale, TextKey::FormNewExpense)
            }
        }
        TransactionKind::Refund => {
            if is_edit {
                t(locale, TextKey::FormEditRefund)
            } else {
                t(locale, TextKey::FormNewRefund)
            }
        }
        TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
            if is_edit {
                t(locale, TextKey::FormEditTransaction)
            } else {
                t(locale, TextKey::FormNewTransaction)
            }
        }
    };

    let popup = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(popup);

    render_form_fields(
        frame,
        layout[0],
        state,
        &FormDisplayValues {
            title,
            wallet_name,
            flow_name,
            category: &category,
            note: &note,
            occurred_at: &occurred_at,
        },
        theme,
    );
    render_form_bottom(frame, layout[1], state, &wallets, &flows, theme);
}

/// Display values for the transaction form fields.
struct FormDisplayValues<'a> {
    title: &'a str,
    wallet_name: &'a str,
    flow_name: &'a str,
    category: &'a str,
    note: &'a str,
    occurred_at: &'a str,
}

/// Renders the form fields section
fn render_form_fields(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    values: &FormDisplayValues<'_>,
    theme: &Theme,
) {
    let form = &state.transactions.form;

    let locale = state.locale;
    let mut lines = vec![
        render_form_field(
            t(locale, TextKey::FormAmount),
            form.amount.value(),
            form.focus == TransactionFormField::Amount,
            t(locale, TextKey::FormHelperAmount),
            theme,
        ),
        render_form_field(
            t(locale, TextKey::FormWallet),
            values.wallet_name,
            form.focus == TransactionFormField::Wallet,
            t(locale, TextKey::FormHelperWallet),
            theme,
        ),
        render_form_field(
            t(locale, TextKey::FormFlow),
            values.flow_name,
            form.focus == TransactionFormField::Flow,
            t(locale, TextKey::FormHelperFlow),
            theme,
        ),
        render_form_field(
            t(locale, TextKey::FormCategory),
            values.category,
            form.focus == TransactionFormField::Category,
            t(locale, TextKey::FormHelperCategory),
            theme,
        ),
        render_form_field(
            t(locale, TextKey::FormNote),
            values.note,
            form.focus == TransactionFormField::Note,
            t(locale, TextKey::FormHelperNote),
            theme,
        ),
        render_form_field(
            t(locale, TextKey::FormWhen),
            values.occurred_at,
            form.focus == TransactionFormField::OccurredAt,
            t(locale, TextKey::FormHelperWhen),
            theme,
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(theme.accent)),
            Span::styled(t(locale, TextKey::FormHintSave), Style::default().fg(theme.text_muted)),
            Span::styled("[Esc]", Style::default().fg(theme.accent)),
            Span::styled(t(locale, TextKey::FormHintCancel), Style::default().fg(theme.text_muted)),
            Span::styled("[Tab]", Style::default().fg(theme.accent)),
            Span::styled(t(locale, TextKey::FormHintNextField), Style::default().fg(theme.text_muted)),
            Span::styled("[↑↓]", Style::default().fg(theme.accent)),
            Span::styled(t(locale, TextKey::FormHintCycleChoices), Style::default().fg(theme.text_muted)),
        ]),
    ];

    if let Some(err) = form.error.as_ref() {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.negative),
        )));
    }

    let block = Block::default()
        .title(values.title)
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
        PickerListConfig {
            title: t(state.locale, TextKey::SectionWallets),
            items: wallets
                .iter()
                .map(|wallet| wallet.name.as_str())
                .collect(),
            selected: form.wallet_index,
            focused: form.focus == TransactionFormField::Wallet,
            empty_text: t(state.locale, TextKey::UiNoElement),
        },
        theme,
    );
    render_picker_list(
        frame,
        list_layout[1],
        PickerListConfig {
            title: t(state.locale, TextKey::SectionFlows),
            items: flows
                .iter()
                .map(|flow| flow.name.as_str())
                .collect(),
            selected: form.flow_index,
            focused: form.focus == TransactionFormField::Flow,
            empty_text: t(state.locale, TextKey::UiNoElement),
        },
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
    let helper_style = Style::default().fg(theme.text_muted);
    Line::from(vec![
        Span::styled(format!("{label:<10}"), label_style),
        Span::styled(format!("[{value}{cursor}]"), value_style),
        Span::raw("  "),
        Span::styled(format!("← {helper}"), helper_style),
    ])
}

/// Configuration for a picker list widget.
struct PickerListConfig<'a> {
    title: &'a str,
    items: Vec<&'a str>,
    selected: usize,
    focused: bool,
    empty_text: &'a str,
}

/// Renders a picker list (wallets or flows)
fn render_picker_list(
    frame: &mut Frame<'_>,
    area: Rect,
    config: PickerListConfig<'_>,
    theme: &Theme,
) {
    let block = Block::default()
        .title(config.title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background));
    if config.items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(config.empty_text))
                .alignment(ratatui::layout::Alignment::Center)
                .block(block),
            area,
        );
        return;
    }

    let items = config
        .items
        .into_iter()
        .map(|name| ListItem::new(Line::from(name.to_string())))
        .collect::<Vec<_>>();
    let mut list_state = ListState::default();
    list_state.select(Some(config.selected.min(items.len() - 1)));

    let highlight_style = if config.focused {
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
        .title(t(state.locale, TextKey::UiRecentCategories))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background));

    if state.transactions.recent_categories.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(t(state.locale, TextKey::UiNoRecentCategories)))
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
    let line = Line::from(Span::styled(text, Style::default().fg(theme.text_muted)));
    frame.render_widget(Paragraph::new(line), area);
}
