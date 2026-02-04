use super::super::*;

use std::str::FromStr;

use crate::{
    app::{
        errors::login_message_for_error,
        format::{map_currency, month_label},
    },
    error::Result,
};
use api_types::{
    transaction::{TransactionList, TransactionView},
    vault::Vault,
};
use chrono::{DateTime, Datelike, FixedOffset, Offset, TimeZone, Utc};
use chrono_tz::Tz;

impl App {
    pub(crate) fn parse_local_datetime(
        &self,
        input: &str,
    ) -> std::result::Result<DateTime<FixedOffset>, String> {
        let naive = chrono::NaiveDateTime::parse_from_str(input.trim(), "%Y-%m-%d %H:%M")
            .map_err(|_| "Formato data non valido. Usa YYYY-MM-DD HH:MM".to_string())?;
        let tz = Tz::from_str(self.config.timezone.as_str()).unwrap_or(Tz::UTC);
        let localized = tz.from_local_datetime(&naive);
        let dt: DateTime<Tz> = match localized {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(dt, _) => dt,
            chrono::LocalResult::None => {
                return Err("Data/ora non valida.".to_string());
            }
        };
        let offset = dt.offset().fix();
        Ok(dt.with_timezone(&offset))
    }

    pub(crate) fn now_in_timezone(&self) -> DateTime<FixedOffset> {
        let tz = Tz::from_str(self.config.timezone.as_str()).unwrap_or(Tz::UTC);
        let now = Utc::now();
        let local = tz.from_utc_datetime(&now.naive_utc());
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
            .stats_get(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                payload,
            )
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
                self.connection_error("Errore connessione");
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
                .transactions_list(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
                    payload,
                )
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

        let tz = Tz::from_str(self.config.timezone.as_str()).unwrap_or(Tz::UTC);
        let start_day = (to - chrono::Duration::days(29))
            .with_timezone(&tz)
            .date_naive();
        let end_day = to.with_timezone(&tz).date_naive();
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

            let local = tx.occurred_at.with_timezone(&tz);
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
                        let category = tx.category.clone().unwrap_or_else(|| "Other".to_string());
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
        let tz = Tz::from_str(self.config.timezone.as_str()).unwrap_or(Tz::UTC);
        dt.with_timezone(&tz).format("%Y-%m-%d %H:%M").to_string()
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
