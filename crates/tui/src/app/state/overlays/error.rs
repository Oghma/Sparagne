//! Error dialog state.

/// Modal error dialog variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorDialogKind {
    Error,
    Connection,
}

/// Describes a modal error dialog.
#[derive(Debug, Clone)]
pub struct ErrorDialogState {
    pub(crate) kind: ErrorDialogKind,
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
    pub(crate) confirm_label: String,
    pub(crate) cancel_label: Option<String>,
    pub(crate) retry_action: Option<ErrorAction>,
}

impl ErrorDialogState {
    pub(crate) fn error(
        title: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            kind: ErrorDialogKind::Error,
            title: title.into(),
            message: message.into(),
            detail,
            confirm_label: "OK".to_string(),
            cancel_label: None,
            retry_action: None,
        }
    }

    pub(crate) fn connection(
        title: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
        retry_action: ErrorAction,
    ) -> Self {
        Self {
            kind: ErrorDialogKind::Connection,
            title: title.into(),
            message: message.into(),
            detail,
            confirm_label: "Retry".to_string(),
            cancel_label: Some("Cancel".to_string()),
            retry_action: Some(retry_action),
        }
    }
}

/// Actions available from an error dialog (e.g., retry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorAction {
    RetrySnapshot,
}
