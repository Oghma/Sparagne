use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, DefaultsField, VaultMode},
    ui::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    match state.vault_ui.mode {
        VaultMode::View => render_view(frame, layout[0], state, &theme),
        VaultMode::Create => render_create(frame, layout[0], state, &theme),
        VaultMode::Defaults => render_defaults(frame, layout[0], state, &theme),
        VaultMode::Select => render_list(frame, layout[0], state, &theme),
    }

    render_footer(frame, layout[1], state, &theme);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let block = Block::default()
        .title(Span::styled(
            " 🏦 Vaults ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused));
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

fn render_view(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let vault_name = display_vault_name(state).unwrap_or_else(|| "Main".to_string());
    let vault_id = state
        .vault
        .as_ref()
        .and_then(|v| v.id.as_deref())
        .unwrap_or("-");
    let currency = state
        .vault
        .as_ref()
        .and_then(|v| v.currency.as_ref())
        .map(|c| format!("{c:?}"))
        .unwrap_or_else(|| "EUR".to_string());
    let (wallets_count, flows_count) = state
        .snapshot
        .as_ref()
        .map(|snap| (snap.wallets.len(), snap.flows.len()))
        .unwrap_or((0, 0));

    let default_wallet_name = state
        .default_wallet_id
        .map(|id| resolve_wallet_name(state, id))
        .unwrap_or_else(|| "None".to_string());
    let default_flow_name = state
        .default_flow_id
        .map(|id| resolve_flow_name(state, id))
        .unwrap_or_else(|| "None".to_string());

    let block = Block::default()
        .title(Span::styled(
            format!(" 🏦 {} ", vault_name),
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let info_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // ID
            Constraint::Length(1), // Currency
            Constraint::Length(1), // Wallets/Flows
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Defaults header
            Constraint::Length(1), // Default wallet
            Constraint::Length(1), // Default flow
            Constraint::Min(0),    // Error/confirmation
        ])
        .split(inner);

    // Vault ID
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ID          ", Style::default().fg(theme.text_muted)),
            Span::styled(vault_id.to_string(), Style::default().fg(theme.text)),
        ])),
        info_layout[0],
    );

    // Currency
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Currency    ", Style::default().fg(theme.text_muted)),
            Span::styled(currency, Style::default().fg(theme.text)),
        ])),
        info_layout[1],
    );

    // Wallets and Flows count
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Wallets     ", Style::default().fg(theme.text_muted)),
            Span::styled(wallets_count.to_string(), Style::default().fg(theme.text)),
            Span::raw("    "),
            Span::styled("Flows  ", Style::default().fg(theme.text_muted)),
            Span::styled(flows_count.to_string(), Style::default().fg(theme.text)),
        ])),
        info_layout[2],
    );

    // Defaults header
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Quick Defaults",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )])),
        info_layout[4],
    );

    // Default wallet
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Default Wallet  ", Style::default().fg(theme.text_muted)),
            Span::styled(
                default_wallet_name,
                if state.default_wallet_id.is_some() {
                    Style::default().fg(theme.text)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
        ])),
        info_layout[5],
    );

    // Default flow
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Default Flow    ", Style::default().fg(theme.text_muted)),
            Span::styled(
                default_flow_name,
                if state.default_flow_id.is_some() {
                    Style::default().fg(theme.text)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
        ])),
        info_layout[6],
    );

    // Error or confirmation
    if let Some(err) = state.vault_ui.error.as_ref() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ✗ ", Style::default().fg(theme.negative)),
                Span::styled(err.clone(), Style::default().fg(theme.negative)),
            ])),
            info_layout[7],
        );
    }
}

fn display_vault_name(state: &AppState) -> Option<String> {
    let vault = state.vault.as_ref()?;
    let name = vault.name.as_deref()?;
    let owner = vault.owner.as_deref();
    let username = state.login.username.trim();

    match owner {
        Some(owner) if !owner.is_empty() && owner != username => Some(format!("{name} ({owner})")),
        _ => Some(name.to_string()),
    }
}

fn render_create(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.vault_ui.form;

    let block = Block::default()
        .title(Span::styled(
            " Create Vault ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Name field
            Constraint::Length(1), // Currency field
            Constraint::Min(0),    // Error
        ])
        .split(inner);

    // Name field
    let name_value = if form.name.is_empty() {
        "_"
    } else {
        form.name.as_str()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  Name      ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(name_value.to_string(), Style::default().fg(theme.text)),
            Span::styled("_", Style::default().fg(theme.accent)),
        ])),
        layout[0],
    );

    // Currency field (fixed for now)
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Currency  ", Style::default().fg(theme.text_muted)),
            Span::styled("EUR", Style::default().fg(theme.text)),
        ])),
        layout[1],
    );

    // Error
    if let Some(err) = form.error.as_ref() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ✗ ", Style::default().fg(theme.negative)),
                Span::styled(err.clone(), Style::default().fg(theme.negative)),
            ])),
            layout[2],
        );
    }
}

