//! Flow list rendering with items and stats header.

use ratatui::{Frame, layout::Rect};

use crate::{
    app::{AppState, EntityListMode, flows_visible_indices},
    text::{TextKey, t},
    ui::{
        common::get_currency,
        screens::entity_list::{EntityItem, EntityListConfig, EntityListStats, render_entity_list},
        theme::Theme,
    },
};

use super::form::render_form;

/// Render the flow list view.
pub fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let show_form = state.flows.mode == EntityListMode::Create;

    let (total_balance, flow_count, archived_count) = state
        .snapshot
        .as_ref()
        .map(|snap| {
            let balance: i64 = snap
                .flows
                .iter()
                .filter(|f| !f.archived)
                .map(|f| f.balance_minor)
                .sum();
            let count = snap.flows.iter().filter(|f| !f.archived).count();
            let archived = snap.flows.iter().filter(|f| f.archived).count();
            (balance, count, archived)
        })
        .unwrap_or((0, 0, 0));

    let currency = get_currency(state);
    let visible = flows_visible_indices(state);
    let has_snapshot = state.snapshot.is_some();

    let max_balance = state
        .snapshot
        .as_ref()
        .map(|snap| {
            snap.flows
                .iter()
                .map(|f| f.balance_minor.unsigned_abs())
                .max()
                .unwrap_or(1) as i64
        })
        .unwrap_or(1);

    let items: Vec<EntityItem<'_>> = if let Some(snap) = state.snapshot.as_ref() {
        visible
            .iter()
            .filter_map(|idx| snap.flows.get(*idx))
            .map(|f| {
                let mut extra_badges = Vec::new();
                if f.is_unallocated {
                    extra_badges.push((t(state.locale, TextKey::EntityBadgeDefault), theme.info));
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
        stats_label: t(locale, TextKey::FlowStatsLabel),
        entity_label: t(locale, TextKey::FlowEntityLabel),
        form_height: 8,
        item_hints: &[
            ("[e]", "dit "),
            ("[m]", "ode "),
            ("[a]", "rchive "),
            ("[Enter]", " details"),
        ],
        welcome_title: t(locale, TextKey::FlowWelcomeTitle),
        welcome_desc: &[
            t(locale, TextKey::FlowWelcomeDesc1),
            t(locale, TextKey::FlowWelcomeDesc2),
        ],
        welcome_hints: &[
            ("[c]", t(locale, TextKey::FlowHintQuickCreate)),
            ("[n]", t(locale, TextKey::FlowHintCreateCap)),
        ],
    };

    let stats = EntityListStats {
        total_balance,
        count: flow_count,
        archived_count,
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
        &stats,
        state.flows.selected,
        state.flows.mode == EntityListMode::List,
        &items,
        max_balance,
        currency,
        state.spinner.index(),
        has_snapshot,
        state.locale,
        theme,
    );
}
