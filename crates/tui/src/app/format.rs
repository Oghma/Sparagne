//! Display formatting utilities.
//!
//! This module provides functions for converting domain types to user-facing
//! strings, including currency symbols, amounts, dates, and enum labels.

use api_types::{membership::MembershipRole, transaction::TransactionKind};
use engine::Currency;

/// Returns a short label for a month number (1-12).
///
/// Returns "???" for invalid month numbers.
pub(crate) fn month_label(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

/// Formats an amount in minor units as a decimal string suitable for input
/// fields.
///
/// Examples (with EUR, 2 minor units):
/// - 1250 → "12.50"
/// - -500 → "-5.00"
/// - 0 → "0.00"
pub(crate) fn format_amount_input(amount_minor: i64, currency: Currency) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let abs = amount_minor.unsigned_abs();
    let scale = 10u64.pow(currency.minor_units() as u32);
    if scale == 1 {
        return format!("{sign}{abs}");
    }
    let major = abs / scale;
    let minor = abs % scale;
    format!(
        "{sign}{major}.{minor:0width$}",
        width = currency.minor_units() as usize
    )
}

/// Calculates year/month with a signed month offset.
///
/// Handles year wraparound correctly (e.g. January - 1 = December of the
/// previous year).
pub(crate) fn offset_month(year: i32, month: u32, offset: i32) -> (i32, u32) {
    let total_months = year * 12 + (month as i32 - 1) + offset;
    let new_year = total_months / 12;
    let new_month = (total_months % 12 + 12) % 12 + 1;
    (new_year, new_month as u32)
}

/// Returns a lowercase label for a transaction kind.
pub(crate) fn transaction_kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Refund => "refund",
        TransactionKind::TransferWallet => "transfer wallet",
        TransactionKind::TransferFlow => "transfer flow",
    }
}

/// Returns a rank for a membership role, used for sorting.
///
/// Owner = 0, Editor = 1, Viewer = 2.
pub(crate) fn member_role_rank(role: MembershipRole) -> u8 {
    match role {
        MembershipRole::Owner => 0,
        MembershipRole::Editor => 1,
        MembershipRole::Viewer => 2,
    }
}
