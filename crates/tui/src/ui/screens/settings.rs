use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{
    app::{AppState, PreferencesField, SettingsTab},
    config::Density,
    ui::{
        common::inset,
        components::{card::Card, tab_bar, tab_bar::TabBarItem},
        screens,
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Tab bar
            Constraint::Length(1), // Spacer
            Constraint::Min(0),    // Content
        ])
        .split(area);

    render_tab_bar(frame, layout[0], state, theme);

    match state.settings_tab {
        SettingsTab::Categories => screens::categories::render(frame, layout[2], state, theme),
        SettingsTab::Vault => screens::vault::render(frame, layout[2], state, theme),
        SettingsTab::Members => screens::members::render(frame, layout[2], state, theme),
        SettingsTab::Preferences => render_preferences(frame, layout[2], state, theme),
    }
}

fn render_tab_bar(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let card = Card::new("Settings", theme);
    let inner = inset(card.inner(area), 1, 0);
    card.render_frame(frame, area);

    let items = [
        TabBarItem::new("1 Categories"),
        TabBarItem::new("2 Vault"),
        TabBarItem::new("3 Members"),
        TabBarItem::new("4 Preferences"),
    ];

    tab_bar::render(frame, inner, &items, state.settings_tab.index(), theme);
}

fn render_preferences(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    let block = Block::default()
        .title(Span::styled(
            " Preferences ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused));
    let inner = block.inner(layout[0]);
    frame.render_widget(block, layout[0]);

    let focus = state.preferences.focus;

    // Emoji Mode toggle
    let emoji_focused = focus == PreferencesField::EmojiMode;
    let emoji_value = if state.emoji_mode { "On" } else { "Off" };
    let emoji_preview = if state.emoji_mode {
        "\u{1F4B0} \u{1F4B8} \u{1F3F7}\u{FE0F} \u{1F4E6}"
    } else {
        "EUR $ # []"
    };

    // Density selection
    let density_focused = focus == PreferencesField::Density;
    let density_label = match state.density {
        Density::Compact => "Compact",
        Density::Normal => "Normal",
        Density::Comfortable => "Comfortable",
    };

    let content_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Emoji mode label
            Constraint::Length(1), // Emoji mode value
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Density label
            Constraint::Length(1), // Density value
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Preview
            Constraint::Min(0),    // Rest
        ])
        .split(inner);

    // Emoji mode
    let emoji_label_style = if emoji_focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Emoji Mode", emoji_label_style),
            if emoji_focused {
                Span::styled("  [Space] toggle", Style::default().fg(theme.text_muted))
            } else {
                Span::raw("")
            },
        ])),
        content_layout[0],
    );

    let emoji_value_style = if emoji_focused {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("    "),
            Span::styled(emoji_value, emoji_value_style),
            Span::raw("  "),
            Span::styled(emoji_preview, Style::default().fg(theme.text_muted)),
        ])),
        content_layout[1],
    );

    // Density
    let density_label_style = if density_focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Density", density_label_style),
            if density_focused {
                Span::styled(
                    "  [Space/Enter] cycle  [\u{2190}\u{2192}] change",
                    Style::default().fg(theme.text_muted),
                )
            } else {
                Span::raw("")
            },
        ])),
        content_layout[3],
    );

    let _density_value_style = if density_focused {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    // Show all density options with current highlighted
    let density_options = [
        ("Compact", Density::Compact),
        ("Normal", Density::Normal),
        ("Comfortable", Density::Comfortable),
    ];
    let density_spans: Vec<Span> = density_options
        .iter()
        .enumerate()
        .flat_map(|(i, (label, density))| {
            let is_selected = *density == state.density;
            let style = if is_selected {
                if density_focused {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.accent)
                }
            } else {
                Style::default().fg(theme.text_muted)
            };
            let prefix = if is_selected { "[" } else { " " };
            let suffix = if is_selected { "]" } else { " " };
            let sep = if i < density_options.len() - 1 {
                "  "
            } else {
                ""
            };
            vec![
                Span::styled(prefix, style),
                Span::styled(*label, style),
                Span::styled(suffix, style),
                Span::raw(sep),
            ]
        })
        .collect();

    frame.render_widget(
        Paragraph::new(Line::from(
            std::iter::once(Span::raw("    "))
                .chain(density_spans)
                .collect::<Vec<_>>(),
        )),
        content_layout[4],
    );

    // Preview section
    let preview_title = if state.emoji_mode {
        "\u{1F4CB} Preview"
    } else {
        "Preview"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!("  {preview_title}"),
                Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("Density: {density_label} | Emoji: {emoji_value}"),
                    Style::default().fg(theme.text_muted),
                ),
            ]),
        ]),
        content_layout[6],
    );

    // Footer hints
    let footer_spans = vec![
        Span::styled("[Tab/\u{2191}\u{2193}]", Style::default().fg(theme.accent)),
        Span::styled(" navigate  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Space]", Style::default().fg(theme.accent)),
        Span::styled(" toggle/cycle  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(" back", Style::default().fg(theme.text_muted)),
    ];
    frame.render_widget(Paragraph::new(Line::from(footer_spans)), layout[1]);
}

