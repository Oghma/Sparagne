//! Toast notification state.

use std::time::Instant;
use uuid::Uuid;

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
