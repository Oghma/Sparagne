use super::super::*;

use crate::{
    app::{
        errors::login_message_for_error,
        format::{map_currency, month_label},
    },
    error::Result,
    text::{TextKey, t},
};
use api_types::{
    transaction::{TransactionList, TransactionView},
    vault::Vault,
};
use chrono::{DateTime, Datelike, FixedOffset, Offset, TimeZone, Utc};
use chrono_tz::Tz;

/// Calculate percentage change between the last two values in a time series.
///
/// Returns `None` when there are fewer than two data points or when the
/// previous value is zero (to avoid division by zero).
pub(crate) fn percentage_change(series: &[(String, i64)]) -> Option<f64> {
    if series.len() < 2 {
        return None;
    }
    let (_, prev) = series[series.len() - 2];
    let (_, current) = series[series.len() - 1];
    if prev == 0 {
        return None;
    }
    Some(((current - prev) as f64 / prev.abs() as f64) * 100.0)
}

/// Calculate net (income minus expense) percentage change from two trend series.
///
/// Returns `None` when either series has fewer than two data points or when
/// the previous net value is zero.
pub(crate) fn calculate_net_change(income_trend: &[(String, i64)], expense_trend: &[(String, i64)]) -> Option<f64> {
    if income_trend.len() < 2 || expense_trend.len() < 2 {
        return None;
    }
    let prev_income = income_trend[income_trend.len() - 2].1;
    let curr_income = income_trend[income_trend.len() - 1].1;
    let prev_expense = expense_trend[expense_trend.len() - 2].1;
    let curr_expense = expense_trend[expense_trend.len() - 1].1;

    let prev_net = prev_income - prev_expense;
    let curr_net = curr_income - curr_expense;

    if prev_net == 0 {
        return None;
    }
    Some(((curr_net - prev_net) as f64 / prev_net.abs() as f64) * 100.0)
}

impl App {
    pub(crate) fn parse_local_datetime(
        &self,
        input: &str,
    ) -> std::result::Result<DateTime<FixedOffset>, String> {
        let naive = chrono::NaiveDateTime::parse_from_str(input.trim(), "%Y-%m-%d %H:%M")
            .map_err(|_| t(self.state.locale, TextKey::ValidationDateInvalid).to_string())?;
        let localized = self.tz.from_local_datetime(&naive);
        let dt: DateTime<Tz> = match localized {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(dt, _) => dt,
            chrono::LocalResult::None => {
                return Err(t(self.state.locale, TextKey::ValidationDateInvalidTimezone).to_string());
            }
        };
        let offset = dt.offset().fix();
        Ok(dt.with_timezone(&offset))
    }

