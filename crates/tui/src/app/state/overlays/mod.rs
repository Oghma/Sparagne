//! Overlay state types for modal dialogs and transient UI elements.

mod bulk_category;
mod confirm;
mod error;
mod grouping;

pub use bulk_category::BulkCategoryDialogState;
pub use confirm::{ConfirmAction, ConfirmDialogKind, ConfirmDialogState};
pub use error::{ErrorAction, ErrorDialogKind, ErrorDialogState};
pub use grouping::GroupingDialogState;

/// Overlay state for modal dialogs and transient UI elements.
#[derive(Debug, Default)]
pub struct OverlayState {
    pub(crate) confirm: Option<ConfirmDialogState>,
    pub(crate) error: Option<ErrorDialogState>,
    pub(crate) bulk_category: Option<BulkCategoryDialogState>,
    pub(crate) grouping: Option<GroupingDialogState>,
}

impl OverlayState {
    /// Returns `true` if a confirmation dialog is currently active.
    #[must_use]
    pub fn has_confirm_dialog(&self) -> bool {
        self.confirm.is_some()
    }
}
