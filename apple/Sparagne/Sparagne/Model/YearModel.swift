import Foundation
import SparagneCore

/// One person's movement in one bucket of the core's `year_breakdown`
/// (`docs/v2/UI.md` §4). Bucket 0 is everything before January; 1...12 are
/// the months of the year.
struct YearRow: Hashable, Sendable {
    let bucket: Int
    let person: String
    /// Income other than opening balances.
    let income: Int64
    /// Opening balances of wallets (the system category `Opening`), signed.
    let opening: Int64
    /// `net_expense` on envelopes without a cap.
    let cashExpense: Int64
    /// `net_expense` on envelopes with a cap: the "fondi".
    let fundExpense: Int64
}

/// A capped envelope as the RIEPILOGO's gauges show it (`docs/v2/UI.md`
/// §2.2): how full it is against its cap.
struct FundGauge: Identifiable, Hashable, Sendable {
    let id: Uuid
    let name: String
    let cap: Int64
    /// The balance for a net cap, the cumulative income for an income cap
    /// (`DISTILLATO_V1.md` §2.2).
    let filled: Int64

    /// 0...1.
    var fraction: Double {
        guard cap > 0 else { return 0 }
        return min(max(Double(filled) / Double(cap), 0), 1)
    }
}

/// One line of the RIEPILOGO's month table (`docs/v2/UI.md` §2.2), everybody
/// together, plus the running total of each person.
struct YearMonth: Hashable, Sendable {
    let month: MonthKey
    /// After the month on screen, in the year on screen: drawn blank.
    let isFuture: Bool
    /// ENTRATE: income without the opening balances.
    let income: Int64
    /// USCITE: net expense on the envelopes without a cap.
    let cashExpense: Int64
    /// USCITE FONDI: net expense on the envelopes with a cap.
    let fundExpense: Int64
    /// FONDO CASSA: the TOTALE of the month before, plus this month's
    /// opening balances.
    let carried: Int64
    /// TOTALE: `savings + carried - fundExpense`, the wallets' total balance
    /// at the end of the month.
    let total: Int64
    /// The same TOTALE per person, in `YearSummary.people` order.
    let totalByPerson: [Int64]

    /// RISPARMIO.
    var savings: Int64 { income - cashExpense }
}

/// Everything the RIEPILOGO draws for one year, up to the month on screen.
struct YearSummary: Sendable {
    let year: Int
    /// The month on screen: decides the year and which months are future.
    let upTo: MonthKey
    /// One column per author of the vault, in the core's order.
    let people: [String]
    /// FONDO CASSA INIZIALE per person: everything before January.
    let initialByPerson: [Int64]
    /// January first, always twelve.
    let months: [YearMonth]
    /// The active capped envelopes, in the vault's order.
    let funds: [FundGauge]

    var initial: Int64 { initialByPerson.reduce(0, +) }

    /// Pure arithmetic over the query's rows and the vault's envelopes, so it
    /// can be tested without a database (`docs/v2/UI.md` §2.2 for the
    /// definitions).
    static func build(year: Int, upTo: MonthKey, rows: [YearRow], flows: [FlowView]) -> YearSummary {
        // Implemented by the summary work; the stub keeps the window building.
        YearSummary(year: year, upTo: upTo, people: [], initialByPerson: [], months: [], funds: [])
    }
}
