//! Recurring templates screen.
//!
//! Shows pending (due) recurring items at the top, followed by the full
//! template list. When in create mode, renders an overlay form.

use api_types::recurring::{RecurrenceFrequency, RecurringKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Row, Table},
};

use crate::{
    app::{AppState, RecurringFormField, RecurringMode},
    text::{TextKey, t},
    ui::{common::themed_block, theme::Theme},
};

/// Main entry point for recurring screen rendering.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    match state.recurring.mode {
        RecurringMode::List => render_list(frame, area, state, theme),
        RecurringMode::Create => {
            render_list(frame, area, state, theme);
            render_form_overlay(frame, area, state, theme);
        }
    }
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;

    let has_pending = !state.recurring.pending.is_empty();
    let constraints = if has_pending {
        vec![
            Constraint::Length(state.recurring.pending.len() as u16 + 3),
            Constraint::Min(0),
        ]
    } else {
        vec![Constraint::Min(0)]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut chunk_idx = 0;

    // Pending section
    if has_pending {
        let pending_block = Block::default()
            .title(Span::styled(
                format!(" {} ({}) ", t(locale, TextKey::RecurringPending), state.recurring.pending.len()),
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.warning));

        let rows: Vec<Row<'_>> = state
            .recurring
            .pending
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let selected = i == state.recurring.selected;
                let kind_icon = match p.template.kind {
                    RecurringKind::Income => "+",
                    RecurringKind::Expense => "-",
                };
                let amount = format_amount(p.template.amount_minor);
                let freq = frequency_label(p.template.frequency, locale);
                let style = if selected {
                    Style::default()
                        .fg(theme.background)
                        .bg(theme.warning)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                };
                Row::new(vec![
                    kind_icon.to_string(),
                    amount,
                    p.template.note.clone().unwrap_or_default(),
                    freq.to_string(),
                    p.period_date.clone(),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(2),
                Constraint::Length(12),
                Constraint::Min(10),
                Constraint::Length(10),
                Constraint::Length(12),
            ],
        )
        .block(pending_block);

        frame.render_widget(table, chunks[chunk_idx]);
        chunk_idx += 1;
    }

    // Templates section
    let templates_block = themed_block(
        t(locale, TextKey::RecurringTitle),
        theme.border,
        theme,
    );

    if state.recurring.templates.is_empty() && !has_pending {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                t(locale, TextKey::RecurringEmpty),
                Style::default().fg(theme.text_muted),
            )),
            Line::from(Span::styled(
                "[c] to create",
                Style::default().fg(theme.text_muted),
            )),
        ])
        .alignment(Alignment::Center)
        .block(templates_block);
        frame.render_widget(empty, chunks[chunk_idx]);
        return;
    }

    let offset = state.recurring.pending.len();
    let rows: Vec<Row<'_>> = state
        .recurring
        .templates
        .iter()
        .enumerate()
        .map(|(i, tmpl)| {
            let selected = (i + offset) == state.recurring.selected;
            let kind_icon = match tmpl.kind {
                RecurringKind::Income => "+",
                RecurringKind::Expense => "-",
            };
            let amount = format_amount(tmpl.amount_minor);
            let freq = frequency_label(tmpl.frequency, locale);
            let enabled_str = if tmpl.enabled {
                t(locale, TextKey::RecurringEnabled)
            } else {
                t(locale, TextKey::RecurringDisabled)
            };
            let style = if selected {
                Style::default()
                    .fg(theme.background)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else if !tmpl.enabled {
                Style::default().fg(theme.text_muted)
            } else {
                Style::default().fg(theme.text)
            };
            Row::new(vec![
                kind_icon.to_string(),
                amount,
                tmpl.note.clone().unwrap_or_default(),
                freq.to_string(),
                format!("d{}", tmpl.day_of_period),
                enabled_str.to_string(),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Min(10),
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(10),
        ],
    )
    .block(templates_block);

    frame.render_widget(table, chunks[chunk_idx]);
}

fn render_form_overlay(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;
    let form = &state.recurring.form;

    // Center a popup
    let popup_width = 50u16.min(area.width.saturating_sub(4));
    let popup_height = 16u16.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", t(locale, TextKey::RecurringFormTitle)),
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let field_constraints: Vec<Constraint> = vec![Constraint::Length(1); 12];
    let fields = Layout::default()
        .direction(Direction::Vertical)
        .constraints(field_constraints)
        .split(inner);

    let kind_label = match form.kind {
        RecurringKind::Income => t(locale, TextKey::RecurringKindIncome),
        RecurringKind::Expense => t(locale, TextKey::RecurringKindExpense),
    };

    let freq_label = frequency_label(form.frequency, locale);

    let wallet_name = resolve_wallet_name(state, form.wallet_index);
    let flow_name = resolve_flow_name(state, form.flow_index);

    let fields_data = [
        (t(locale, TextKey::RecurringFormKind), kind_label, RecurringFormField::Kind),
        (t(locale, TextKey::RecurringFormAmount), form.amount.value(), RecurringFormField::Amount),
        (t(locale, TextKey::FormWallet), wallet_name.as_str(), RecurringFormField::Wallet),
        (t(locale, TextKey::FormFlow), flow_name.as_str(), RecurringFormField::Flow),
        (t(locale, TextKey::FormCategory), form.category.value(), RecurringFormField::Category),
        (t(locale, TextKey::FormNote), form.note.value(), RecurringFormField::Note),
        (t(locale, TextKey::RecurringFormFrequency), freq_label, RecurringFormField::Frequency),
        (t(locale, TextKey::RecurringFormDay), form.day_of_period.value(), RecurringFormField::DayOfPeriod),
        (t(locale, TextKey::RecurringFormStartDate), form.start_date.value(), RecurringFormField::StartDate),
        (t(locale, TextKey::RecurringFormEndDate), form.end_date.value(), RecurringFormField::EndDate),
    ];

    for (i, (label, value, field)) in fields_data.iter().enumerate() {
        if i >= fields.len() {
            break;
        }
        let focused = form.focus == *field;
        let label_style = if focused {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted)
        };
        let cursor = if focused { "▸ " } else { "  " };
        let line = Line::from(vec![
            Span::styled(cursor, label_style),
            Span::styled(format!("{label}: "), label_style),
            Span::styled(value.to_string(), Style::default().fg(theme.text)),
        ]);
        frame.render_widget(Paragraph::new(line), fields[i]);
    }

    // Error line
    if let Some(err) = &form.error
        && fields.len() > 10
    {
        let err_line = Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(theme.negative),
        ));
        frame.render_widget(Paragraph::new(err_line), fields[10]);
    }

    // Hint line
    if fields.len() > 11 {
        let hints = Line::from(vec![
            Span::styled(
                t(locale, TextKey::FormHintSave),
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                t(locale, TextKey::FormHintCancel),
                Style::default().fg(theme.text_muted),
            ),
        ]);
        frame.render_widget(Paragraph::new(hints), fields[11]);
    }
}

fn format_amount(amount_minor: i64) -> String {
    let euros = amount_minor / 100;
    let cents = (amount_minor % 100).unsigned_abs();
    format!("{euros}.{cents:02}")
}

fn frequency_label(freq: RecurrenceFrequency, locale: crate::text::Locale) -> &'static str {
    match freq {
        RecurrenceFrequency::Daily => t(locale, TextKey::RecurringFreqDaily),
        RecurrenceFrequency::Weekly => t(locale, TextKey::RecurringFreqWeekly),
        RecurrenceFrequency::Monthly => t(locale, TextKey::RecurringFreqMonthly),
        RecurrenceFrequency::Yearly => t(locale, TextKey::RecurringFreqYearly),
    }
}

fn resolve_wallet_name(state: &AppState, index: usize) -> String {
    state
        .snapshot
        .as_ref()
        .and_then(|s| s.wallets.get(index))
        .map(|w| w.name.clone())
        .unwrap_or_else(|| "-".to_string())
}

fn resolve_flow_name(state: &AppState, index: usize) -> String {
    state
        .snapshot
        .as_ref()
        .and_then(|s| s.flows.get(index))
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "-".to_string())
}
