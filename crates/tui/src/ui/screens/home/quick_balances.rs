//! Quick balances card rendering.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use engine::Money;

use crate::{
    app::AppState,
    text::{TextKey, t},
    ui::{components::card::Card, theme::Theme},
};

use super::common::{get_currency, render_empty_state, truncate};

/// Renders the quick balances card showing wallet and flow summaries.
pub fn render_quick_balances(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let currency = get_currency(state);
    let card = Card::new(t(state.locale, TextKey::HomeQuickBalances), theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let Some(snapshot) = state.snapshot.as_ref() else {
        render_empty_state(
            frame,
            inner,
            t(state.locale, TextKey::HomeNoDataYet),
            t(state.locale, TextKey::HomeAddFirstTxn),
            theme,
        );
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    // Dynamic name width: reserve space for prefix, padding, and amount
    let amount_reserve = 15; // e.g. "+12,345.67 EUR"
    let prefix_width = 5; // "  💰 " or "  📦 "
    let min_pad = 2;
    let max_name_width = (inner.width as usize)
        .saturating_sub(prefix_width + min_pad + amount_reserve)
        .max(8);

    // Wallets section
    let mut wallets: Vec<_> = snapshot.wallets.iter().filter(|w| !w.archived).collect();
    wallets.sort_by(|a, b| b.balance_minor.cmp(&a.balance_minor));

    if !wallets.is_empty() {
        lines.push(Line::from(Span::styled(
            t(state.locale, TextKey::HomeWallets),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::BOLD),
        )));

        // Calculate how much space we have
        let available_height = inner.height as usize;
        let max_wallets = (available_height / 2).max(2).min(wallets.len());

        for wallet in wallets.iter().take(max_wallets) {
            let balance_color = if wallet.balance_minor >= 0 {
                theme.positive
            } else {
                theme.negative
            };
            let amount_str = Money::new(wallet.balance_minor).format(currency);
            let name_len = prefix_width + max_name_width;
            let pad = (inner.width as usize).saturating_sub(name_len + amount_str.len());
            lines.push(Line::from(vec![
                Span::raw("  💰 "),
                Span::styled(
                    format!(
                        "{:<max_name_width$}",
                        truncate(&wallet.name, max_name_width)
                    ),
                    Style::default().fg(theme.text),
                ),
                Span::raw(" ".repeat(pad)),
                Span::styled(amount_str, Style::default().fg(balance_color)),
            ]));
        }
    }

    // Budgets section (flows that are not archived and not "Unallocated")
    let flows: Vec<_> = snapshot
        .flows
        .iter()
        .filter(|f| !f.archived && !f.is_unallocated)
        .collect();

    if !flows.is_empty() {
        let remaining = inner.height as usize - lines.len();
        if remaining > 2 {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                t(state.locale, TextKey::HomeBudgets),
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::BOLD),
            )));

            let max_flows = (remaining - 3).max(1).min(flows.len());

            for flow in flows.iter().take(max_flows) {
                let balance_color = if flow.balance_minor >= 0 {
                    theme.positive
                } else {
                    theme.negative
                };
                let amount_str = Money::new(flow.balance_minor).format(currency);
                let name_len = prefix_width + max_name_width;
                let pad = (inner.width as usize).saturating_sub(name_len + amount_str.len());
                lines.push(Line::from(vec![
                    Span::raw("  📦 "),
                    Span::styled(
                        format!("{:<max_name_width$}", truncate(&flow.name, max_name_width)),
                        Style::default().fg(theme.text),
                    ),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(amount_str, Style::default().fg(balance_color)),
                ]));
            }
        }
    }

    lines.truncate(inner.height as usize);
    frame.render_widget(Paragraph::new(lines), inner);
}
