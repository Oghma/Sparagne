import Foundation
import SparagneCore

/// One active envelope as the RIEPILOGO's "Buste" table reads it: where it
/// stands at the end of the month on screen, and what the year put in and
/// took out of it, by person.
struct EnvelopeYearRow: Identifiable, Hashable, Sendable {
    let id: Uuid
    let name: String
    let isUnallocated: Bool
    /// The cap's kind on a fund, `nil` on an envelope without one.
    let capKind: FundGauge.Kind?
    /// The balance at the end of the month on screen, from the core's
    /// `balance_breakdown` on the envelopes.
    let balance: Int64
    /// Income rows of the year so far: transfers between envelopes are
    /// neither earned nor spent, so the allocation plan's moves are out.
    let income: Int64
    /// Net expense of the year so far, everybody together.
    let expense: Int64
    /// `expense` by person, in `EnvelopeYear.people` order.
    let expenseByPerson: [Int64]
}

/// The "Buste" table: every active envelope, Unallocated first.
struct EnvelopeYear: Hashable, Sendable {
    /// The vault's people, the columns of the expense split.
    let people: [String]
    let rows: [EnvelopeYearRow]

    /// `flows` are the vault's active envelopes, `flowPerson` the core's
    /// envelope x person matrix over January up to the month on screen,
    /// `balances` its `balance_breakdown` on the envelopes over the year's
    /// bounds (bucket 0 before January), `people` the columns.
    static func build(
        flows: [FlowView],
        flowPerson: [FlowPersonTotals],
        balances: [BalanceDelta],
        upTo: MonthKey,
        people: [String]
    ) -> EnvelopeYear {
        // A stub for now.
        _ = (flows, flowPerson, balances, upTo)
        return EnvelopeYear(people: people, rows: [])
    }
}
