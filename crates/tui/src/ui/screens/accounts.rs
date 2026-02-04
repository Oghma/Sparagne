use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::{
    app::{AccountsTab, AppState, FlowsMode, WalletsMode},
    ui::{
        components::{card::Card, tab_bar, tab_bar::TabBarItem},
        screens,
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    // Build breadcrumb based on current state
    let breadcrumb = build_breadcrumb(state, &theme);
    let has_breadcrumb = !breadcrumb.is_empty();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(if has_breadcrumb { 1 } else { 0 }),
            Constraint::Min(0),
        ])
        .split(area);

    render_tab_bar(frame, layout[0], state, &theme);

    if has_breadcrumb {
        frame.render_widget(Paragraph::new(Line::from(breadcrumb)), layout[1]);
    }

    match state.accounts_tab {
        AccountsTab::Sources => screens::wallets::render(frame, layout[2], state),
        AccountsTab::Envelopes => screens::flows::render(frame, layout[2], state),
        AccountsTab::Goals => render_goals_placeholder(frame, layout[2], &theme),
    }
}

fn build_breadcrumb<'a>(state: &AppState, theme: &Theme) -> Vec<Span<'a>> {
    let mut crumbs = Vec::new();

    match state.accounts_tab {
        AccountsTab::Sources => {
            crumbs.push(Span::styled(" Accounts", Style::default().fg(theme.dim)));
            crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
            crumbs.push(Span::styled("Sources", Style::default().fg(theme.text_muted)));

            match state.wallets.mode {
                WalletsMode::List => {}
                WalletsMode::Detail => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        "Detail",
                        Style::default().fg(theme.accent),
                    ));
                }
                WalletsMode::Create => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        "Create",
                        Style::default().fg(theme.positive),
                    ));
                }
                WalletsMode::Rename => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        "Rename",
                        Style::default().fg(theme.warning),
                    ));
                }
            }
        }
        AccountsTab::Envelopes => {
            crumbs.push(Span::styled(" Accounts", Style::default().fg(theme.dim)));
            crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
            crumbs.push(Span::styled("Envelopes", Style::default().fg(theme.text_muted)));

            match state.flows.mode {
                FlowsMode::List => {}
                FlowsMode::Detail => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        "Detail",
                        Style::default().fg(theme.accent),
                    ));
                }
                FlowsMode::Create => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        "Create",
                        Style::default().fg(theme.positive),
                    ));
                }
                FlowsMode::Rename => {
                    crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
                    crumbs.push(Span::styled(
                        "Rename",
                        Style::default().fg(theme.warning),
                    ));
                }
            }
        }
        AccountsTab::Goals => {
            crumbs.push(Span::styled(" Accounts", Style::default().fg(theme.dim)));
            crumbs.push(Span::styled(" > ", Style::default().fg(theme.border)));
            crumbs.push(Span::styled("Goals", Style::default().fg(theme.text_muted)));
        }
    }

    crumbs
}

fn render_tab_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new("Accounts", theme);
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

fn render_goals_placeholder(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let card = Card::new("Goals", theme);
    let inner = card.inner(area);
    card.render_frame(frame, area);

    let lines = vec![
        Line::from(Span::styled(
            "Goals view is coming soon.",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Use Sources for wallets and Envelopes for budgets.",
            Style::default().fg(theme.text_muted),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    let x = area.x.saturating_add(horizontal);
    let y = area.y.saturating_add(vertical);
    let width = area.width.saturating_sub(horizontal.saturating_mul(2));
    let height = area.height.saturating_sub(vertical.saturating_mul(2));
    Rect {
        x,
        y,
        width,
        height,
    }
}
