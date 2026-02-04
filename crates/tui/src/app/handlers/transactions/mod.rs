//! Transaction handler module.
//!
//! This module contains all transaction-related handlers split into focused submodules:
//! - `categories`: Category selection and cycling
//! - `form_input`: Form input handling and focus management
//! - `list`: List operations (selection, filtering)
//! - `pickers`: Wallet/flow/transfer picker logic
//! - `visual_mode`: Visual mode operations and bulk actions

mod categories;
mod form_input;
mod list;
mod pickers;
mod visual_mode;
