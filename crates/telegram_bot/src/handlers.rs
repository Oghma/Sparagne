//! Handlers for managing commands

use teloxide::{dispatching::dialogue::InMemStorage, prelude::Dialogue};
use uuid::Uuid;

pub mod entry;
pub mod exports;
pub mod start;
pub mod statistics;
pub mod user;

#[derive(Debug, Default, Clone)]
pub enum GlobalState {
    #[default]
    Idle,
    InDelete(Vec<Uuid>),
}

type GlobalDialogue = Dialogue<GlobalState, InMemStorage<GlobalState>>;
