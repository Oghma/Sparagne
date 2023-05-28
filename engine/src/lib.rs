pub use cash_flows::CashFlow;
pub use error::EngineError;
pub use vault::Vault;

mod cash_flows;
mod entry;
mod error;
mod vault;
mod wallets;

type ResultEngine<T> = Result<T, EngineError>;
