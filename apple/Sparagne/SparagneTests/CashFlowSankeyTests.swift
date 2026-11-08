import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Hand-made core rows for the cash-flow Sankey, shared with
/// `SankeyLayoutTests`: the core's query is tested in the core.
enum SankeyFixture {
    /// An income category, its id its name.
    static func earned(_ name: String, _ amount: Int64, system: Bool = false) -> CategoryTotals {
        CategoryTotals(
            categoryId: name, name: name, isSystem: system,
            income: amount, expense: 0, refund: 0, netExpense: 0, count: 1
        )
    }

    /// What `flow` spent on `category`, net of the refund as the core nets
    /// it.
    static func spent(
        _ flow: String, _ category: String, _ expense: Int64, refund: Int64 = 0, system: Bool = false
    ) -> FlowCategoryTotals {
        FlowCategoryTotals(
            flowId: flow, categoryId: category, name: category, isSystem: system,
            expense: expense, refund: refund, netExpense: max(expense - refund, 0)
        )
    }

    static var flows: [FlowView] {
        [
            TestFlow.make("unallocated", id: "Unallocated", unallocated: true),
            TestFlow.make("Casa"),
            TestFlow.make("Spesa"),
            TestFlow.make("Vacanze"),
            TestFlow.make("Auto"),
        ]
    }

    /// A month that saved: 2.700,00 in, 1.369,00 out.
    static var month: CashFlowSankey? {
        CashFlowSankey.build(
            income: [earned("Stipendio", 240_000), earned("Interessi", 18_000), earned("Rimborsi spese", 12_000)],
            expenses: [
                spent("Casa", "Mutuo", 80_000),
                spent("Casa", "Utenze", 13_900),
                spent("Spesa", "Spesa", 20_000),
                spent("Spesa", "Ristoranti", 6_000, refund: 1_000),
                spent("Unallocated", "Ristoranti", 8_000),
                spent("Vacanze", "Viaggi", 10_000),
            ],
            flows: flows
        )
    }

    /// Every node is worth its links: a source what leaves it, a sink what
    /// reaches it, a node in between both; none is alone or empty, and every
    /// link joins two nodes of the graph, left to right.
    static func expectBalanced(_ sankey: CashFlowSankey, sourceLocation: SourceLocation = #_sourceLocation) {
        let ids = Set(sankey.nodes.map(\.id))
        #expect(ids.count == sankey.nodes.count, "node ids are unique", sourceLocation: sourceLocation)
        for node in sankey.nodes {
            let incoming = sankey.incoming(node.id)
            let outgoing = sankey.outgoing(node.id)
            #expect(node.value > 0, "\(node.id) is worth something", sourceLocation: sourceLocation)
            #expect(!incoming.isEmpty || !outgoing.isEmpty, "\(node.id) has a link", sourceLocation: sourceLocation)
            if !incoming.isEmpty {
                #expect(
                    incoming.reduce(0) { $0 + $1.value } == node.value,
                    "\(node.id) is worth what reaches it", sourceLocation: sourceLocation
                )
            }
            if !outgoing.isEmpty {
                #expect(
                    outgoing.reduce(0) { $0 + $1.value } == node.value,
                    "\(node.id) is worth what leaves it", sourceLocation: sourceLocation
                )
            }
        }
        for link in sankey.links {
            #expect(link.value > 0, sourceLocation: sourceLocation)
            let from = sankey.node(link.from)
            let to = sankey.node(link.to)
            #expect(from != nil && to != nil, "\(link.id) joins two nodes", sourceLocation: sourceLocation)
            if let from, let to {
                #expect(to.column == from.column + 1, "\(link.id) goes one column right", sourceLocation: sourceLocation)
            }
        }
        #expect(Set(sankey.links.map(\.id)).count == sankey.links.count, sourceLocation: sourceLocation)
    }
}

