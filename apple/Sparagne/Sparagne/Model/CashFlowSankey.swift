import Foundation
import SparagneCore

/// The RIEPILOGO's cash-flow Sankey for one period: the income by category
/// flowing into one Entrate node, from there into what each flow spent and
/// into the savings, and from each flow into the categories it paid. When
/// the period spent more than came in, a "from the balance" node feeds
/// Entrate instead of the savings. Every node is worth the sum of its links,
/// so the graph balances whatever the refunds did.
///
/// Transfers between flows (the Riparto, Sposta) are not in it: a flow is
/// worth what it spent, not what it was filled with. A refund nets the
/// expense of its flow and category; what it gives back beyond that pair's
/// expense (last month's bill refunded this month) comes in as a «Rimborsi»
/// node beside the income, so the savings are the cards' own: income minus
/// expenses net of every refund. The cards only differ when one person's
/// refunds in a month exceed their expenses, which they count as nothing.
struct CashFlowSankey: Equatable, Sendable {
    /// What a node stands for, which decides its column and its colour.
    enum Role: Hashable, Sendable {
        case income, refunds, fromBalance, total, flow, savings, category
    }

    struct Node: Identifiable, Hashable, Sendable {
        let id: String
        /// 0 income, 1 Entrate, 2 flows and savings, 3 categories.
        let column: Int
        /// Already localized: Unallocated, Uncategorized and the folded rest
        /// get their app names here.
        let title: String
        /// Minor units: the sum of the node's links, `> 0`.
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

    /// Column by column, each top to bottom: income heaviest first, the
    /// folded rest, the refunds, then the balance; flows heaviest first, then the
    /// savings; categories heaviest first, then the folded rest.
    let nodes: [Node]
    let links: [Link]
    /// What the period earned, openings left out: the base of every
    /// percentage, 0 when nothing came in.
    let income: Int64

    /// Income categories with a node of their own before the rest is folded.
    static let incomeLimit = 5
    /// Expense categories with a node of their own, as the year's list.
    static let categoryLimit = SummaryCategories.limit
    /// An income category under this share of what was earned, or an
    /// expense category under this share of what was spent, folds into the
    /// rest even within the limit: a sliver of a node only crowds the labels.
    static let foldShare = 0.015

    static let totalId = "total"
    static let savingsId = "savings"
    static let fromBalanceId = "balance"
    static let refundsId = "refunds"

    // MARK: - Reading

    func node(_ id: String) -> Node? { nodes.first { $0.id == id } }

    /// The columns that have a node, left to right: 0 is missing when one
    /// income category is all that came in, 3 when nothing was spent.
    var columns: [Int] { Array(Set(nodes.map(\.column))).sorted() }

    /// The nodes of `column`, top to bottom.
    func nodes(in column: Int) -> [Node] { nodes.filter { $0.column == column } }

    func incoming(_ id: String) -> [Link] { links.filter { $0.to == id } }
    func outgoing(_ id: String) -> [Link] { links.filter { $0.from == id } }

    /// What the flows spent between them.
    var expenses: Int64 { nodes.reduce(0) { $0 + ($1.role == .flow ? $1.value : 0) } }

    /// The figure a node says: its value, except Entrate, which says what
    /// was earned even when refunds or the balance made up the rest (0 with
    /// no income).
    func shown(_ node: Node) -> Int64 { node.role == .total ? income : node.value }

    /// `value` over the income, `nil` when nothing came in.
    func share(_ value: Int64) -> Double? {
        income > 0 ? Double(value) / Double(income) : nil
    }

    /// `"14,2%"`: `value`'s share of the income as the labels and the
    /// tooltip write it; `"< 0,1%"` for a share that would round to `"0,0%"`
    /// (7,90 of interest in a year of salary), which is not nothing, and
    /// `"> 99,9%"` for one that would round to `"100,0%"` without being the
    /// whole income. `nil` when nothing was earned.
    func shareText(_ value: Int64) -> String? {
        guard income > 0 else { return nil }
        // Within 0,05% of either end, that included: the formatter rounds
        // half to even, so 0,05% would print 0,0% too. `d <= income / 2000`
        // is `d × 2000 <= income` without the overflow.
        if value > 0, value <= income / 2000 {
            return "< \(LedgerMoney.number(0.1, decimals: 1))%"
        }
        if value < income, income - value <= income / 2000 {
            return "> \(LedgerMoney.number(99.9, decimals: 1))%"
        }
        return LedgerMoney.percent(value, of: income, decimals: 1)
    }

