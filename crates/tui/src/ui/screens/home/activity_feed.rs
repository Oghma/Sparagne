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
    text::{TextKey, t},
    ui::{
        common::format_date_label,
        components::{card::Card, money::money_emoji},
        theme::Theme,
    },
};

use crate::ui::common::tx_icon_color;

use super::common::{get_currency, render_empty_state, truncate};

/// Renders the activity feed showing recent transactions and alerts.
pub fn render_activity_feed(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new(t(state.locale, TextKey::HomeActivityFeed), theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let feed_items = home_feed_items(state);
    if feed_items.is_empty() {
        render_empty_state(
            frame,
            inner,
            t(state.locale, TextKey::HomeNoActivityYet),
            t(state.locale, TextKey::HomeAddFirstTxn),
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

    let cat_col_width: usize = 14; // 2 for 🏷 + 12 for text
    let amount_col: usize = 14; // fits "+18277.22 EUR" right-aligned
    let emoji_col: usize = if state.emoji_mode { 3 } else { 0 }; // emoji(2) + space(1)
    let time_col: usize = if inner.width >= 50 { 7 } else { 0 }; // 2 space + 5 HH:MM
    let show_time = inner.width >= 50;
    let show_meta = inner.width >= 70;
    let left_fixed: usize = 2 + 1 + 1 + emoji_col + amount_col + time_col + 2; // indent + icon(1) + space + emoji + amount + time + space
    let note_width = if show_meta {
        (inner.width as usize)
            .saturating_sub(left_fixed + cat_col_width)
            .max(8)
    } else {
        (inner.width as usize).saturating_sub(left_fixed).max(8)
    };

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
                    if last_date.is_some() {
                        items.push(ListItem::new(Line::from("")));
                    }
                    let date_label = format_date_label(tx_date, today, yesterday, state.locale);
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

                let (icon, icon_color) = tx_icon_color(tx.kind, theme);

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

                // Layout: icon → [emoji] → amount (fixed-width) → time → note → [category]
                let money = Money::new(amount);
                let formatted = money.format(currency);
                let (amount_color, prefix) = if amount > 0 {
                    (theme.positive, "+")
                } else if amount < 0 {
                    (theme.negative, "")
                } else {
                    (theme.text, "")
                };
                let amount_text = format!("{:>amount_col$}", format!("{prefix}{formatted}"));

                let mut line = vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                ];
                if state.emoji_mode {
                    line.push(Span::styled(
                        format!("{} ", money_emoji(amount)),
                        Style::default().fg(amount_color),
                    ));
                }
                line.push(Span::styled(
                    amount_text,
                    Style::default().fg(amount_color),
                ));

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
                    let used_left = left_fixed + note_width;
                    let pad = (inner.width as usize).saturating_sub(used_left + cat_col_width);
                    line.push(Span::raw(" ".repeat(pad)));
                    if let Some(category) = category {
                        line.push(Span::styled(
                            format!("🏷{}", truncate(category, cat_col_width.saturating_sub(2))),
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
    let income = state.stats.current_month_income;
    let expenses = state.stats.current_month_expenses;
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
