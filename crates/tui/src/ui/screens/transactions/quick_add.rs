//! Quick-add input bar rendering.
//!
//! Displays:
//! - Input field with cursor
//! - Live preview of parsed transaction
//! - Ambiguous field disambiguation UI
//! - Syntax hints and examples
//! - Envelope suggestions

use engine::Money;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::{
    app::{
        AppState, QuickAddAmbiguousKind, flow_name_suggestions, resolve_category_matches,
        resolve_flow_matches, resolve_wallet_matches,
    },
    quick_add::{QuickAddKind, parse},
    text::{TextKey, t},
    ui::theme::Theme,
};

use super::common::default_wallet_flow_names;
use crate::ui::common::{get_currency, themed_block};

/// Renders the quick-add input bar at the top of the transaction list
pub fn render_quick_add(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let (default_wallet_name, default_flow_name) = default_wallet_flow_names(state);
    let currency = get_currency(state);

    let input = state.transactions.quick_input.as_str();

    // Try to parse the input for live preview
    let parsed = if !input.trim().is_empty() {
        parse(input, currency, state.locale).ok()
    } else {
        None
    };

    let border_color = if state.transactions.quick_active {
        theme.accent
    } else {
        theme.border
    };

    let locale = state.locale;
    let placeholder = t(locale, TextKey::QuickAddPlaceholder);
    let (input_text, input_style) = if input.is_empty() {
        (placeholder, Style::default().fg(theme.text_muted))
    } else {
        (input, Style::default().fg(theme.text))
    };

    let cursor = if state.transactions.quick_active {
        "_"
    } else {
        ""
    };

    // First line: input field
    let mut lines = vec![Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent)),
        Span::styled(input_text.to_string(), input_style),
        Span::styled(cursor, Style::default().fg(theme.accent)),
    ])];

    // Show preview or error or help
    if let Some(err) = &state.transactions.quick_error {
        lines.push(Line::from(Span::styled(
            format!("⚠ {err}"),
            Style::default().fg(theme.negative),
        )));
    } else if let Some(p) = &parsed {
        // Show live preview
        let (type_icon, type_color) = match p.kind {
            QuickAddKind::Income => ("▲", theme.positive),
            QuickAddKind::Expense => ("▼", theme.negative),
            QuickAddKind::Refund => ("↩", theme.warning),
            QuickAddKind::TransferWallet | QuickAddKind::TransferFlow => ("⇄", theme.transfer),
        };
        let amount_str = Money::new(p.amount_minor).format(currency);
        let note = p.note.as_deref().unwrap_or("-");

        // Check for ambiguous matches
        let category_matches = p
            .category
            .as_ref()
            .map(|c| resolve_category_matches(state, c))
            .unwrap_or_default();
        let wallet_matches = p
            .wallet
            .as_ref()
            .map(|w| resolve_wallet_matches(state, w))
            .unwrap_or_default();
        let flow_matches = p
            .flow
            .as_ref()
            .map(|f| resolve_flow_matches(state, f))
            .unwrap_or_default();

        // Determine display values considering ambiguous state
        let (category_display, category_style, category_ambiguous) = resolve_ambiguous_display(
            state,
            &p.category,
            &category_matches,
            QuickAddAmbiguousKind::Category,
            "#",
            theme,
        );

        let (wallet_display, wallet_style, wallet_ambiguous) = if p.wallet.is_some() {
            resolve_ambiguous_display(
                state,
                &p.wallet,
                &wallet_matches,
                QuickAddAmbiguousKind::Wallet,
                "@",
                theme,
            )
        } else {
            (
                format!("@{default_wallet_name}"),
                Style::default().fg(theme.text_muted),
                false,
            )
        };

        let (flow_display, flow_style, flow_ambiguous) = if p.flow.is_some() {
            resolve_ambiguous_display(
                state,
                &p.flow,
                &flow_matches,
                QuickAddAmbiguousKind::Flow,
                ">",
                theme,
            )
        } else {
            (
                format!(">{default_flow_name}"),
                Style::default().fg(theme.text_muted),
                false,
            )
        };

        // Build preview line based on transaction type
        if p.kind == QuickAddKind::TransferWallet {
            let from = p.from_wallet.as_deref().unwrap_or("-");
            let to = p.to_wallet.as_deref().unwrap_or("-");
            lines.push(Line::from(vec![
                Span::styled(type_icon, Style::default().fg(type_color)),
                Span::raw(" "),
                Span::styled(amount_str, Style::default().fg(type_color)),
                Span::raw("  "),
                Span::styled(note, Style::default().fg(theme.text)),
                Span::raw("  │  "),
                Span::styled(format!("@{from} → @{to}"), Style::default().fg(theme.transfer)),
                Span::raw("  │  "),
                Span::styled(t(locale, TextKey::QuickAddToday), Style::default().fg(theme.text_muted)),
            ]));
        } else if p.kind == QuickAddKind::TransferFlow {
            let from = p.from_flow.as_deref().unwrap_or("-");
            let to = p.to_flow.as_deref().unwrap_or("-");
            lines.push(Line::from(vec![
                Span::styled(type_icon, Style::default().fg(type_color)),
                Span::raw(" "),
                Span::styled(amount_str, Style::default().fg(type_color)),
                Span::raw("  "),
                Span::styled(note, Style::default().fg(theme.text)),
                Span::raw("  │  "),
                Span::styled(format!(">{from} → >{to}"), Style::default().fg(theme.transfer)),
                Span::raw("  │  "),
                Span::styled(t(locale, TextKey::QuickAddToday), Style::default().fg(theme.text_muted)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(type_icon, Style::default().fg(type_color)),
                Span::raw(" "),
                Span::styled(amount_str, Style::default().fg(type_color)),
                Span::raw("  "),
                Span::styled(note, Style::default().fg(theme.text)),
                Span::raw("  │  "),
                Span::styled(category_display, category_style),
                Span::raw("  │  "),
                Span::styled(flow_display, flow_style),
                Span::raw("  │  "),
                Span::styled(wallet_display, wallet_style),
                Span::raw("  │  "),
                Span::styled(t(locale, TextKey::QuickAddToday), Style::default().fg(theme.text_muted)),
            ]));
        }

        // Show ambiguous options if any
        let has_ambiguous = category_ambiguous || wallet_ambiguous || flow_ambiguous;
        if state.transactions.quick_active && has_ambiguous {
            if let Some(amb) = &state.transactions.quick_ambiguous {
                let options_str = amb
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, (_, name))| {
                        if i == amb.selected {
                            format!("[{name}]")
                        } else {
                            name.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                let kind_prefix = match amb.kind {
                    QuickAddAmbiguousKind::Category => "#",
                    QuickAddAmbiguousKind::Wallet => "@",
                    QuickAddAmbiguousKind::Flow => ">",
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{kind_prefix}? "),
                        Style::default().fg(theme.warning),
                    ),
                    Span::styled(options_str, Style::default().fg(theme.text_muted)),
                    Span::raw("  "),
                    Span::styled("[Ctrl+R]", Style::default().fg(theme.accent)),
                    Span::styled(t(locale, TextKey::QuickAddCycle), Style::default().fg(theme.text_muted)),
                ]));
            } else {
                // Build ambiguous hint for fields with multiple matches
                let mut hints = Vec::new();
                if category_matches.len() > 1 {
                    let names: Vec<&str> = category_matches
                        .iter()
                        .take(3)
                        .map(|(_id, name)| name.as_str())
                        .collect();
                    hints.push(format!("#? {}", names.join(" | ")));
                }
                if wallet_matches.len() > 1 {
                    let names: Vec<&str> = wallet_matches
                        .iter()
                        .take(3)
                        .map(|(_id, name)| name.as_str())
                        .collect();
                    hints.push(format!("@? {}", names.join(" | ")));
                }
                if flow_matches.len() > 1 {
                    let names: Vec<&str> = flow_matches
                        .iter()
                        .take(3)
                        .map(|(_id, name)| name.as_str())
                        .collect();
                    hints.push(format!(">? {}", names.join(" | ")));
                }
                if !hints.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(hints.join("  "), Style::default().fg(theme.warning)),
                        Span::raw("  "),
                        Span::styled("[Ctrl+R]", Style::default().fg(theme.accent)),
                        Span::styled(t(locale, TextKey::QuickAddCycle), Style::default().fg(theme.text_muted)),
                    ]));
                }
            }
        } else if state.transactions.quick_active
            && let Some(flow_query) = p.flow.as_deref()
            && flow_matches
                .first()
                .is_none_or(|(_id, name)| name.to_lowercase() != flow_query.to_lowercase())
        {
            // Show envelope suggestions if not exact match
            let suggestions = flow_name_suggestions(state, flow_query, 3);
            if !suggestions.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("{}{}", t(locale, TextKey::QuickAddEnvelopeSuggestions), suggestions.join(", ")),
                    Style::default().fg(theme.text_muted),
                )));
            }
        }
    } else if state.transactions.quick_active {
        lines.push(Line::from(Span::styled(
            t(locale, TextKey::QuickAddSyntaxHint),
            Style::default().fg(theme.text_muted),
        )));
    } else {
        // Collapsed state - show syntax and shortcuts
        lines.push(Line::from(vec![
            Span::styled("⚡ ", Style::default().fg(theme.warning)),
            Span::styled("Syntax: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                t(locale, TextKey::QuickAddSyntaxShort),
                Style::default().fg(theme.text_muted),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(t(locale, TextKey::QuickAddExamples), Style::default().fg(theme.text_muted)),
            Span::styled("50 lunch #food @main", Style::default().fg(theme.text_muted)),
            Span::styled("  |  ", Style::default().fg(theme.border)),
            Span::styled("+100 salary >income", Style::default().fg(theme.text_muted)),
            Span::styled("  |  ", Style::default().fg(theme.border)),
            Span::styled("r30 refund", Style::default().fg(theme.text_muted)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   [a]", Style::default().fg(theme.accent)),
            Span::styled(" quick add  ", Style::default().fg(theme.text_muted)),
            Span::styled("[i]", Style::default().fg(theme.accent)),
            Span::styled(" income  ", Style::default().fg(theme.text_muted)),
            Span::styled("[e]", Style::default().fg(theme.accent)),
            Span::styled(" expense  ", Style::default().fg(theme.text_muted)),
            Span::styled("[t]", Style::default().fg(theme.accent)),
            Span::styled(" transfer  ", Style::default().fg(theme.text_muted)),
            Span::styled("[?]", Style::default().fg(theme.accent)),
            Span::styled(" help", Style::default().fg(theme.text_muted)),
        ]));
    }

    let block = themed_block(t(locale, TextKey::QuickAddTitle), border_color, theme);
    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, area);
}

