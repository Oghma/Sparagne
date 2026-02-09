use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::{
    app::AppState,
    ui::{common::themed_block, theme::Theme},
};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let block = themed_block("🏦 Vaults", theme.border_focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items = state
        .vault_ui
        .list
        .items
        .iter()
        .map(|vault| {
            let is_shared = vault.shared;
            let name = if is_shared {
                format!("{} ({})", vault.name, vault.owner)
            } else {
                vault.name.clone()
            };
            let name_style = if is_shared {
                Style::default().fg(theme.text_muted)
            } else {
                Style::default().fg(theme.text)
            };
            let currency = format!("{:?}", vault.currency);
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled("🏦 ", Style::default().fg(theme.text_muted)),
                Span::styled(name, name_style),
                Span::raw("  "),
                Span::styled(currency, Style::default().fg(theme.text_muted)),
            ]))
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No vaults",
                Style::default().fg(theme.text_muted),
            ))
            .alignment(Alignment::Center),
            inner,
        );
    } else {
        let mut list_state = ListState::default();
        list_state.select(Some(
            state
                .vault_ui
                .list
                .selected
                .min(items.len().saturating_sub(1)),
        ));
        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("» ");
        frame.render_stateful_widget(list, inner, &mut list_state);
    }
}
