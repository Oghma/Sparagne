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
    ///
    /// The balance sums every bucket up to the month on screen, bucket 0
    /// included, so it is where the envelope stood at the end of that month,
    /// moves of the allocation plan and all. Income and expense come from the
    /// matrix instead, which reads income, expense and refund rows only: a
    /// move between envelopes is neither earned nor spent.
    static func build(
        flows: [FlowView],
        flowPerson: [FlowPersonTotals],
        balances: [BucketTargetTotals],
        upTo: MonthKey,
        people: [String]
    ) -> EnvelopeYear {
        var balance: [Uuid: Int64] = [:]
        for delta in balances where Int(delta.bucket) <= upTo.month {
            balance[delta.targetId, default: 0] += delta.delta
        }
        var lines: [Uuid: [FlowPersonTotals]] = [:]
        for line in flowPerson { lines[line.flowId, default: []].append(line) }

        let ordered = flows.filter(\.isUnallocated) + flows.filter { !$0.isUnallocated }
        let rows = ordered.map { flow in
            let flowLines = lines[flow.id, default: []]
            return EnvelopeYearRow(
                id: flow.id,
                name: flow.name,
                isUnallocated: flow.isUnallocated,
                capKind: Self.capKind(flow.mode),
                balance: balance[flow.id, default: 0],
                income: flowLines.reduce(0) { $0 + $1.income },
                // The core nets expense against refund per envelope and
                // person; the envelope's figure is the sum of those.
                expense: flowLines.reduce(0) { $0 + $1.netExpense },
                expenseByPerson: people.map { name in
                    flowLines.filter { $0.person == name }.reduce(0) { $0 + $1.netExpense }
                }
            )
        }
        return EnvelopeYear(people: people, rows: rows)
    }

    /// What the cap is measured on, as the fund cards say it; `nil` without
    /// a cap.
    private static func capKind(_ mode: FlowMode) -> FundGauge.Kind? {
        switch mode {
        case .unlimited: nil
        case .netCapped: .balance
        case .incomeCapped: .income
        }
    }
}
