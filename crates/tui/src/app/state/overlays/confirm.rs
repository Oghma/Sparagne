//! Confirmation dialog state.

/// Modal confirmation dialog variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDialogKind {
    Delete,
    Archive,
    DiscardChanges,
}

/// Describes a modal confirmation dialog with optional follow-up actions.
#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub(crate) kind: ConfirmDialogKind,
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
    pub(crate) warning: Option<String>,
    pub(crate) preview: Vec<String>,
    pub(crate) confirm_label: String,
    pub(crate) cancel_label: String,
    pub(crate) extra_label: Option<String>,
    pub(crate) confirm_action: ConfirmAction,
    pub(crate) extra_action: Option<ConfirmAction>,
}

impl ConfirmDialogState {
    pub(crate) fn delete(
        title: impl Into<String>,
        message: impl Into<String>,
        warning: impl Into<String>,
        preview: Vec<String>,
        confirm_label: impl Into<String>,
        action: ConfirmAction,
    ) -> Self {
        Self {
            kind: ConfirmDialogKind::Delete,
            title: title.into(),
            message: message.into(),
            detail: None,
            warning: Some(warning.into()),
            preview,
            confirm_label: confirm_label.into(),
            cancel_label: "Cancel".to_string(),
            extra_label: None,
            confirm_action: action,
            extra_action: None,
        }
    }

    pub(crate) fn archive(
        title: impl Into<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
        preview: Vec<String>,
        confirm_label: impl Into<String>,
        action: ConfirmAction,
    ) -> Self {
        Self {
            kind: ConfirmDialogKind::Archive,
            title: title.into(),
            message: message.into(),
            detail: Some(detail.into()),
            warning: None,
            preview,
            confirm_label: confirm_label.into(),
            cancel_label: "Cancel".to_string(),
            extra_label: None,
            confirm_action: action,
            extra_action: None,
        }
    }

    pub(crate) fn discard_changes(
        title: impl Into<String>,
        message: impl Into<String>,
        confirm_label: impl Into<String>,
        discard_label: impl Into<String>,
        confirm_action: ConfirmAction,
        discard_action: ConfirmAction,
    ) -> Self {
        Self {
            kind: ConfirmDialogKind::DiscardChanges,
            title: title.into(),
            message: message.into(),
            detail: None,
            warning: None,
            preview: Vec::new(),
            confirm_label: confirm_label.into(),
            cancel_label: "Cancel".to_string(),
            extra_label: Some(discard_label.into()),
            confirm_action,
            extra_action: Some(discard_action),
        }
    }
}

/// Actions executed when a confirmation dialog is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    DeleteTransaction,
    DeleteVault,
    ArchiveWalletWithUndo,
    ArchiveFlowWithUndo,
    ToggleCategoryArchive,
    DiscardTransactionForm,
    DiscardTransferForm,
    SubmitTransactionForm,
    SubmitTransferForm,
}
