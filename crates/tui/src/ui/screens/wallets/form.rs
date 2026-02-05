//! Wallet form rendering (create/rename).

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{
    app::{AppState, WalletFormField, WalletsMode, wallets_visible_indices},
    ui::{
        components::input_dialog::InputDialog,
        forms::FormFieldRenderer,
        theme::Theme,
    },
};

/// Renders the rename dialog overlay.
pub fn render_rename_dialog(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return;
    };
    let indices = wallets_visible_indices(state);
    let Some(index) = indices.get(state.wallets.selected).copied() else {
        return;
    };
    let Some(wallet) = snapshot.wallets.get(index) else {
        return;
    };

    let error = state.wallets.form.name.state.validation.error_message();

    let dialog = InputDialog {
        title: "Rename Wallet",
        current_label: Some("Current:"),
        current_value: Some(wallet.name.as_str()),
        prompt: "New name:",
        value: state.wallets.form.name.value(),
        focused: state.wallets.form.focus == WalletFormField::Name,
        error,
        confirm_label: "Save",
        cancel_label: "Cancel",
    };

    crate::ui::components::input_dialog::render(frame, area, dialog, theme);
}

/// Renders the inline create form.
pub fn render_form(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let form = &state.wallets.form;
    let is_rename = state.wallets.mode == WalletsMode::Rename;

    let title = if is_rename {
        " Rename Wallet "
    } else {
        " New Wallet "
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
        Span::styled(" next field  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Esc]", Style::default().fg(theme.accent)),
        Span::styled(" cancel", Style::default().fg(theme.text_muted)),
    ]));

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(theme.accent)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
