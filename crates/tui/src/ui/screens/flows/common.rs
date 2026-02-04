//! Common utilities shared across flows screen modules.

use engine::Currency;

/// Map API currency type to engine currency type.
pub fn map_currency(currency: &api_types::Currency) -> Currency {
    match currency {
        api_types::Currency::Eur => Currency::Eur,
    }
}
