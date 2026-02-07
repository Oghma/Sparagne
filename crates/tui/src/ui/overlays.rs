use ratatui::{Frame, layout::Rect};

use crate::{
    app::{AppState, Screen},
    text::{TextKey, t},
    ui::{Theme, components},
};

pub(crate) fn render_overlays(frame: &mut Frame<'_>, area: Rect, state: &AppState, theme: &Theme) {
    components::help_overlay::render(frame, area, state, theme);
    components::command_palette::render(frame, area, state, theme);
    components::global_search::render(frame, area, state, theme);
    components::confirm_dialog::render(frame, area, state.overlays.confirm.as_ref(), theme);
    components::error_dialog::render(
        frame,
        area,
        state.overlays.error.as_ref(),
        theme,
        state.locale,
    );
    components::bulk_category_dialog::render(
        frame,
        area,
        state.overlays.bulk_category.as_ref(),
        theme,
    );
    components::grouping_dialog::render(
        frame,
        area,
        state.overlays.grouping.as_ref(),
        state.transactions.grouping_mode,
        theme,
        state.locale,
    );
    components::toast::render(frame, area, state.toast.as_ref(), theme);

    if state.screen == Screen::Home && state.snapshot.is_none() && state.overlays.error.is_none() {
        components::loading::render_fullscreen(
            frame,
            area,
            components::loading::spinner_frame(state.spinner.index()),
            t(state.locale, TextKey::LoadingGeneric),
            Some(t(state.locale, TextKey::LoadingVaultData)),
            theme,
        );
    }
}