fn render_defaults(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        let block = Block::default()
            .title(Span::styled(
                " Quick Defaults ",
                Style::default().fg(theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_focused));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Snapshot not available",
                Style::default().fg(theme.text_muted),
            )))
            .alignment(Alignment::Center)
            .block(block),
            area,
        );
        return;
    };

    let wallet_names: Vec<String> = snapshot
        .wallets
        .iter()
        .filter(|wallet| !wallet.archived)
        .map(|wallet| wallet.name.clone())
        .collect();
    let flow_names: Vec<String> = snapshot
        .flows
        .iter()
        .filter(|flow| !flow.archived)
        .map(|flow| flow.name.clone())
        .collect();

    let defaults = &state.vault_ui.defaults;
    let wallet_label = if defaults.wallet_index == 0 {
        "None"
    } else {
        wallet_names
            .get(defaults.wallet_index - 1)
            .map(|name| name.as_str())
            .unwrap_or("None")
    };
    let flow_label = if defaults.flow_index == 0 {
        "None"
    } else {
        flow_names
            .get(defaults.flow_index - 1)
            .map(|name| name.as_str())
            .unwrap_or("None")
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    // Form section
    let form_block = Block::default()
        .title(Span::styled(
            " Quick Defaults ",
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused));
    let form_inner = form_block.inner(layout[0]);
    frame.render_widget(form_block, layout[0]);

    let form_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Wallet
            Constraint::Length(1), // Flow
        ])
        .split(form_inner);

    // Wallet field
    let wallet_focused = defaults.focus == DefaultsField::Wallet;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  Default Wallet  ",
                if wallet_focused {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
            Span::styled(wallet_label.to_string(), Style::default().fg(theme.text)),
            if wallet_focused {
                Span::styled("  ↑↓", Style::default().fg(theme.text_muted))
            } else {
                Span::raw("")
            },
        ])),
        form_layout[0],
    );

    // Flow field
    let flow_focused = defaults.focus == DefaultsField::Flow;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  Default Flow    ",
                if flow_focused {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
            Span::styled(flow_label.to_string(), Style::default().fg(theme.text)),
            if flow_focused {
                Span::styled("  ↑↓", Style::default().fg(theme.text_muted))
            } else {
                Span::raw("")
            },
        ])),
        form_layout[1],
    );

    // Lists section
    let lists = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[1]);

    render_defaults_list(
        frame,
        lists[0],
        "Wallets",
        &wallet_names,
        defaults.wallet_index,
        wallet_focused,
        theme,
    );
    render_defaults_list(
        frame,
        lists[1],
        "Flows",
        &flow_names,
        defaults.flow_index,
        flow_focused,
        theme,
    );

    // Error
    if let Some(err) = defaults.error.as_ref() {
        let error_area = Rect {
            y: area.y + area.height.saturating_sub(1),
            height: 1,
            ..area
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("✗ ", Style::default().fg(theme.negative)),
                Span::styled(err.clone(), Style::default().fg(theme.negative)),
            ])),
            error_area,
        );
    }
}

fn render_defaults_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
    focused: bool,
    theme: &Theme,
) {
    let mut list_items = Vec::with_capacity(items.len() + 1);
    list_items.push(ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled("None", Style::default().fg(theme.text_muted)),
    ])));
    list_items.extend(items.iter().map(|name| {
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(name.clone(), Style::default().fg(theme.text)),
        ]))
    }));

    let mut list_state = ListState::default();
    if !list_items.is_empty() {
        list_state.select(Some(selected.min(list_items.len() - 1)));
    }

    let highlight_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let border_color = if focused {
        theme.border_focused
    } else {
        theme.border
    };

    let block = Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(theme.accent),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    let list = List::new(list_items)
        .block(block)
        .highlight_style(highlight_style)
        .highlight_symbol("» ");
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let hints = match state.vault_ui.mode {
        VaultMode::View => vec![
            ("[c]", "create"),
            ("[d]", "defaults"),
            ("[l]", "list"),
            ("[x]", "delete"),
        ],
        VaultMode::Create => vec![("[Enter]", "create"), ("[Esc]", "cancel")],
        VaultMode::Defaults => vec![
            ("[Tab]", "next"),
            ("[↑↓]", "change"),
            ("[Enter]", "save"),
            ("[Esc]", "cancel"),
        ],
        VaultMode::Select => vec![
            ("[Enter]", "select"),
            ("[↑↓]", "navigate"),
            ("[Esc]", "back"),
        ],
    };

    let mut spans = Vec::new();
    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(*key, Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            format!(" {action}"),
            Style::default().fg(theme.text_muted),
        ));
    }

    // Add list error if present
    if state.vault_ui.mode == VaultMode::Select {
        if let Some(err) = state.vault_ui.list.error.as_ref() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                err.clone(),
                Style::default().fg(theme.negative),
            ));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn resolve_wallet_name(state: &AppState, wallet_id: uuid::Uuid) -> String {
    state
        .snapshot
        .as_ref()
        .and_then(|snap| {
            snap.wallets
                .iter()
                .find(|wallet| wallet.id == wallet_id)
                .map(|wallet| wallet.name.clone())
        })
        .unwrap_or_else(|| wallet_id.to_string())
}

fn resolve_flow_name(state: &AppState, flow_id: uuid::Uuid) -> String {
    state
        .snapshot
        .as_ref()
        .and_then(|snap| {
            snap.flows
                .iter()
                .find(|flow| flow.id == flow_id)
                .map(|flow| flow.name.clone())
        })
        .unwrap_or_else(|| flow_id.to_string())
}
