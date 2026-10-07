import Foundation
import SparagneCore

/// What the templates that run cost and bring in, a month at a time: the
/// "Uscite fisse al mese" and "Entrate fisse" of the Ricorrenze status line
/// (`docs/v2/UI.md` §2.5).
///
/// A month is not a whole number of days or weeks, so a daily or weekly
/// template is spread over the average month: 365.25 days or 52.1775 weeks a
/// year, over twelve. That is why the status line writes "≈". Everything is
/// `Decimal`, rounded to the cent once, on the total.
enum RecurringMonthly {
    struct Totals: Equatable {
        /// Minor units a month.
        var expenses: Int64
        var income: Int64
    }

    private static let daysPerMonth = Decimal(36_525) / 100 / 12
    private static let weeksPerMonth = Decimal(521_775) / 10_000 / 12

    /// How many times a month the schedule comes round, on average.
    static func perMonth(_ schedule: Schedule) -> Decimal {
        // The core refuses an interval of zero; this only keeps the
        // division defined if one ever arrives.
        let interval = Decimal(max(schedule.interval, 1))
        switch schedule.frequency {
        case .daily: return daysPerMonth / interval
        case .weekly: return weeksPerMonth / interval
        case .monthly: return 1 / interval
        case .yearly: return 1 / (12 * interval)
        }
    }

    /// The templates that are enabled and not archived, each at its monthly
    /// equivalent, expenses and income apart.
    static func totals(_ templates: [RecurringView]) -> Totals {
        var expenses = Decimal(0)
        var income = Decimal(0)
        for template in templates where template.enabled && !template.archived {
            let monthly = Decimal(template.amount) * perMonth(template.schedule)
            switch template.kind {
            case .expense: expenses += monthly
            case .income: income += monthly
            // A template is only ever an income or an expense.
            case .refund, .transferWallet, .transferFlow: break
            }
        }
        return Totals(expenses: cents(expenses), income: cents(income))
    }

    /// Minor units, half away from zero.
    static func cents(_ value: Decimal) -> Int64 {
        var input = value
        var rounded = Decimal()
        NSDecimalRound(&rounded, &input, 0, .plain)
        return NSDecimalNumber(decimal: rounded).int64Value
    }
}
