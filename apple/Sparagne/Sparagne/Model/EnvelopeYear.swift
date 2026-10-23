import Foundation
import SparagneCore

/// One envelope of the "Buste" table: its balance at the month's end and its year by person.
struct EnvelopeYearRow: Identifiable, Hashable, Sendable {
    let id: Uuid
    let name: String
    let isUnallocated: Bool
    /// The cap's kind on a fund, `nil` on an envelope without one.
    let capKind: FundGauge.Kind?
    /// The balance at the end of the month on screen, from the core's
    /// `balance_breakdown` on the envelopes.
    let balance: Int64
    /// What came into the envelope in the year so far: income rows and
    /// moves from other envelopes, the allocation plan's shares among them.
    /// Each envelope is a cash flow of its own.
    let income: Int64
    /// What went out of it: net expense and moves to other envelopes,
    /// everybody together.
    let expense: Int64
    /// `expense` by person, in `EnvelopeYear.people` order: a move counts
    /// for whoever made it.
    let expenseByPerson: [Int64]
}

/// The "Buste" table: every active envelope, Unallocated first.
struct EnvelopeYear: Hashable, Sendable {
    /// The vault's people, the columns of the expense split.
    let people: [String]
    let rows: [EnvelopeYearRow]

    /// `flows` are the vault's active envelopes, `flowPerson` the core's
    /// envelope x person matrix over January up to the month on screen and
    /// `transfers` the moves between envelopes over the same range,
    /// `balances` its `balance_breakdown` on the envelopes over the year's
    /// bounds (bucket 0 before January), `people` the columns.
    ///
    /// The balance sums every bucket up to the month on screen, bucket 0
    /// included, so it is where the envelope stood at the end of that month.
    /// Income and expense add the moves to the matrix's income, expense and
    /// refund rows: the household neither earns nor spends what goes from
    /// one envelope to another, but each envelope gains or loses it, and the
    /// month table leaves the moves out instead.
    static func build(
        flows: [FlowView],
        flowPerson: [FlowPersonTotals],
        transfers: [FlowTransferTotals] = [],
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
        var moves: [Uuid: [FlowTransferTotals]] = [:]
        for move in transfers { moves[move.flowId, default: []].append(move) }

        let ordered = flows.filter(\.isUnallocated) + flows.filter { !$0.isUnallocated }
        let rows = ordered.map { flow in
            let flowLines = lines[flow.id, default: []]
            let flowMoves = moves[flow.id, default: []]
            return EnvelopeYearRow(
                id: flow.id,
                name: flow.name,
                isUnallocated: flow.isUnallocated,
                capKind: FundGauge.Kind(flow.mode),
                balance: balance[flow.id, default: 0],
                income: flowLines.reduce(0) { $0 + $1.income } + flowMoves.reduce(0) { $0 + $1.movedIn },
                // The core nets expense against refund per envelope and
                // person; the envelope's figure is the sum of those.
                expense: flowLines.reduce(0) { $0 + $1.netExpense } + flowMoves.reduce(0) { $0 + $1.movedOut },
                expenseByPerson: people.map { name in
                    flowLines.filter { $0.person == name }.reduce(0) { $0 + $1.netExpense }
                        + flowMoves.filter { $0.person == name }.reduce(0) { $0 + $1.movedOut }
                }
            )
        }
        return EnvelopeYear(people: people, rows: rows)
    }
}
