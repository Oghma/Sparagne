//! Shared rendering logic for entity list screens (wallets, flows).
//!
//! Both the wallets and flows list screens share 95%+ identical structure:
//! stats header, search bar, item list with progress bars, empty state.
//! This module extracts that shared rendering into a config-driven function,
//! leaving each screen as a thin wrapper with its entity-specific details.

use engine::{Currency, Money};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    text::{Locale, TextKey, t},
    ui::{components::loading, theme::Theme},
};

/// An entity item extracted from the snapshot for rendering.
pub(crate) struct EntityItem<'a> {
    pub name: &'a str,
    pub balance_minor: i64,
    pub archived: bool,
    /// Extra badges to render after the progress bar (e.g. "[default]").
    pub extra_badges: Vec<(&'static str, ratatui::style::Color)>,
    /// Emoji icon for this item.
    pub emoji: &'static str,
}

/// Aggregated stats for the entity list header.
pub(crate) struct EntityListStats {
    pub total_balance: i64,
    pub count: usize,
    pub archived_count: usize,
}

/// Configuration for an entity list screen.
pub(crate) struct EntityListConfig<'a> {
    /// Block title (e.g. " Wallets ", " Budgets & Goals ").
    pub title: &'a str,
    /// Stats header label (e.g. "Total:", "Allocated:").
    pub stats_label: &'a str,
    /// Entity count label (e.g. "wallets", "envelopes").
    pub entity_label: &'a str,
    /// Form height when showing the create form.
    pub form_height: u16,
    /// Action hints shown on the selected item line.
    pub item_hints: &'a [(&'static str, &'static str)],
    /// Welcome empty state icon (e.g. "💰 Welcome!").
    pub welcome_title: &'a str,
    /// Welcome empty state description lines.
    pub welcome_desc: &'a [&'a str],
    /// Welcome create hints (pairs of key + label).
    pub welcome_hints: &'a [(&'static str, &'static str)],
}

/// Renders the full entity list screen.
///
/// This is the shared implementation called by both wallets/list.rs and flows/list.rs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_entity_list(
    frame: &mut Frame<'_>,
    area: Rect,
    config: &EntityListConfig<'_>,
    show_form: bool,
    render_form_fn: &dyn Fn(&mut Frame<'_>, Rect),
    search_active: bool,
    search_query: &str,
    show_archived: bool,
    stats: &EntityListStats,
    selected: usize,
    is_list_mode: bool,
    items: &[EntityItem<'_>],
    max_balance: i64,
    currency: Currency,
    spinner_index: usize,
    has_snapshot: bool,
    locale: Locale,
    theme: &Theme,
) {
    let constraints = if show_form {
        vec![
            Constraint::Length(2),
            Constraint::Length(config.form_height),
            Constraint::Min(0),
        ]
    } else {
        vec![Constraint::Length(2), Constraint::Min(0)]
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Render stats header
    render_stats_header(frame, layout[0], config, stats, currency, locale, theme);

    let list_area = if show_form {
        render_form_fn(frame, layout[1]);
        layout[2]
    } else {
        layout[1]
    };

    // Search bar in footer
    let mut header_spans = render_search_header_spans(search_active, search_query, locale, theme);

    if show_archived {
        header_spans.push(Span::styled("  ", Style::default()));
        header_spans.push(Span::styled(
            t(locale, TextKey::EntityArchivedOn),
            Style::default().fg(theme.warning),
        ));
    }

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(config.title, Style::default().fg(theme.accent)))
        .title_bottom(Line::from(header_spans).centered());

    if !has_snapshot {
        loading::render_inline_block(
            frame,
            list_area,
            list_block,
            loading::spinner_frame(spinner_index),
            t(locale, TextKey::LoadingGeneric),
            None,
            theme,
        );
        return;
    }

    // Build list items
    let list_items: Vec<ListItem<'_>> = items
        .iter()
        .enumerate()
        .map(|(list_idx, item)| {
            let is_selected = list_idx == selected;
            let name_style = if item.archived {
                Style::default().fg(theme.text_muted)
            } else {
                Style::default().fg(theme.text)
            };

            let balance_color = if item.balance_minor >= 0 {
                theme.positive
            } else {
                theme.negative
            };

            let bar = crate::ui::common::progress_bar(
                item.balance_minor.unsigned_abs() as i64,
                max_balance,
                10,
            );

            let mut spans = vec![
                Span::raw(format!("  {} ", item.emoji)),
                Span::styled(format!("{:<16}", item.name), name_style),
                Span::styled(
                    format!("{:>12}", Money::new(item.balance_minor).format(currency)),
                    Style::default().fg(balance_color),
                ),
                Span::raw("  "),
                Span::styled(bar, Style::default().fg(theme.accent)),
            ];

            for (badge_text, badge_color) in &item.extra_badges {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(*badge_text, Style::default().fg(*badge_color)));
            }

            if item.archived {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    t(locale, TextKey::EntityBadgeArchived),
                    Style::default().fg(theme.warning),
                ));
            }

            if is_selected && is_list_mode {
                let mut hints = vec![Span::raw("     ")];
                for (key, label) in config.item_hints {
                    hints.push(Span::styled(*key, Style::default().fg(theme.accent)));
                    hints.push(Span::styled(*label, Style::default().fg(theme.text_muted)));
                }
                ListItem::new(vec![Line::from(spans), Line::from(hints)])
            } else {
                ListItem::new(Line::from(spans))
            }
        })
        .collect();

    if list_items.is_empty() {
        render_empty_state(frame, list_area, list_block, search_query, config, locale, theme);
        return;
    }

    let mut list_state = ListState::default();
    list_state.select(Some(selected.min(list_items.len().saturating_sub(1))));

    let list = List::new(list_items)
        .block(list_block)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, list_area, &mut list_state);
}

