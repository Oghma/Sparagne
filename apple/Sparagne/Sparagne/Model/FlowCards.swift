import Foundation
import SparagneCore

/// One card of the RIEPILOGO's flows: an active flow at the end of the month
/// on screen, what came into it and went out of it in that month, and its
/// cap's bar when it has one.
struct FlowCard: Identifiable, Hashable, Sendable {
    let id: Uuid
    let name: String
    /// Where the flow stood at the end of the month on screen.
    let balance: Int64
    /// The month's income rows and moves in, counted as the "Flussi" table
    /// counts the year's.
    let monthIncome: Int64
    /// The month's net expense and moves out.
    let monthExpense: Int64
    /// `nil` on a flow without a cap. A cap on the balance fills with
    /// `balance`; one on the income with the cumulative income, which the
    /// core keeps as of today, not as of the month on screen.
    let gauge: FundGauge?

    /// A cap reached: the card says so and goes after the open flows.
    var isComplete: Bool { gauge.map { $0.filled >= $0.cap } ?? false }

    /// The cards of `flows` in the order the RIEPILOGO draws them: first
    /// the capped flows still filling, which ask for money, then the open
    /// ones, then the capped ones already full. The vault's order within
    /// each group, so a card moves only when it changes group, never because
    /// another one filled a little more. Unallocated has no card: it is money
    /// waiting for the Riparto, not a flow of its own.
    ///
    /// `month` is the "Flussi" table built on the month on screen alone: its
    /// balances are the month's end, its income and expense the month's.
    static func cards(flows: [FlowView], month: EnvelopeYear) -> [FlowCard] {
        let modes = Dictionary(flows.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let cards = month.rows.filter { !$0.isUnallocated }.map { row in
            let flow = modes[row.id]
            return FlowCard(
                id: row.id,
                name: row.name,
                balance: row.balance,
                monthIncome: row.income,
                monthExpense: row.expense,
                gauge: flow.flatMap {
                    FundGauge(id: row.id, name: row.name, mode: $0.mode, balance: row.balance, incomeTotal: $0.incomeTotal)
                }
            )
        }
        return cards.enumerated()
            .sorted { ($0.element.group, $0.offset) < ($1.element.group, $1.offset) }
            .map(\.element)
    }

    private var group: Int {
        guard gauge != nil else { return 1 }
        return isComplete ? 2 : 0
    }
}
