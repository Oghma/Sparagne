//! Transaction submission actions - split from the monolithic transactions_submit.rs
//!
//! This module handles all transaction submission-related actions:
//! - `form`: Transaction form submission and editing
//! - `transfers`: Wallet and flow transfer submissions
//! - `filter`: Transaction list filtering
//! - `quick_add`: Quick-add parsing and submission
//! - `void`: Transaction voiding and bulk operations

mod filter;
mod form;
mod quick_add;
mod transfers;
mod void;
