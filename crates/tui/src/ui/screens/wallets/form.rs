//! Wallet form rendering (create/rename).

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{AppState, EntityListMode, WalletFormField, wallets_visible_indices},
    text::{TextKey, t},
    ui::{
        common::themed_block,
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
        title: t(state.locale, TextKey::FormTitleRenameWallet),
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
    let is_rename = state.wallets.mode == EntityListMode::Rename;

    let title = if is_rename {
        t(state.locale, TextKey::FormTitleRenameWallet)
    } else {
        t(state.locale, TextKey::FormTitleNewWallet)
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

    frame.render_widget(Paragraph::new(lines).block(themed_block(title, theme.accent, theme)), area);
}
