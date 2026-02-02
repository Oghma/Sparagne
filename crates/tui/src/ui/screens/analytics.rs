// Analytics screen - wraps the stats screen with the new name
use ratatui::{Frame, layout::Rect};

use crate::app::AppState;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    // Reuse the existing stats rendering logic
    super::stats::render(frame, area, state);
}
