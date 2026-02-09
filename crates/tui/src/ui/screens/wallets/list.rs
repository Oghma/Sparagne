//! Wallet list rendering.

use ratatui::{Frame, layout::Rect};

use crate::{
    app::{AppState, EntityListMode, wallets_visible_indices},
    text::{TextKey, t},
    ui::{
        common::get_currency,
        screens::entity_list::{EntityItem, EntityListConfig, render_entity_list},
        theme::Theme,
    },
};

use super::form::render_form;

/// Renders the wallet list view.
pub fn render_list(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    focused: bool,
) {
    let show_form = state.wallets.mode == EntityListMode::Create;

    let total_balance: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.wallets
                .iter()
                .filter(|w| !w.archived)
                .map(|w| w.balance_minor)
                .sum()
        })
        .unwrap_or(0);

    let currency = get_currency(state);
    let visible = wallets_visible_indices(state);
    let has_snapshot = state.snapshot.is_some();

    let max_balance = total_balance.unsigned_abs().max(1) as i64;

    let items: Vec<EntityItem<'_>> = if let Some(snap) = state.snapshot.as_ref() {
        visible
            .iter()
            .filter_map(|idx| snap.wallets.get(*idx))
            .map(|w| EntityItem {
                name: &w.name,
                balance_minor: w.balance_minor,
                archived: w.archived,
                extra_badges: vec![],
                emoji: "\u{1f4b0}",
            })
            .collect()
    } else {
        vec![]
    };

    let locale = state.locale;
    let config = EntityListConfig {
        title: t(locale, TextKey::WalletTitle),
        form_height: 7,
        welcome_title: t(locale, TextKey::WalletWelcomeTitle),
        welcome_desc: &[
            t(locale, TextKey::WalletWelcomeDesc1),
            t(locale, TextKey::WalletWelcomeDesc2),
        ],
        welcome_hints: &[
            ("[c]", t(locale, TextKey::WalletHintQuickCreate)),
            ("[n]", t(locale, TextKey::WalletHintCreateDetails)),
        ],
        border_color: if focused { theme.accent } else { theme.border },
    };

    render_entity_list(
        frame,
        area,
        &config,
        show_form,
        &|f, a| render_form(f, a, state, theme),
        state.wallets.search.active,
        state.wallets.search.query.trim(),
        state.wallets.show_archived,
        state.wallets.selected,
        &items,
        max_balance,
        currency,
        state.spinner.index(),
        has_snapshot,
        state.locale,
        theme,
    );
}