    // MARK: - Hover

    /// What the tooltip of node `id` says: its title and the figure its
    /// label shows. `nil` when the graph has no such node, as after a reload
    /// under a pointer at rest.
    func tip(forNode id: String) -> (title: String, value: Int64)? {
        guard let node = node(id) else { return nil }
        return (node.title, shown(node))
    }

    /// "Casa → Utenze" and the ribbon's value; `nil` when the graph has no
    /// such ribbon.
    func tip(forLink id: String) -> (title: String, value: Int64)? {
        guard let link = links.first(where: { $0.id == id }), let from = node(link.from), let to = node(link.to)
        else { return nil }
        return ("\(from.title) \u{2192} \(to.title)", link.value)
    }

    // MARK: - Building

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
        let earning = heaviestFirst(income.filter { $0.income > 0 }.map { (item: $0, value: $0.income) })
        let earned = earning.reduce(Int64(0)) { $0 + $1.value }
        let spending = Spending(expenses)
        let spent = spending.total
        let refunded = spending.excessRefunds
        guard earned > 0 || spent > 0 || refunded > 0 else { return nil }

        // What came in against what went out: the savings and what the
        // balance gave are the cards' figures, every refund netted.
        let cameIn = earned + refunded
        let fromBalance = max(spent - cameIn, 0)
        let savings = max(cameIn - spent, 0)
        var nodes: [Node] = []
        var links: [Link] = []

        // Column 0: the income by category, the refunds beyond their
        // expenses, then the balance it took. Folded on what was earned.
        let (keptIncome, restIncome) = fold(earning, limit: incomeLimit, total: earned)
        var sources = keptIncome.map { entry in
            Node(
                id: "in:\(entry.item.categoryId)", column: 0,
                title: SummaryText.categoryName(entry.item.name, isSystem: entry.item.isSystem),
                value: entry.value, role: .income, isOther: false
            )
        }
        if !restIncome.isEmpty {
            sources.append(
                Node(
                    id: "in:other", column: 0, title: SummaryText.categoryName("", isSystem: false, isOther: true),
                    value: restIncome.reduce(0) { $0 + $1.value }, role: .income, isOther: true
                )
            )
        }
        if refunded > 0 {
            sources.append(
                Node(
                    id: refundsId, column: 0, title: String(localized: "Refunds"),
                    value: refunded, role: .refunds, isOther: false
                )
            )
        }
        if fromBalance > 0 {
            sources.append(
                Node(
                    id: fromBalanceId, column: 0, title: String(localized: "From the balance"),
                    value: fromBalance, role: .fromBalance, isOther: false
                )
            )
        }
        // One income category and nothing else would repeat Entrate.
        let showsSources = !(sources.count == 1 && sources[0].role == .income)
        if showsSources {
            nodes += sources
            links += sources.map { Link(from: $0.id, to: totalId, value: $0.value) }
        }

        // Column 1: Entrate, worth what goes through it.
        nodes.append(
            Node(
                id: totalId, column: 1, title: String(localized: "Income"),
                value: max(cameIn, spent), role: .total, isOther: false
            )
        )

        // Column 2: what each flow spent, heaviest first, then the savings.
        let names = Dictionary(flows.map { ($0.id, $0.isUnallocated ? NameBook.unallocatedLabel : $0.name) }) { first, _ in first }
        let flowOrder = heaviestFirst(spending.flows)
        for entry in flowOrder {
            let id = "flow:\(entry.item)"
            nodes.append(
                Node(
                    id: id, column: 2, title: names[entry.item] ?? TransactionRow.placeholder,
                    value: entry.value, role: .flow, isOther: false
                )
            )
            links.append(Link(from: totalId, to: id, value: entry.value))
        }
        if savings > 0 {
            nodes.append(
                Node(
                    id: savingsId, column: 2, title: String(localized: "Savings"),
                    value: savings, role: .savings, isOther: false
                )
            )
            links.append(Link(from: totalId, to: savingsId, value: savings))
        }

