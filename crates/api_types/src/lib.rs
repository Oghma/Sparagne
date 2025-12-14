use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    #[default]
    Eur,
}

pub mod cash_flow {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct CashFlowGet {
        pub vault_id: String,
        /// Cash flow id (UUID).
        ///
        /// This is serialized as a string in JSON.
        pub id: Option<Uuid>,
        /// Cash flow name (legacy convenience).
        pub name: Option<String>,
    }
}

pub mod vault {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VaultNew {
        pub name: String,
        pub currency: Option<Currency>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Vault {
        pub id: Option<String>,
        pub name: Option<String>,
        pub currency: Option<Currency>,
    }
}

pub mod user {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct PairUser {
        pub code: String,
        pub telegram_id: String,
    }
}

pub mod entry {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct EntryNew {
        pub vault_id: String,
        pub amount_minor: i64,
        pub category: String,
        pub note: String,
        /// Cash flow id (UUID). Optional for wallet-only entries.
        pub cash_flow_id: Option<Uuid>,
        /// Wallet id (UUID). Optional for flow-only entries.
        pub wallet_id: Option<Uuid>,
        /// RFC3339 timestamp, including timezone offset (local user time).
        ///
        /// Examples:
        /// - `2025-12-14T12:34:56Z`
        /// - `2025-12-14T13:34:56+01:00`
        pub date: DateTime<FixedOffset>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct EntryDelete {
        pub vault_id: String,
        pub entry_id: String,
        pub cash_flow_id: Option<Uuid>,
        pub wallet_id: Option<Uuid>,
    }
}

pub mod stats {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Statistic {
        pub currency: Currency,
        pub balance_minor: i64,
        pub total_income_minor: i64,
        pub total_expenses_minor: i64,
    }
}