/// The RIEPILOGO's cash-flow Sankey from the core's rows: income by
/// category into Entrate, Entrate into the flows and the savings, the flows
/// into the categories.
struct CashFlowSankeyTests {
    private typealias F = SankeyFixture

    private static func titles(_ sankey: CashFlowSankey, column: Int) -> [String] {
        sankey.nodes(in: column).map(\.title)
    }

    private static func value(_ sankey: CashFlowSankey, _ id: String) -> Int64? {
        sankey.node(id)?.value
    }

    @Test("Every node is worth its links, and Entrate lets through all that came in, to the flows and the savings")
    func balanced() throws {
        let sankey = try #require(F.month)
        F.expectBalanced(sankey)
        #expect(sankey.columns == [0, 1, 2, 3])
        #expect(sankey.income == 270_000)
        #expect(sankey.expenses == 136_900)
        #expect(Self.value(sankey, CashFlowSankey.totalId) == 270_000)
        #expect(Self.value(sankey, CashFlowSankey.savingsId) == 133_100)
        #expect(sankey.node(CashFlowSankey.fromBalanceId) == nil)
        #expect(Self.titles(sankey, column: 0) == ["Stipendio", "Interessi", "Rimborsi spese"])
        #expect(Self.titles(sankey, column: 1) == [String(localized: "Income")])
    }