/// Helper to resolve display value for potentially ambiguous fields.
/// Returns (display_string, style, is_ambiguous)
fn resolve_ambiguous_display(
    state: &AppState,
    query: &Option<String>,
    matches: &[(Uuid, String)],
    kind: QuickAddAmbiguousKind,
    prefix: &str,
    theme: &Theme,
) -> (String, Style, bool) {
    let Some(query_str) = query else {
        return (
            "-".to_string(),
            Style::default().fg(theme.text_muted),
            false,
        );
    };

    if matches.is_empty() {
        // No matches - show warning
        return (
            format!("?{prefix}{query_str}"),
            Style::default().fg(theme.warning),
            false,
        );
    }

    if matches.len() == 1 {
        // Single match - resolved
        return (
            format!("{prefix}{}", matches[0].1),
            Style::default().fg(theme.accent),
            false,
        );
    }

    // Multiple matches - ambiguous
    // Check if we have a selection in quick_ambiguous
    if let Some(amb) = &state.transactions.quick_ambiguous
        && amb.kind == kind
        && let Some((_, name)) = amb.current()
    {
        return (
            format!("{prefix}{name}"),
            Style::default().fg(theme.warning),
            true,
        );
    }

    // No selection yet - show first match with warning style
    (
        format!("{prefix}{}", matches[0].1),
        Style::default().fg(theme.warning),
        true,
    )
}
