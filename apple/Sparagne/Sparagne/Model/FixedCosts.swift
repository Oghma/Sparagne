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
    static func build(templates: [RecurringView], year: YearSummary) -> FixedCosts? {
        // A stub for now.
        _ = (templates, year)
        return nil
    }
}
