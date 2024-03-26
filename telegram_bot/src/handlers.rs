//! Handlers for managing commands

use teloxide::{dispatching::dialogue::InMemStorage, prelude::Dialogue};

pub mod entry;
pub mod exports;
pub mod statistics;
pub mod user;

#[derive(Debug, Default, Clone)]
pub enum GlobalState {
    #[default]
    Idle,
    InDelete(Vec<(String, String)>),
}

type GlobalDialogue = Dialogue<GlobalState, InMemStorage<GlobalState>>;
