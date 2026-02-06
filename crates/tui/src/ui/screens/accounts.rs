use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::{
    app::{AccountsTab, AppState, EntityListMode},
    text::{TextKey, t},
    ui::{
        common::inset,
        components::{card::Card, tab_bar, tab_bar::TabBarItem},
        screens,
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {

    // Build breadcrumb based on current state
    let breadcrumb = build_breadcrumb(state, theme);
    let has_breadcrumb = !breadcrumb.is_empty();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(if has_breadcrumb { 1 } else { 0 }),
            Constraint::Min(0),
        ])
        .split(area);

    render_tab_bar(frame, layout[0], state, theme);

    if has_breadcrumb {
        frame.render_widget(Paragraph::new(Line::from(breadcrumb)), layout[1]);
    }

    match state.accounts_tab {
        AccountsTab::Sources => screens::wallets::render(frame, layout[2], state, theme),
        AccountsTab::Envelopes => screens::flows::render(frame, layout[2], state, theme),
        AccountsTab::Goals => render_goals_placeholder(frame, layout[2], state, theme),
    }
}

fn build_breadcrumb<'a>(state: &AppState, theme: &Theme) -> Vec<Span<'a>> {
    let locale = state.locale;
    let mut crumbs = Vec::new();

    match state.accounts_tab {
        AccountsTab::Sources => {
            crumbs.push(Span::styled(t(locale, TextKey::BreadcrumbAccounts), Style::default().fg(theme.text_muted)));
            crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
            crumbs.push(Span::styled(t(locale, TextKey::BreadcrumbSources), Style::default().fg(theme.text_muted)));

            match state.wallets.mode {
                EntityListMode::List => {}
                EntityListMode::Detail => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        t(locale, TextKey::BreadcrumbDetail),
                        Style::default().fg(theme.accent),
                    ));
                }
                EntityListMode::Create => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        t(locale, TextKey::BreadcrumbCreate),
                        Style::default().fg(theme.positive),
                    ));
                }
                EntityListMode::Rename => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        t(locale, TextKey::BreadcrumbRename),
                        Style::default().fg(theme.warning),
                    ));
                }
            }
        }
        AccountsTab::Envelopes => {
            crumbs.push(Span::styled(t(locale, TextKey::BreadcrumbAccounts), Style::default().fg(theme.text_muted)));
            crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
            crumbs.push(Span::styled(t(locale, TextKey::BreadcrumbEnvelopes), Style::default().fg(theme.text_muted)));

            match state.flows.mode {
                EntityListMode::List => {}
                EntityListMode::Detail => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        t(locale, TextKey::BreadcrumbDetail),
                        Style::default().fg(theme.accent),
                    ));
                }
                EntityListMode::Create => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        t(locale, TextKey::BreadcrumbCreate),
                        Style::default().fg(theme.positive),
                    ));
                }
                EntityListMode::Rename => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        t(locale, TextKey::BreadcrumbRename),
                        Style::default().fg(theme.warning),
                    ));
                }
            }
        }
        AccountsTab::Goals => {
            crumbs.push(Span::styled(t(locale, TextKey::BreadcrumbAccounts), Style::default().fg(theme.text_muted)));
            crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
            crumbs.push(Span::styled(t(locale, TextKey::BreadcrumbGoals), Style::default().fg(theme.text_muted)));
        }
    }

    crumbs
}

fn render_tab_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new(t(state.locale, TextKey::AccountsCardTitle), theme);
    let inner = inset(card.inner(area), 1, 0);
    card.render_frame(frame, area);

    // Calculate counts for badges
    let (wallet_count, envelope_count) = state
        .snapshot
        .as_ref()
        .map(|snap| {
            let wallets = snap.wallets.iter().filter(|w| !w.archived).count();
            let flows = snap.flows.iter().filter(|f| !f.archived).count();
            (wallets, flows)
        })
        .unwrap_or((0, 0));

    let items = [
        TabBarItem::new("💰 Sources").with_badge(wallet_count),
        TabBarItem::new("📦 Envelopes").with_badge(envelope_count),
        TabBarItem::new("🎯 Goals"),
    ];

    tab_bar::render(frame, inner, &items, state.accounts_tab.index(), theme);
}

fn render_goals_placeholder(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let locale = state.locale;
    let card = Card::new(t(locale, TextKey::BreadcrumbGoals), theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let lines = vec![
        Line::from(Span::styled(
            t(locale, TextKey::AccountsGoalsPlaceholder),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            t(locale, TextKey::AccountsGoalsHint),
            Style::default().fg(theme.text_muted),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

