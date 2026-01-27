use ratatui::{Frame, layout::Rect};

use crate::{
    app::BulkCategoryDialogState,
    ui::{Theme, components::input_dialog},
};

pub fn render(frame: &mut Frame<'_>, area: Rect, dialog: Option<&BulkCategoryDialogState>) {
    let Some(dialog) = dialog else {
        return;
    };

    let theme = Theme::default();
    let current_value = format!("{} selected", dialog.count);
    input_dialog::render(
        frame,
        area,
        input_dialog::InputDialog {
            title: "Bulk Categorize",
            current_label: Some("Selected:"),
            current_value: Some(current_value.as_str()),
            prompt: "Category (#):",
            value: dialog.input.as_str(),
            focused: true,
            error: dialog.error.as_deref(),
            confirm_label: "Apply",
            cancel_label: "Cancel",
        },
        &theme,
    );
}
