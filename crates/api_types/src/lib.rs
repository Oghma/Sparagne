use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod cash_flow {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct CashFlowGet {
        pub name: String,
        pub vault_id: String,
    }
}

pub mod vault {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VaultNew {
        pub name: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Vault {
        pub id: Option<String>,
        pub name: Option<String>,
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
        pub amount: f64,
        pub category: String,
        pub note: String,
        pub cash_flow: String,
        pub date: Duration,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct EntryDelete {
        pub vault_id: String,
        pub entry_id: String,
        pub cash_flow: Option<String>,
        pub wallet: Option<String>,
    }
}

pub mod stats {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Statistic {
        pub balance: f64,
        pub total_income: f64,
        pub total_expenses: f64,
    }
}

