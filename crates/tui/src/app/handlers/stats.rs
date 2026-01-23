use super::super::*;

impl App {
    pub(crate) fn stats_next_month(&mut self) {
        let (year, month) = self.state.stats.current_month;
        if month == 12 {
            self.state.stats.current_month = (year + 1, 1);
        } else {
            self.state.stats.current_month = (year, month + 1);
        }
    }

    /// Navigate to previous month in stats view
    pub(crate) fn stats_prev_month(&mut self) {
        let (year, month) = self.state.stats.current_month;
        if month == 1 {
            self.state.stats.current_month = (year - 1, 12);
        } else {
            self.state.stats.current_month = (year, month - 1);
        }
    }
}
