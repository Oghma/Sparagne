use super::super::*;

impl App {
    pub(crate) fn stats_next_tab(&mut self) {
        self.state.stats.tab = self.state.stats.tab.next();
    }

    pub(crate) fn stats_prev_tab(&mut self) {
        self.state.stats.tab = self.state.stats.tab.prev();
    }

    pub(crate) fn stats_set_tab(&mut self, index: usize) {
        self.state.stats.tab = StatsTab::from_index(index);
    }

    pub(crate) fn stats_next_month(&mut self) {
        let (year, month) = self.state.stats.current_month;
        if month == 12 {
            self.state.stats.current_month = (year + 1, 1);
        } else {
            self.state.stats.current_month = (year, month + 1);
        }
        self.stats_refresh_month_data();
    }

    /// Navigate to previous month in stats view
    pub(crate) fn stats_prev_month(&mut self) {
        let (year, month) = self.state.stats.current_month;
        if month == 1 {
            self.state.stats.current_month = (year - 1, 12);
        } else {
            self.state.stats.current_month = (year, month - 1);
        }
        self.stats_refresh_month_data();
    }

    /// Refresh category breakdown, income, and expenses from cached per-month
    /// maps after navigating to a different month.
    fn stats_refresh_month_data(&mut self) {
        let key = self.state.stats.current_month;
        self.state.stats.category_breakdown = self
            .state
            .stats
            .monthly_category_breakdowns
            .get(&key)
            .cloned()
            .unwrap_or_default();
        self.state.stats.current_month_income = self
            .state
            .stats
            .monthly_income_map
            .get(&key)
            .copied()
            .unwrap_or(0);
        self.state.stats.current_month_expenses = self
            .state
            .stats
            .monthly_expense_map
            .get(&key)
            .copied()
            .unwrap_or(0);
    }
}
