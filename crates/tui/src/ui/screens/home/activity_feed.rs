//! Activity feed rendering for home screen.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use api_types::transaction::TransactionKind;
use chrono::{Local, NaiveDate};
use engine::{Currency, Money};

use crate::{
    app::{AppState, FlowAlertSeverity, HomeFeedItem, home_feed_items},
    ui::{
        common::format_date_label,
        components::{card::Card, money::styled_amount_emoji},
        theme::Theme,
    },
};

use super::common::{get_currency, render_empty_state, truncate, ICON_EXPENSE, ICON_INCOME, ICON_REFUND, ICON_TRANSFER};

/// Renders the activity feed showing recent transactions and alerts.
pub fn render_activity_feed(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new("Activity Feed", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let feed_items = home_feed_items(state);
    if feed_items.is_empty() {
        render_empty_state(
            frame,
            inner,
            "No activity yet",
            "[n] to add your first transaction",
            theme,
        );
        return;
    }

    let today = Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);

    let mut items: Vec<ListItem> = Vec::new();
    let mut last_date: Option<NaiveDate> = None;
    let mut selected_row = None;
    let insight = home_insight(state, currency);
    let mut insight_inserted = false;

    let note_width = (inner.width as usize).saturating_sub(30).clamp(14, 32);
    let cat_width = 12usize.min(inner.width as usize);
    let show_time = inner.width >= 50;
    let show_meta = inner.width >= 70;

    for (feed_idx, item) in feed_items.iter().enumerate() {
        match item {
            HomeFeedItem::FlowAlert(alert) => {
                if feed_idx == state.home_feed_selected {
                    selected_row = Some(items.len());
                }
                let balance = Money::new(alert.balance_minor).format(currency);
                let label = match alert.severity {
                    FlowAlertSeverity::Critical => "deficit".to_string(),
                    FlowAlertSeverity::Warning => {
                        let threshold = Money::new(alert.threshold_minor).format(currency);
                        format!("≤ {threshold}")
                    }
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("⚠", Style::default().fg(theme.warning)),
                    Span::raw(" "),
                    Span::styled(
                        "Alert: ".to_string(),
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} ", truncate(&alert.name, 18)),
                        Style::default().fg(theme.text),
                    ),
                    Span::styled(
                        format!("{label} ({balance})"),
                        Style::default().fg(theme.warning),
                    ),
                ])));
            }
            HomeFeedItem::Transaction { index } => {
                let Some(tx) = state.transactions.items.get(*index) else {
                    continue;
                };
                let tx_date = tx.occurred_at.date_naive();
                if last_date != Some(tx_date) {
                    let date_label = format_date_label(tx_date, today, yesterday);
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!("  {date_label}"),
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::BOLD),
                    ))));
                    last_date = Some(tx_date);
                    if !insight_inserted && let Some(insight_text) = insight.as_ref() {
                        items.push(ListItem::new(Line::from(Span::styled(
                            insight_text.clone(),
                            Style::default().fg(theme.info),
                        ))));
                        insight_inserted = true;
                    }
                }

                let (icon, icon_color) = match tx.kind {
                    TransactionKind::Income => (ICON_INCOME, theme.income),
                    TransactionKind::Expense => (ICON_EXPENSE, theme.expense),
                    TransactionKind::Refund => (ICON_REFUND, theme.refund),
                    TransactionKind::TransferWallet | TransactionKind::TransferFlow => {
                        (ICON_TRANSFER, theme.transfer)
                    }
                };

                let amount = if tx.kind == TransactionKind::Expense {
                    -tx.amount_minor.abs()
                } else {
                    tx.amount_minor
                };

                let note = tx.note.as_deref().unwrap_or("-");
                let category = tx.category.as_deref();
                let time = tx.occurred_at.format("%H:%M").to_string();

                if feed_idx == state.home_feed_selected {
                    selected_row = Some(items.len());
                }

                // Layout: icon → amount → time → note → [category]
                let mut line = vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    styled_amount_emoji(amount, currency, theme, state.emoji_mode),
                ];

                if show_time {
                    line.push(Span::raw("  "));
                    line.push(Span::styled(time, Style::default().fg(theme.text_muted)));
                }

                line.push(Span::raw("  "));
                line.push(Span::styled(
                    format!("{:<note_width$}", truncate(note, note_width)),
                    Style::default().fg(theme.text),
                ));

                if show_meta {
                    if let Some(category) = category {
                        line.push(Span::raw("  "));
                        line.push(Span::styled(
                            format!("🏷{}", truncate(category, cat_width)),
                            Style::default().fg(theme.accent),
                        ));
                    }
                }

                items.push(ListItem::new(Line::from(line)));
            }
        }
    }

    let mut list_state = ListState::default();
    if let Some(row) = selected_row {
        list_state.select(Some(row));
    }

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");

    frame.render_stateful_widget(list, inner, &mut list_state);
}

fn home_insight(state: &AppState, currency: Currency) -> Option<String> {
    let stats = state.stats.data.as_ref()?;
    let income = stats.total_income_minor;
    let expenses = stats.total_expenses_minor;
    if income == 0 && expenses == 0 {
        return None;
    }
    let net = income - expenses;
    if net > 0 {
        let pct = if income > 0 {
            (net as f64 / income as f64) * 100.0
        } else {
            0.0
        };
        Some(format!(
            "  💡 Insight: You saved {} ({pct:.1}% of income) this month",
            Money::new(net).format(currency)
        ))
    } else if net < 0 {
        Some(format!(
            "  💡 Insight: Expenses exceed income by {} this month",
            Money::new(net.abs()).format(currency)
        ))
    } else {
        None
    }
}