    @Test("The flows go heaviest first, Unallocated by its app name, then the savings; the refund nets its own pair")
    func flowsAndSavings() throws {
        let sankey = try #require(F.month)
        #expect(
            Self.titles(sankey, column: 2)
                == ["Casa", "Spesa", "Vacanze", NameBook.unallocatedLabel, String(localized: "Savings")]
        )
        #expect(sankey.nodes(in: 2).map(\.value) == [93_900, 25_000, 10_000, 8_000, 133_100])
        #expect(sankey.nodes(in: 2).map(\.role) == [.flow, .flow, .flow, .flow, .savings])
    }

    @Test("A category is one node whoever paid it, with one ribbon from each flow that did")
    func globalCategories() throws {
        let sankey = try #require(F.month)
        #expect(Self.titles(sankey, column: 3) == ["Mutuo", "Spesa", "Utenze", "Ristoranti", "Viaggi"])
        #expect(Self.value(sankey, "cat:Ristoranti") == 13_000)
        let paying = sankey.incoming("cat:Ristoranti").map { "\($0.from)=\($0.value)" }.sorted()
        #expect(paying == ["flow:Spesa=5000", "flow:Unallocated=8000"])
        // A category and an income category of the same name stay apart.
        #expect(sankey.node("cat:Spesa")?.role == .category)
    }

    @Test("Every share is of the income")
    func sharesOfIncome() throws {
        let sankey = try #require(F.month)
        #expect(sankey.share(27_000) == 0.1)
        #expect(sankey.shown(try #require(sankey.node(CashFlowSankey.totalId))) == 270_000)
    }

    @Test("Spending more than came in takes the rest from the balance, never with savings")
    func fromTheBalance() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 240_000), F.earned("Rimborsi spese", 12_000)],
                expenses: [
                    F.spent("Vacanze", "Viaggi", 150_000),
                    F.spent("Casa", "Casa", 60_000),
                    F.spent("Casa", "Utenze", 24_000),
                    F.spent("Unallocated", "Ristoranti", 65_860),
                    F.spent("Spesa", "Spesa", 35_000),
                ],
                flows: F.flows
            )
        )
        F.expectBalanced(sankey)
        #expect(sankey.node(CashFlowSankey.savingsId) == nil)
        let balance = try #require(sankey.node(CashFlowSankey.fromBalanceId))
        #expect(balance.value == 82_860)
        #expect(balance.column == 0)
        #expect(balance.title == String(localized: "From the balance"))
        // Last in its column, under the income.
        #expect(sankey.nodes(in: 0).map(\.role) == [.income, .income, .fromBalance])
        // Entrate lets through what was spent, and says what came in.
        let total = try #require(sankey.node(CashFlowSankey.totalId))
        #expect(total.value == 334_860)
        #expect(sankey.shown(total) == 252_000)
        #expect(sankey.share(balance.value).map { ($0 * 1000).rounded() } == 329)
    }

    @Test("Past five income categories and past eight expense categories the rest is one «Other» node, its ribbons summed per flow")
    func foldedColumns() throws {
        // More than the expenses, so no balance weighs on the column.
        let income = [100, 50, 40, 30, 20, 10, 5].enumerated().map { index, amount in
            F.earned("Entrata \(index + 1)", Int64(amount) * 100_000)
        }
        // Eleven categories, 11.000,00 down to 1.000,00, paid by two flows.
        let expenses = (1...11).flatMap { index -> [FlowCategoryTotals] in
            let amount = Int64(12 - index) * 100_000
            return [F.spent("Casa", "Categoria \(index)", amount * 3 / 4), F.spent("Spesa", "Categoria \(index)", amount / 4)]
        }
        let sankey = try #require(CashFlowSankey.build(income: income, expenses: expenses, flows: F.flows))
        F.expectBalanced(sankey)

        let other = String(localized: "Other")
        #expect(Self.titles(sankey, column: 0) == ["Entrata 1", "Entrata 2", "Entrata 3", "Entrata 4", "Entrata 5", other])
        #expect(Self.value(sankey, "in:other") == 1_500_000)
        #expect(sankey.node("in:other")?.isOther == true)

        #expect(Self.titles(sankey, column: 3) == (1...8).map { "Categoria \($0)" } + [other])
        // 3.000,00 + 2.000,00 + 1.000,00.
        #expect(Self.value(sankey, "cat:other") == 600_000)
        #expect(sankey.incoming("cat:other").map(\.value).sorted() == [150_000, 450_000])
    }

    @Test("A category under 1,5% of its column folds even within the limit, but a rest of one keeps its name")
    func lightCategories() throws {
        let two = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 500_000)],
                expenses: [
                    F.spent("Casa", "Mutuo", 100_000), F.spent("Spesa", "Spesa", 50_000),
                    F.spent("Spesa", "Edicola", 1_000), F.spent("Casa", "Francobolli", 1_000),
                ],
                flows: F.flows
            )
        )
        F.expectBalanced(two)
        #expect(Self.titles(two, column: 3) == ["Mutuo", "Spesa", String(localized: "Other")])
        #expect(Self.value(two, "cat:other") == 2_000)

        let one = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 500_000)],
                expenses: [F.spent("Casa", "Mutuo", 100_000), F.spent("Spesa", "Spesa", 50_000), F.spent("Spesa", "Edicola", 1_000)],
                flows: F.flows
            )
        )
        #expect(Self.titles(one, column: 3) == ["Mutuo", "Spesa", "Edicola"])
    }

    @Test("With one income category the income column would repeat Entrate: the graph starts there")
    func singleIncomeCategory() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [F.earned("Uncategorized", 100_000, system: true)],
                expenses: [F.spent("Casa", "Mutuo", 40_000)],
                flows: F.flows
            )
        )
        F.expectBalanced(sankey)
        #expect(sankey.columns == [1, 2, 3])
        #expect(sankey.incoming(CashFlowSankey.totalId).isEmpty)
        #expect(Self.value(sankey, CashFlowSankey.totalId) == 100_000)
        #expect(Self.value(sankey, CashFlowSankey.savingsId) == 60_000)
    }

    @Test("One income category with money from the balance keeps the column, the balance has to come from somewhere")
    func singleIncomeCategoryFromTheBalance() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [F.earned("Uncategorized", 100_000, system: true)],
                expenses: [F.spent("Casa", "Mutuo", 140_000)],
                flows: F.flows
            )
        )
        F.expectBalanced(sankey)
        #expect(Self.titles(sankey, column: 0) == [String(localized: "Uncategorized"), String(localized: "From the balance")])
        #expect(Self.value(sankey, CashFlowSankey.fromBalanceId) == 40_000)
    }

    @Test("With nothing earned the balance pays for everything, Entrate is as big as what went through and says 0, and nothing has a share")
    func noIncome() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [],
                expenses: [F.spent("Casa", "Mutuo", 40_000), F.spent("Spesa", "Spesa", 10_000)],
                flows: F.flows
            )
        )
        F.expectBalanced(sankey)
        #expect(sankey.nodes(in: 0).map(\.id) == [CashFlowSankey.fromBalanceId])
        #expect(Self.value(sankey, CashFlowSankey.fromBalanceId) == 50_000)
        let total = try #require(sankey.node(CashFlowSankey.totalId))
        #expect(total.value == 50_000)
        #expect(sankey.shown(total) == 0)
        #expect(sankey.income == 0)
        #expect(sankey.share(50_000) == nil)
    }

    @Test("With nothing spent everything earned is savings, and there is no category column")
    func noExpenses() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 100_000), F.earned("Interessi", 5_000)],
                expenses: [],
                flows: F.flows
            )
        )
        F.expectBalanced(sankey)
        #expect(sankey.columns == [0, 1, 2])
        #expect(sankey.nodes(in: 2).map(\.id) == [CashFlowSankey.savingsId])
        #expect(Self.value(sankey, CashFlowSankey.savingsId) == 105_000)
    }

    @Test("A refund larger than what one flow spent on a category leaves that ribbon out, and a flow with nothing left is not drawn")
    func refundAboveExpense() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 100_000)],
                expenses: [
                    F.spent("Casa", "Spesa", 10_000, refund: 15_000),
                    F.spent("Casa", "Utenze", 5_000),
                    F.spent("Vacanze", "Viaggi", 20_000, refund: 30_000),
                ],
                flows: F.flows
            )
        )
        F.expectBalanced(sankey)
        #expect(Self.value(sankey, "flow:Casa") == 5_000)
        #expect(sankey.node("flow:Vacanze") == nil)
        #expect(sankey.node("cat:Spesa") == nil)
        #expect(sankey.node("cat:Viaggi") == nil)
        // The refund does not lower the savings past what was spent.
        #expect(Self.value(sankey, CashFlowSankey.savingsId) == 95_000)
    }

    @Test("An archived flow that spent in the period keeps its name; one the vault no longer lists is a dash")
    func archivedFlow() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 100_000)],
                expenses: [F.spent("Vecchia", "Spesa", 3_000), F.spent("Sparita", "Spesa", 2_000)],
                flows: F.flows + [TestFlow.make("Vecchia", archived: true)]
            )
        )
        #expect(sankey.node("flow:Vecchia")?.title == "Vecchia")
        #expect(sankey.node("flow:Sparita")?.title == TransactionRow.placeholder)
    }

    @Test("A system category is named in the user's language, the folded rest is «Other»")
    func localizedNames() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 100_000), F.earned("uncategorized", 10_000, system: true)],
                expenses: [F.spent("Casa", "uncategorized", 3_000, system: true)],
                flows: F.flows
            )
        )
        #expect(sankey.node("in:uncategorized")?.title == String(localized: "Uncategorized"))
        #expect(sankey.node("cat:uncategorized")?.title == String(localized: "Uncategorized"))
    }

    @Test("Nothing earned and nothing spent is no graph, refunds that cancel the expenses included")
    func empty() {
        #expect(CashFlowSankey.build(income: [], expenses: [], flows: F.flows) == nil)
        let expenseOnly = CategoryTotals(
            categoryId: "Spesa", name: "Spesa", isSystem: false,
            income: 0, expense: 5_000, refund: 5_000, netExpense: 0, count: 2
        )
        #expect(
            CashFlowSankey.build(
                income: [expenseOnly], expenses: [F.spent("Casa", "Spesa", 5_000, refund: 5_000)], flows: F.flows
            ) == nil
        )
    }
}