        // Column 3: the categories, global, heaviest first, then the rest.
        let (keptCategories, restCategories) = fold(
            heaviestFirst(spending.categories), limit: categoryLimit, total: spent
        )
        let folded = Set(restCategories.map(\.item))
        for entry in keptCategories {
            let category = spending.names[entry.item]
            nodes.append(
                Node(
                    id: "cat:\(entry.item)", column: 3,
                    title: SummaryText.categoryName(category?.name ?? "", isSystem: category?.isSystem ?? false),
                    value: entry.value, role: .category, isOther: false
                )
            )
        }
        if !restCategories.isEmpty {
            nodes.append(
                Node(
                    id: "cat:other", column: 3, title: SummaryText.categoryName("", isSystem: false, isOther: true),
                    value: restCategories.reduce(0) { $0 + $1.value }, role: .category, isOther: true
                )
            )
        }
        // One ribbon per flow and category, the folded ones as one.
        for flow in flowOrder {
            var toRest: Int64 = 0
            for category in keptCategories {
                if let value = spending.pairs[Pair(flow: flow.item, category: category.item)] {
                    links.append(Link(from: "flow:\(flow.item)", to: "cat:\(category.item)", value: value))
                }
            }
            for category in folded {
                toRest += spending.pairs[Pair(flow: flow.item, category: category)] ?? 0
            }
            if toRest > 0 {
                links.append(Link(from: "flow:\(flow.item)", to: "cat:other", value: toRest))
            }
        }

        return CashFlowSankey(nodes: nodes, links: links, income: earned)
    }

    // MARK: - Helpers

    private struct Pair: Hashable {
        let flow: Uuid
        let category: Uuid
    }

    /// The core's rows summed per flow and category: each pair's net expense
    /// (a refund larger than the expense leaves the pair out), the flows' and
    /// categories' totals in the order the core first names them, and what
    /// the refunds gave back beyond their pairs' expenses.
    private struct Spending {
        var pairs: [Pair: Int64] = [:]
        var flows: [(item: Uuid, value: Int64)] = []
        var categories: [(item: Uuid, value: Int64)] = []
        var names: [Uuid: (name: String, isSystem: Bool)] = [:]
        var total: Int64 = 0
        var excessRefunds: Int64 = 0

        init(_ rows: [FlowCategoryTotals]) {
            // Expense and refund per pair first, netted once: the core sends
            // one row a pair, and two would still net as one.
            var order: [Pair] = []
            var expense: [Pair: Int64] = [:]
            var refund: [Pair: Int64] = [:]
            for row in rows {
                let pair = Pair(flow: row.flowId, category: row.categoryId)
                if expense[pair] == nil {
                    order.append(pair)
                    if names[row.categoryId] == nil {
                        names[row.categoryId] = (name: row.name, isSystem: row.isSystem)
                    }
                }
                expense[pair, default: 0] += row.expense
                refund[pair, default: 0] += row.refund
            }
            var flowIndex: [Uuid: Int] = [:]
            var categoryIndex: [Uuid: Int] = [:]
            for pair in order {
                let net = (expense[pair] ?? 0) - (refund[pair] ?? 0)
                guard net > 0 else {
                    excessRefunds -= net
                    continue
                }
                pairs[pair] = net
                total += net
                if let index = flowIndex[pair.flow] {
                    flows[index].value += net
                } else {
                    flowIndex[pair.flow] = flows.count
                    flows.append((item: pair.flow, value: net))
                }
                if let index = categoryIndex[pair.category] {
                    categories[index].value += net
                } else {
                    categoryIndex[pair.category] = categories.count
                    categories.append((item: pair.category, value: net))
                }
            }
        }
    }

    /// Heaviest first; a tie keeps the order it came in, the core's.
    private static func heaviestFirst<T>(_ entries: [(item: T, value: Int64)]) -> [(item: T, value: Int64)] {
        entries.enumerated()
            .sorted { $0.element.value != $1.element.value ? $0.element.value > $1.element.value : $0.offset < $1.offset }
            .map(\.element)
    }

    /// The first `limit` entries at or above `foldShare` of `total`, and the
    /// rest to fold. A rest of one keeps its own node: folding it would only
    /// hide its name; and when none reaches the share, the first `limit`
    /// keep theirs all the same, so the column is never one «Other».
    private static func fold<T>(
        _ entries: [(item: T, value: Int64)], limit: Int, total: Int64
    ) -> (kept: [(item: T, value: Int64)], rest: [(item: T, value: Int64)]) {
        let threshold = Double(total) * foldShare
        var kept: [(item: T, value: Int64)] = []
        var rest: [(item: T, value: Int64)] = []
        for (index, entry) in entries.enumerated() {
            if index < limit, Double(entry.value) >= threshold {
                kept.append(entry)
            } else {
                rest.append(entry)
            }
        }
        if kept.isEmpty {
            kept = Array(entries.prefix(limit))
            rest = Array(entries.dropFirst(limit))
        }
        if rest.count == 1 {
            kept += rest
            rest = []
        }
        return (kept, rest)
    }
}
