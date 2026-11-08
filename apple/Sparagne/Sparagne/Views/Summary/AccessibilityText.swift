import Foundation

/// Pure formatting for the VoiceOver labels used across the RIEPILOGO
/// (`SummaryView.swift`) and the ledger's summary panel
/// (`Views/Ledger/SummaryPanel.swift`): kept out
/// of the views themselves so the text can be checked without a window.
///
/// Every piece handed in here (labels, amounts) is already localized or
/// formatted; joining them with punctuation needs no further localization,
/// the same way the summary panel's cards join their own parts. A phrase that
/// adds a real word of its own (`fundGauge`'s "of") goes through
/// `String(localized:)` instead.
enum AccessibilityText {
    /// One of the RIEPILOGO's four month cards: "Income · September, €1,234.56, 2 people".
    static func card(title: String, value: String, caption: String?) -> String {
        [title, value, caption].compactMap(\.self).joined(separator: ", ")
    }

    /// A fund gauge's `accessibilityValue`: "99.8%, €29,931.00 of €30,000.00".
    static func fundGauge(percent: String, filled: String, cap: String) -> String {
        String(localized: "\(percent), \(filled) of \(cap)")
    }

    /// One row of the RIEPILOGO's month table (or one point of a chart),
    /// read as a sentence: "September, Income €9,200.00, Expenses €3,100.00".
    static func monthRow(month: String, figures: [(String, String)]) -> String {
        ([month] + figures.map { "\($0.0) \($0.1)" }).joined(separator: ", ")
    }

    /// One figure of a person x row table: "Income, Elisa: €1,200.00", or
    /// "Income: €1,200.00" when the table has no person column to add.
    static func figure(_ label: String, person: String? = nil, amount: String) -> String {
        guard let person else { return "\(label): \(amount)" }
        return "\(label), \(person): \(amount)"
    }

    /// The cash-flow Sankey as one sentence, what came in and then where it
    /// went: "Income €4,200.00: Casa €1,800.00, Cash €900.00, Savings
    /// €600.00", the balance after the income when the period spent more
    /// than it earned: "Income €2,520.00, From the balance €828.60: …".
    static func sankey(sources: [(String, String)], spending: [(String, String)]) -> String {
        let from = sources.map { "\($0.0) \($0.1)" }.joined(separator: ", ")
        let to = spending.map { "\($0.0) \($0.1)" }.joined(separator: ", ")
        return to.isEmpty ? from : "\(from): \(to)"
    }

    /// The cash-flow graph's VoiceOver elements in reading order: first the
    /// graph as a whole, called `name`, its value the `sankey` sentence over
    /// what was earned, the refunds and the balance, and the flows and the
    /// savings; then a node at a time, its title and its figure with its
    /// share of the income, as the labels on screen write it.
    static func sankeyElements(_ graph: CashFlowSankey, name: String) -> [(label: String, value: String)] {
        var sources = [(String(localized: "Income"), LedgerMoney.amount(graph.income))]
        for id in [CashFlowSankey.refundsId, CashFlowSankey.fromBalanceId] {
            if let node = graph.node(id) { sources.append((node.title, LedgerMoney.amount(node.value))) }
        }
        let spending = graph.nodes(in: 2).map { ($0.title, LedgerMoney.amount($0.value)) }
        let nodes = graph.nodes.map { node -> (label: String, value: String) in
            let amount = LedgerMoney.amount(graph.shown(node))
            guard node.role != .total, let share = graph.shareText(graph.shown(node)) else {
                return (node.title, amount)
            }
            return (node.title, "\(amount), \(share)")
        }
        return [(name, sankey(sources: sources, spending: spending))] + nodes
    }
}
