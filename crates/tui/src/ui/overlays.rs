use ratatui::{Frame, layout::Rect};

use crate::{
    app::{AppState, Screen},
    ui::{components, Theme},
};

pub(crate) fn render_overlays(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let theme = Theme::default();

    components::help_overlay::render(frame, area, state);
    components::command_palette::render(frame, area, state);
    components::global_search::render(frame, area, state);
    components::confirm_dialog::render(frame, area, state.overlays.confirm.as_ref());
    components::error_dialog::render(frame, area, state.overlays.error.as_ref());
    components::bulk_category_dialog::render(frame, area, state.overlays.bulk_category.as_ref());
    components::grouping_dialog::render(
        frame,
        area,
        state.overlays.grouping.as_ref(),
        state.transactions.grouping_mode,
    );
    components::toast::render(frame, area, state.toast.as_ref());

    if state.screen == Screen::Home
        && state.snapshot.is_none()
        && state.overlays.error.is_none()
    {
        components::loading::render_fullscreen(
            frame,
            area,
            components::loading::spinner_frame(state.spinner.index()),
            "Loading...",
            Some("Fetching vault data"),
            &theme,
        );
    }
}
