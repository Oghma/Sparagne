import Foundation
import SparagneCore

/// The fifth card of the month: what the recurring expenses commit every
/// month, and the slice of the income that is.
struct FixedCosts: Hashable, Sendable {
    /// Minor units a month: the enabled expense templates at their monthly
    /// equivalent, as the Ricorrenze status line counts them
    /// (`RecurringMonthly`).
    let monthly: Int64
    /// `monthly` over the average monthly income of the year's finished
    /// months; `nil` without an income to measure it on.
    let shareOfIncome: Double?

    /// `nil` without an enabled expense template: the card then says so
    /// instead of showing a zero.
    ///
    /// The income is the average of the finished months, not of the month on
    /// screen: a live month has not been paid in full yet, and one month's
    /// salary is too uneven to measure a commitment on.
    static func build(templates: [RecurringView], year: YearSummary) -> FixedCosts? {
        let monthly = RecurringMonthly.totals(templates).expenses
        guard monthly != 0 else { return nil }
        let finished = year.finished
        var shareOfIncome: Double?
        if !finished.isEmpty {
            let averageIncome = Double(finished.reduce(Int64(0)) { $0 + $1.income }) / Double(finished.count)
            if averageIncome > 0 { shareOfIncome = Double(monthly) / averageIncome }
        }
        return FixedCosts(monthly: monthly, shareOfIncome: shareOfIncome)
    }
}
