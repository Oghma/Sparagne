//! Flow list rendering with items and stats header.

use ratatui::{Frame, layout::Rect};

use engine::Money;

use crate::{
    app::{AppState, EntityListMode, flows_visible_indices},
    text::{TextKey, t},
    ui::{
        common::get_currency,
        screens::entity_list::{EntityItem, EntityListConfig, render_entity_list},
        theme::Theme,
    },
};

use super::form::render_form;

/// Render the flow list view.
pub fn render_list(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    focused: bool,
) {
    let show_form = state.flows.mode == EntityListMode::Create;

    let total_balance: i64 = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.flows
                .iter()
                .filter(|f| !f.archived)
                .map(|f| f.balance_minor)
                .sum()
        })
        .unwrap_or(0);

    let currency = get_currency(state);
    let visible = flows_visible_indices(state);
    let has_snapshot = state.snapshot.is_some();

    let max_balance = total_balance.unsigned_abs().max(1) as i64;

    let items: Vec<EntityItem<'_>> = if let Some(snap) = state.snapshot.as_ref() {
        visible
            .iter()
            .filter_map(|idx| snap.flows.get(*idx))
            .map(|f| {
                let mut extra_badges = Vec::new();
                if f.is_unallocated {
                    extra_badges.push((t(state.locale, TextKey::EntityBadgeDefault), theme.info));
                }
                if f.allow_negative {
                    extra_badges.push((
                        t(state.locale, TextKey::FlowBadgeAllowNegative),
                        theme.warning,
                    ));
                }

                // Cap badge
                if let Some(cap) = f.max_balance {
                    let cap_text = if f.income_balance.is_some() {
                        format!("Income cap: {}", Money::new(cap).format(currency))
                    } else {
                        format!("Cap: {}", Money::new(cap).format(currency))
                    };
                    extra_badges.push((Box::leak(cap_text.into_boxed_str()), theme.accent));
                }

                EntityItem {
                    name: &f.name,
                    balance_minor: f.balance_minor,
                    archived: f.archived,
                    extra_badges,
                    emoji: if f.is_unallocated {
                        "\u{1f4e6}"
                    } else {
                        "\u{1f3af}"
                    },
                }
            })
            .collect()
    } else {
        vec![]
    };

    let locale = state.locale;
    let config = EntityListConfig {
        title: t(locale, TextKey::FlowTitle),
        form_height: 9,
        welcome_title: t(locale, TextKey::FlowWelcomeTitle),
        welcome_desc: &[
            t(locale, TextKey::FlowWelcomeDesc1),
            t(locale, TextKey::FlowWelcomeDesc2),
        ],
        welcome_hints: &[
            ("[c]", t(locale, TextKey::FlowHintQuickCreate)),
            ("[n]", t(locale, TextKey::FlowHintCreateCap)),
        ],
        border_color: if focused { theme.accent } else { theme.border },
    };

    render_entity_list(
        frame,
        area,
        &config,
        show_form,
        &|f, a| render_form(f, a, state, theme),
        state.flows.search.active,
        state.flows.search.query.trim(),
        state.flows.show_archived,
        state.flows.selected,
        &items,
        max_balance,
        currency,
        state.spinner.index(),
        has_snapshot,
        state.locale,
        theme,
    );
}
