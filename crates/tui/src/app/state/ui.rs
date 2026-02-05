//! UI-related state types (help, spinner, connection).

/// Help overlay state.
#[derive(Debug, Default)]
pub struct HelpState {
    pub active: bool,
}

/// Global spinner state used by loading overlays.
#[derive(Debug, Default)]
pub struct SpinnerState {
    index: usize,
}

impl SpinnerState {
    const FRAME_COUNT: usize = 10;

    pub(crate) fn tick(&mut self) {
        self.index = (self.index + 1) % Self::FRAME_COUNT;
    }

    pub(crate) fn index(&self) -> usize {
        self.index
    }
}

/// Connection state for API health tracking.
#[derive(Debug, Default)]
pub struct ConnectionState {
    pub ok: bool,
    pub message: Option<String>,
}
