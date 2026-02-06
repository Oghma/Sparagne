use chrono::{Datelike, Local};

use api_types::stats::Statistic;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsTab {
    CashFlow,
    Spending,
    NetWorth,
}

cyclic_enum!(StatsTab { CashFlow => 0, Spending => 1, NetWorth => 2 });

#[derive(Debug)]
pub struct StatsState {
    pub data: Option<Statistic>,
    pub error: Option<String>,
    /// Current month being viewed (year, month 1-12)
    pub current_month: (i32, u32),
    pub tab: StatsTab,
    /// Category breakdown computed from transactions
    pub category_breakdown: Vec<(String, i64)>,
    /// Monthly trend data (last 6 months of expenses)
    pub monthly_trend: Vec<(String, i64)>,
    /// Monthly trend data (last 6 months of income)
    pub monthly_income: Vec<(String, i64)>,
    /// Sparkline data for last 30 days (shifted to >= 0)
    pub sparkline: Vec<u64>,
    pub sparkline_min: i64,
    pub sparkline_max: i64,
}

impl Default for StatsState {
    fn default() -> Self {
        let now = Local::now();
        Self {
            data: None,
            error: None,
            current_month: (now.year(), now.month()),
            tab: StatsTab::CashFlow,
            category_breakdown: Vec::new(),
            monthly_trend: Vec::new(),
            monthly_income: Vec::new(),
            sparkline: Vec::new(),
            sparkline_min: 0,
            sparkline_max: 0,
        }
    }
}