    pub(crate) fn now_in_timezone(&self) -> DateTime<FixedOffset> {
        let now = Utc::now();
        let local = self.tz.from_utc_datetime(&now.naive_utc());
        let offset = local.offset().fix();
        local.with_timezone(&offset)
    }
    pub(crate) async fn load_stats(&mut self) -> Result<()> {
        let payload = Vault {
            id: self.state.vault.as_ref().and_then(|v| v.id.clone()),
            name: self.state.vault.as_ref().and_then(|v| v.name.clone()),
            currency: None,
            owner: None,
        };

        let res = self
            .client
            .stats_get(payload)
            .await;

        match res {
            Ok(stat) => {
                self.state.stats.data = Some(stat);
                self.state.stats.error = None;
                self.connection_ok(None);
                self.load_stats_series().await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.stats.error = Some(login_message_for_error(err, self.state.locale));
                self.connection_error(t(self.state.locale, TextKey::ErrorConnection));
            }
        }

        Ok(())
    }
    pub(crate) async fn load_stats_series(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let to = self.now_in_timezone();
        let from = to - chrono::Duration::days(180);

        let mut cursor = None;
        let mut transactions = Vec::new();
        loop {
            let payload = TransactionList {
                vault_id: vault_id.clone(),
                flow_id: None,
                wallet_id: None,
                limit: Some(200),
                cursor,
                from: Some(from),
                to: Some(to),
                kinds: None,
                include_voided: Some(false),
                include_transfers: Some(false),
            };

            let res = self
                .client
                .transactions_list(payload)
                .await;

            match res {
                Ok(list) => {
                    transactions.extend(list.transactions);
                    if let Some(next) = list.next_cursor {
                        cursor = Some(next);
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    if self.handle_auth_error(&err) {
                        return Ok(());
                    }
                    self.state.stats.error = Some(login_message_for_error(err, self.state.locale));
                    return Ok(());
                }
            }
        }

        self.compute_stats_series(&transactions, to);
        Ok(())
    }
    pub(crate) fn compute_stats_series(
        &mut self,
        transactions: &[TransactionView],
        to: DateTime<FixedOffset>,
    ) {
        use api_types::transaction::TransactionKind;
        use std::collections::HashMap;

        let start_day = (to - chrono::Duration::days(29))
            .with_timezone(&self.tz)
            .date_naive();
        let end_day = to.with_timezone(&self.tz).date_naive();
        let days_count = (end_day - start_day).num_days().max(0) as usize + 1;
        let mut daily_net = vec![0i64; days_count];

        let mut category_breakdown: HashMap<String, i64> = HashMap::new();
        let mut monthly_income: HashMap<(i32, u32), i64> = HashMap::new();
        let mut monthly_expense: HashMap<(i32, u32), (i64, i64)> = HashMap::new(); // (expense, refund)

        let (current_year, current_month) = self.state.stats.current_month;

        for tx in transactions {
            if tx.voided {
                continue;
            }

            let local = tx.occurred_at.with_timezone(&self.tz);
            let date = local.date_naive();
            let year = date.year();
            let month = date.month();

            match tx.kind {
                TransactionKind::Income => {
                    if date >= start_day && date <= end_day {
                        let idx = (date - start_day).num_days() as usize;
                        daily_net[idx] += tx.amount_minor.abs();
                    }
                    *monthly_income.entry((year, month)).or_insert(0) += tx.amount_minor.abs();
                }
                TransactionKind::Expense => {
                    if date >= start_day && date <= end_day {
                        let idx = (date - start_day).num_days() as usize;
                        daily_net[idx] -= tx.amount_minor.abs();
                    }
                    let entry = monthly_expense.entry((year, month)).or_insert((0, 0));
                    entry.0 += tx.amount_minor.abs();

                    if year == current_year && month == current_month {
                        let category = tx.category.clone().unwrap_or_else(|| t(self.state.locale, TextKey::UiOther).to_string());
                        *category_breakdown.entry(category).or_insert(0) += tx.amount_minor.abs();
                    }
                }
                TransactionKind::Refund => {
                    if date >= start_day && date <= end_day {
                        let idx = (date - start_day).num_days() as usize;
                        daily_net[idx] += tx.amount_minor.abs();
                    }
                    let entry = monthly_expense.entry((year, month)).or_insert((0, 0));
                    entry.1 += tx.amount_minor.abs();
                }
                TransactionKind::TransferWallet | TransactionKind::TransferFlow => {}
            }
        }

        let mut cumulative = Vec::with_capacity(daily_net.len());
        let mut running = 0i64;
        for delta in daily_net {
            running += delta;
            cumulative.push(running);
        }

        let min = cumulative.iter().copied().min().unwrap_or(0);
        let max = cumulative.iter().copied().max().unwrap_or(0);
        let shift = if min < 0 { -min } else { 0 };
        let sparkline = cumulative
            .iter()
            .map(|value| (value + shift) as u64)
            .collect::<Vec<_>>();

        let mut breakdown = category_breakdown.into_iter().collect::<Vec<_>>();
        breakdown.sort_by(|a, b| b.1.cmp(&a.1));

        let months = Self::build_last_months(to, 6);
        let mut monthly_expenses_vec = Vec::new();
        let mut monthly_income_vec = Vec::new();
        for (year, month, label) in months {
            let income = monthly_income.get(&(year, month)).copied().unwrap_or(0);
            let (expense, refund) = monthly_expense
                .get(&(year, month))
                .copied()
                .unwrap_or((0, 0));
            let net_expense = (expense - refund).max(0);
            monthly_income_vec.push((label.clone(), income));
            monthly_expenses_vec.push((label, net_expense));
        }

        self.state.stats.category_breakdown = breakdown;
        self.state.stats.monthly_trend = monthly_expenses_vec;
        self.state.stats.monthly_income = monthly_income_vec;
        self.state.stats.sparkline = sparkline;
        self.state.stats.sparkline_min = min;
        self.state.stats.sparkline_max = max;
    }
    pub(crate) fn build_last_months(
        to: DateTime<FixedOffset>,
        count: usize,
    ) -> Vec<(i32, u32, String)> {
        let mut months = Vec::new();
        let mut year = to.year();
        let mut month = to.month();
        for _ in 0..count {
            months.push((year, month, month_label(month)));
            if month == 1 {
                month = 12;
                year -= 1;
            } else {
                month -= 1;
            }
        }
        months.reverse();
        months
    }
    pub(crate) fn format_local_datetime(&self, dt: DateTime<FixedOffset>) -> String {
        dt.with_timezone(&self.tz).format("%Y-%m-%d %H:%M").to_string()
    }

    pub(crate) fn current_currency(&self) -> engine::Currency {
        self.state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur)
    }
}
