//! A collection of types that implement `CashFlow` trait.
//!
//! The available cash flows are:
//! - [`UnBounded`]
//! - [`Bounded`]
//! - [`HardBounded`]
use crate::entry::Entry;

/// `CashFlow` trait. Base requirements for a `CashFlow`.
trait CashFlow {
    fn add_entry(&mut self, balance: f64, category: String, note: String) -> Result<(), &str> {
        let entry = Entry::new(balance, category, note);
        self.insert(entry)
    }

    fn insert(&mut self, entry: Entry) -> Result<(), &str>;
}
