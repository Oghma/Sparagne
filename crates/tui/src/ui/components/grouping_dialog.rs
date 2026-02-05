use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{GroupingDialogState, GroupingMode},
    ui::{components::centered_rect, theme::Theme},
};

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    dialog: Option<&GroupingDialogState>,
    current: GroupingMode,
    theme: &Theme,
) {
    let Some(dialog) = dialog else {
        return;
    };
    let popup = centered_rect(46, 38, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(Span::styled(
            " Group Transactions ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("d", Style::default().fg(theme.accent)),
        Span::raw(" date  "),
        Span::styled("c", Style::default().fg(theme.accent)),
        Span::raw(" category  "),
        Span::styled("w", Style::default().fg(theme.accent)),
        Span::raw(" wallet  "),
        Span::styled("e", Style::default().fg(theme.accent)),
        Span::raw(" envelope"),
    ]));
    frame.render_widget(hint, layout[0]);

    let items: Vec<ListItem> = GroupingMode::ALL
        .iter()
        .map(|mode| {
            let (key, label) = mode_meta(*mode);
            let mut spans = vec![
                Span::styled(format!("[{key}]"), Style::default().fg(theme.accent)),
                Span::raw(format!(" {label}")),
            ];
            if *mode == current {
                spans.push(Span::raw("  "));
                spans.push(Span::styled("current", Style::default().fg(theme.dim)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(dialog.selected.min(items.len().saturating_sub(1))));

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");

    frame.render_stateful_widget(list, layout[1], &mut state);
}

fn mode_meta(mode: GroupingMode) -> (char, &'static str) {
    match mode {
        GroupingMode::Date => ('d', "Date"),
        GroupingMode::Category => ('c', "Category"),
        GroupingMode::Wallet => ('w', "Wallet"),
        GroupingMode::Envelope => ('e', "Envelope"),
    }
}
