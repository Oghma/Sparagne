//! Flow dialogs (rename dialog and other modals).

use ratatui::{Frame, layout::Rect};

use crate::{
    app::{AppState, FlowFormField, flows_visible_indices},
    ui::{components::input_dialog::InputDialog, theme::Theme},
};

/// Render the rename flow dialog.
pub fn render_rename_dialog(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    let Some(snapshot) = state.snapshot.as_ref() else {
        return;
    };
    let indices = flows_visible_indices(state);
    let Some(index) = indices.get(state.flows.selected).copied() else {
        return;
    };
    let Some(flow) = snapshot.flows.get(index) else {
        return;
    };

    let error = state.flows.form.name.state.validation.error_message();

    let dialog = InputDialog {
        title: "Rename Flow",
        current_label: Some("Current:"),
        current_value: Some(flow.name.as_str()),
        prompt: "New name:",
        value: state.flows.form.name.value(),
        focused: state.flows.form.focus == FlowFormField::Name,
        error,
        confirm_label: "Save",
        cancel_label: "Cancel",
    };

    crate::ui::components::input_dialog::render(frame, area, dialog, theme);
}
