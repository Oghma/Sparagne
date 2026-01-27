use std::time::Instant;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct HelpState {
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct ToastState {
    pub message: String,
    pub level: ToastLevel,
    pub created_at: Instant,
    pub expires_at: Instant,
    pub undo_action: Option<UndoAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Error,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoAction {
    TransactionVoid { ids: Vec<Uuid> },
    WalletArchive { id: Uuid },
    FlowArchive { id: Uuid },
}

#[derive(Debug, Default)]
pub struct ConnectionState {
    pub ok: bool,
    pub message: Option<String>,
}

#[derive(Debug, Default)]
pub struct CommandPaletteState {
    pub active: bool,
    pub query: String,
    pub selected: usize,
}

/// Overlay state for modal dialogs and transient UI elements.
#[derive(Debug, Default)]
pub struct OverlayState {
    pub(crate) confirm: Option<ConfirmDialogState>,
    pub(crate) error: Option<ErrorDialogState>,
    pub(crate) bulk_category: Option<BulkCategoryDialogState>,
}

impl OverlayState {
    /// Returns true if a confirmation dialog is currently displayed.
    pub fn has_confirm_dialog(&self) -> bool {
        self.confirm.is_some()
    }

    /// Returns true if an error dialog is currently displayed.
    pub fn has_error_dialog(&self) -> bool {
        self.error.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct BulkCategoryDialogState {
    pub(crate) transaction_ids: Vec<Uuid>,
    pub(crate) count: usize,
    pub(crate) input: String,
    pub(crate) error: Option<String>,
}

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
    ToggleWalletArchive,
    ToggleFlowArchive,
    ToggleCategoryArchive,
    DiscardTransactionForm,
    DiscardTransferForm,
    SubmitTransactionForm,
    SubmitTransferForm,
}

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

/// Global spinner state used by loading overlays.
#[derive(Debug, Default)]
pub struct SpinnerState {
    index: usize,
}

impl SpinnerState {
    pub(crate) fn tick(&mut self) {
        self.index = (self.index + 1) % SPINNER_FRAME_COUNT;
    }

    pub(crate) fn index(&self) -> usize {
        self.index
    }
}

const SPINNER_FRAME_COUNT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    NewExpense,
    NewIncome,
    NewRefund,
    NewTransferWallet,
    NewTransferFlow,
    Categories,
    CategoryAliases,
    Members,
    WalletNew,
    FlowNew,
    VaultCreate,
    Refresh,
    ToggleVoided,
}

impl PaletteCommand {
    pub fn all() -> Vec<Self> {
        vec![
            Self::NewExpense,
            Self::NewIncome,
            Self::NewRefund,
            Self::NewTransferWallet,
            Self::NewTransferFlow,
            Self::Categories,
            Self::CategoryAliases,
            Self::Members,
            Self::WalletNew,
            Self::FlowNew,
            Self::VaultCreate,
            Self::Refresh,
            Self::ToggleVoided,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NewExpense => "Transactions: New Expense",
            Self::NewIncome => "Transactions: New Income",
            Self::NewRefund => "Transactions: New Refund",
            Self::NewTransferWallet => "Transactions: New Transfer Wallet",
            Self::NewTransferFlow => "Transactions: New Transfer Flow",
            Self::Categories => "Categories: Open",
            Self::CategoryAliases => "Categories: Aliases",
            Self::Members => "Members: Open",
            Self::WalletNew => "Wallets: New",
            Self::FlowNew => "Flows: New",
            Self::VaultCreate => "Vault: Create",
            Self::Refresh => "Refresh",
            Self::ToggleVoided => "Transactions: Toggle voided",
        }
    }
}
