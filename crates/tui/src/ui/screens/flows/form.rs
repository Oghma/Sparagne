//! Flow create/edit form overlay.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{AppState, EntityListMode, FlowFormField, FlowModeChoice},
    text::{TextKey, t},
    ui::{common::themed_block, forms::FormFieldRenderer, theme::Theme},
};

/// Render the flow creation/edit form.
pub fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.flows.form;
    let is_rename = state.flows.mode == EntityListMode::Rename;

    let title = if is_rename {
        t(state.locale, TextKey::FormTitleRenameFlow)
    } else {
        t(state.locale, TextKey::FormTitleNewFlow)
    };

    let mut lines = vec![
        Line::from(""),
        FormFieldRenderer::render_input_field(
            &form.name.label,
            form.name.value(),
            &form.name.state,
            theme,
        ),
    ];

    if !is_rename {
        // Mode field (not a TextField, render manually)
        let mode_focused = form.focus == FlowFormField::Mode;
        let mode_label_style = if mode_focused {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        let mode_value_style = if mode_focused {
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        let mode_cursor = if mode_focused { "▏" } else { "" };
        lines.push(Line::from(vec![
            Span::styled("Type: ", mode_label_style),
            Span::styled(form.mode.label().to_string(), mode_value_style),
            Span::styled(mode_cursor.to_string(), Style::default().fg(theme.accent)),
        ]));

        // Cap field (show "-" if unlimited mode)
        let cap_value = if matches!(form.mode, FlowModeChoice::Unlimited) {
            "-"
        } else {
            form.cap.value()
        };
        lines.push(FormFieldRenderer::render_input_field(
            &form.cap.label,
            cap_value,
            &form.cap.state,
            theme,
        ));

        lines.push(FormFieldRenderer::render_input_field(
            &form.opening.label,
            form.opening.value(),
            &form.opening.state,
            theme,
        ));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(theme.accent)),
        Span::styled(
            if is_rename { " save  " } else { " create  " },
            Style::default().fg(theme.text_muted),
        ),
        Span::styled("[Tab]", Style::default().fg(theme.accent)),
        Span::styled(" next  ", Style::default().fg(theme.text_muted)),
        if !is_rename {
            Span::styled("[M]", Style::default().fg(theme.accent))
        } else {
            Span::raw("")
        },
        if !is_rename {
            Span::styled(" toggle type  ", Style::default().fg(theme.text_muted))
        } else {
            Span::raw("")
        },
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(" cancel", Style::default().fg(theme.text_muted)),
    ]));

    frame.render_widget(
        Paragraph::new(lines).block(themed_block(title, theme.accent, theme)),
        area,
    );
}
