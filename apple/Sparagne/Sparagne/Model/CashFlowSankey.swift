import Foundation
import SparagneCore

/// The RIEPILOGO's cash-flow Sankey for one period: the income by category
/// flowing into one Entrate node, from there into what each flow spent and
/// into the savings, and from each flow into the categories it paid. When
/// the period spent more than it earned, a "from the balance" node feeds
/// Entrate instead of the savings. Every node is worth the sum of its links,
/// so the graph balances whatever the refunds did.
///
/// Prepared as a contract: `build` returns `nil` until it is written.
struct CashFlowSankey: Equatable, Sendable {
    /// What a node stands for, which decides its column and its colour.
    enum Role: Hashable, Sendable {
        case income, fromBalance, total, flow, savings, category
    }

    struct Node: Identifiable, Hashable, Sendable {
        let id: String
        /// 0 income, 1 Entrate, 2 flows and savings, 3 categories.
        let column: Int
        /// Already localized: Unallocated, Uncategorized and the folded rest
        /// get their app names here.
        let title: String
        /// Minor units: the sum of the node's links.
        let value: Int64
        let role: Role
        /// The node that folds what is past the column's limit.
        let isOther: Bool
    }

    struct Link: Identifiable, Hashable, Sendable {
        let from: String
        let to: String
        /// Minor units, `> 0`.
        let value: Int64

        var id: String { "\(from)>\(to)" }
    }

    let nodes: [Node]
    let links: [Link]
    /// What the period earned, openings left out: the base of every
    /// percentage, 0 when nothing came in.
    let income: Int64

    /// Income categories with a node of their own before the rest is folded.
    static let incomeLimit = 5
    /// Expense categories with a node of their own, as the year's list.
    static let categoryLimit = SummaryCategories.limit

    /// The graph of one period, `nil` when it earned and spent nothing.
    /// `income` is `category_totals` over the period, `expenses`
    /// `flow_category_totals` over the same period and person, `flows` the
    /// vault's flows, archived included, for their names.
    static func build(
        income: [CategoryTotals],
        expenses: [FlowCategoryTotals],
        flows: [FlowView],
        incomeLimit: Int = CashFlowSankey.incomeLimit,
        categoryLimit: Int = CashFlowSankey.categoryLimit
    ) -> CashFlowSankey? {
        nil
    }
}
