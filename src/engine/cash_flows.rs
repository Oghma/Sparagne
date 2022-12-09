//! A collection of types that implement `CashFlow` trait.
//!
//! The available cash flows are:
//! - [`UnBounded`]
//! - [`Bounded`]
//! - [`HardBounded`]
use super::entry::Entry;
use super::errors::EngineError;
pub use bounded::Bounded;
pub use hard_bounded::HardBounded;
pub use unbounded::UnBounded;

mod bounded;
mod hard_bounded;
mod unbounded;

/// `CashFlow` trait. Base requirements for a `CashFlow`.
pub trait CashFlow {
    fn add_entry(
        &mut self,
        balance: f64,
        category: String,
        note: String,
    ) -> Result<uuid::Uuid, EngineError> {
        let entry = Entry::new(balance, category, note);
        self.insert(entry)
    }

    fn archive(&mut self);

    fn delete_entry(&mut self, id: &uuid::Uuid) -> Result<(), EngineError>;

    fn update_entry(
        &mut self,
        id: &uuid::Uuid,
        balance: f64,
        category: String,
        note: String,
    ) -> Result<(), EngineError>;

    fn insert(&mut self, entry: Entry) -> Result<uuid::Uuid, EngineError>;
}