fn render_search_header_spans<'a>(
    search_active: bool,
    search_query: &'a str,
    locale: Locale,
    theme: &Theme,
) -> Vec<Span<'a>> {
    if search_active || !search_query.is_empty() {
        vec![
            Span::styled(
                t(locale, TextKey::SearchLabel),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(
                if search_query.is_empty() {
                    "..."
                } else {
                    search_query
                },
                Style::default().fg(if search_active {
                    theme.accent
                } else {
                    theme.text
                }),
            ),
            Span::styled(
                t(locale, TextKey::SearchClearShort),
                Style::default().fg(theme.text_muted),
            ),
        ]
    } else {
        vec![
            Span::styled("[c]", Style::default().fg(theme.accent)),
            Span::styled(" create  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Ctrl+F]", Style::default().fg(theme.accent)),
            Span::styled(" search  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Enter]", Style::default().fg(theme.accent)),
            Span::styled(" details", Style::default().fg(theme.text_muted)),
        ]
    }
}

fn render_stats_header(
    frame: &mut Frame<'_>,
    area: Rect,
    config: &EntityListConfig<'_>,
    stats: &EntityListStats,
    currency: Currency,
    locale: Locale,
    theme: &Theme,
) {
    let total_balance = stats.total_balance;
    let count = stats.count;
    let archived_count = stats.archived_count;
    let balance_color = if total_balance >= 0 {
        theme.positive
    } else {
        theme.negative
    };

    let label = config.stats_label;
    let entity_label = config.entity_label;
    let mut spans = vec![
        Span::styled(format!(" {label} "), Style::default().fg(theme.text_muted)),
        Span::styled(
            Money::new(total_balance).format(currency),
            Style::default()
                .fg(balance_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({count} {entity_label})"),
            Style::default().fg(theme.text_muted),
        ),
    ];

    if archived_count > 0 {
        spans.push(Span::styled("  │  ", Style::default().fg(theme.border)));
        spans.push(Span::styled(
            format!("{}{archived_count}", t(locale, TextKey::EntityArchivedCount)),
            Style::default().fg(theme.warning),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_empty_state(
    frame: &mut Frame<'_>,
    area: Rect,
    block: Block<'_>,
    search_query: &str,
    config: &EntityListConfig<'_>,
    locale: Locale,
    theme: &Theme,
) {
    let lines = if !search_query.is_empty() {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::raw(t(locale, TextKey::SearchNoResults)),
                Span::styled(
                    format!("\"{search_query}\""),
                    Style::default().fg(theme.accent),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                t(locale, TextKey::SearchClearHint),
                Style::default().fg(theme.text_muted),
            )),
        ]
    } else {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                config.welcome_title.to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];
        for desc in config.welcome_desc {
            lines.push(Line::from(Span::styled(
                (*desc).to_string(),
                Style::default().fg(theme.text_muted),
            )));
        }
        lines.push(Line::from(""));
        let mut hint_spans = Vec::new();
        for (key, label) in config.welcome_hints {
            hint_spans.push(Span::styled(*key, Style::default().fg(theme.accent)));
            hint_spans.push(Span::styled(*label, Style::default().fg(theme.text_muted)));
        }
        lines.push(Line::from(hint_spans));
        lines
    };

    let empty_msg = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(block);
    frame.render_widget(empty_msg, area);
}
