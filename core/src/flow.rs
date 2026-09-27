//! Flows (envelopes) and their balance rules.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DomainError, Result};

/// Internal name of the per-vault system flow that can go negative.
pub const UNALLOCATED_NAME: &str = "unallocated";

/// Upper-bound rule of a flow. Stored as data, never as a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum FlowMode {
    /// No cap.
    Unlimited,
    /// `balance <= cap` at all times.
    NetCapped { cap: i64 },
    /// Cumulative positive legs `<= cap`; spending does not free room.
    IncomeCapped { cap: i64 },
}

impl FlowMode {
    /// Cap must be positive when present.
    pub fn validate(self) -> Result<()> {
        match self {
            FlowMode::Unlimited => Ok(()),
            FlowMode::NetCapped { cap } | FlowMode::IncomeCapped { cap } if cap > 0 => Ok(()),
            _ => Err(DomainError::InvalidFlow("cap must be > 0".to_string())),
        }
    }
}

/// A flow as loaded from the projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    pub id: Uuid,
    pub name: String,
    pub is_unallocated: bool,
    pub balance: i64,
    pub cap: Option<i64>,
    /// `Some` only for income-capped flows.
    pub income_total: Option<i64>,
    pub allow_negative: bool,
    pub archived: bool,
}

impl Flow {
    #[must_use]
    pub fn mode(&self) -> FlowMode {
        match (self.cap, self.income_total) {
            (None, _) => FlowMode::Unlimited,
            (Some(cap), None) => FlowMode::NetCapped { cap },
            (Some(cap), Some(_)) => FlowMode::IncomeCapped { cap },
        }
    }

    /// Integrity of the stored mode fields.
    pub fn validate_mode_fields(&self) -> Result<()> {
        let bad = |what: &str| DomainError::InvalidFlow(format!("flow '{}': {what}", self.name));
        if let Some(cap) = self.cap
            && cap <= 0
        {
            return Err(bad("cap must be > 0"));
        }
        match (self.cap, self.income_total) {
            (None, Some(_)) => Err(bad("income_total requires a cap")),
            (_, Some(total)) if total < 0 => Err(bad("income_total must be >= 0")),
            (Some(cap), Some(total)) if total > cap => Err(bad("income_total exceeds cap")),
            _ => Ok(()),
        }
    }

    /// Replace a leg of `old` with a leg of `new` (use 0 for create/remove),
    /// enforcing non-negativity and the cap.
    pub fn apply_leg_change(&mut self, old: i64, new: i64) -> Result<()> {
        let new_balance = self.balance - old + new;
        if !self.is_unallocated && !self.allow_negative && new_balance < 0 {
            return Err(DomainError::InsufficientFunds(self.name.clone()));
        }
        match self.mode() {
            FlowMode::Unlimited => {}
            FlowMode::NetCapped { cap } => {
                if new_balance > cap {
                    return Err(DomainError::MaxBalanceReached(self.name.clone()));
                }
            }
            FlowMode::IncomeCapped { cap } => {
                let total = self.income_total.unwrap_or(0) - old.max(0) + new.max(0);
                if total > cap {
                    return Err(DomainError::MaxBalanceReached(self.name.clone()));
                }
                self.income_total = Some(total);
            }
        }
        self.balance = new_balance;
        Ok(())
    }

    /// Same bookkeeping as [`Flow::apply_leg_change`] with no rule checks.
    /// Used by void: undoing is always allowed.
    pub fn apply_leg_change_unchecked(&mut self, old: i64, new: i64) {
        if let Some(total) = self.income_total {
            self.income_total = Some(total - old.max(0) + new.max(0));
        }
        self.balance = self.balance - old + new;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn flow(cap: Option<i64>, income_capped: bool, allow_negative: bool) -> Flow {
        Flow {
            id: Uuid::nil(),
            name: "Cash".to_string(),
            is_unallocated: false,
            balance: 0,
            cap,
            income_total: if income_capped { Some(0) } else { None },
            allow_negative,
            archived: false,
        }
    }

    #[test]
    fn unlimited_moves_balance() {
        let mut f = flow(None, false, false);
        f.apply_leg_change(0, 123).unwrap();
        f.apply_leg_change(123, 1000).unwrap();
        f.apply_leg_change(1000, 0).unwrap();
        assert_eq!(f.balance, 0);
    }

    #[test]
    fn net_capped_rejects_over_cap() {
        let mut f = flow(Some(1000), false, false);
        assert_eq!(
            f.apply_leg_change(0, 2044).unwrap_err(),
            DomainError::MaxBalanceReached("Cash".to_string())
        );
        f.apply_leg_change(0, 123).unwrap();
        assert!(f.apply_leg_change(123, 2000).is_err());
    }

    #[test]
    fn normal_flow_cannot_go_negative_but_unallocated_and_allow_negative_can() {
        let mut f = flow(None, false, false);
        assert_eq!(
            f.apply_leg_change(0, -1).unwrap_err(),
            DomainError::InsufficientFunds("Cash".to_string())
        );
        let mut u = flow(None, false, false);
        u.is_unallocated = true;
        u.apply_leg_change(0, -1).unwrap();
        let mut n = flow(None, false, true);
        n.apply_leg_change(0, -500).unwrap();
        assert_eq!(n.balance, -500);
    }

    #[test]
    fn income_capped_tracks_income_total() {
        let mut f = flow(Some(1000), true, false);
        f.apply_leg_change(0, 100).unwrap();
        f.apply_leg_change(0, 100).unwrap();
        assert_eq!(f.income_total, Some(200));
        f.apply_leg_change(100, -50).unwrap();
        assert_eq!(f.income_total, Some(100));
        assert!(f.apply_leg_change(0, 950).is_err());
    }

    #[test]
    fn allow_negative_still_respects_cap() {
        let mut f = flow(Some(1000), false, true);
        assert!(f.apply_leg_change(0, 2000).is_err());
    }

    #[test]
    fn unchecked_bypasses_rules() {
        let mut f = flow(Some(100), true, false);
        f.apply_leg_change_unchecked(0, -500);
        assert_eq!(f.balance, -500);
        f.apply_leg_change_unchecked(0, 5000);
        assert_eq!(f.income_total, Some(5000));
    }
}
