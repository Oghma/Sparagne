import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The cash-flow Sankey as one load of the store fills it, from the core's
/// real rows: the month on screen and the year so far, under PERSONA too.
/// The graph's own rules are `CashFlowSankeyTests`'; this checks the rows
/// reach it.
struct CashFlowSankeyStoreTests {
    private static let february = MonthKey(year: 2025, month: 2)
    private static let march = MonthKey(year: 2025, month: 3)
    private static let april = MonthKey(year: 2025, month: 4)

    private static func day(_ month: MonthKey, _ day: Int) throws -> Date {
        try #require(Calendar.current.date(from: DateComponents(year: month.year, month: month.month, day: day)))
    }

    /// Vault `Main`, envelopes `Casa` and `Spesa`, on screen March 2025.
    /// February: a bonus of 500,00 for matteo. March: matteo's salary of
    /// 2.000,00 and elisa's 10,00 of interest, both into Unallocated; Casa
    /// pays matteo's Mutuo 800,00 and elisa's Spesa 100,00, the envelope
    /// Spesa matteo's Spesa 300,00. April: only a move of 200,00 from
    /// Unallocated to Spesa.
    private static func household() async throws -> AppStore {
        let store = try await TestStore.make(vault: "Main", wallet: "Conto", opening: 100_000, author: "matteo")
        await store.createEnvelope(name: "Casa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        await store.createEnvelope(name: "Spesa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let unallocated = try TestStore.unallocated(in: store).id
        let casa = try TestStore.flow("Casa", in: store).id
        let spesa = try TestStore.flow("Spesa", in: store).id
        store.month = march

        func income(_ day: Date, _ category: String, _ amount: Int64, person: String? = nil) async {
            await store.addRow(
                day: day, flowId: unallocated, category: category, note: category.lowercased(), amount: amount,
                kind: .income, person: person, typedIn: .income
            )
        }
        func expense(_ day: Date, _ flow: Uuid, _ category: String, _ amount: Int64, person: String? = nil) async {
            await store.addRow(
                day: day, flowId: flow, category: category, note: category.lowercased(), amount: amount,
                kind: .expense, person: person, typedIn: .expenses
            )
        }
        try await income(day(february, 10), "Bonus", 50_000)
        try await income(day(march, 10), "Stipendio", 200_000)
        try await income(day(march, 11), "Interessi", 1_000, person: "elisa")
        try await expense(day(march, 12), casa, "Mutuo", 80_000)
        try await expense(day(march, 13), casa, "Spesa", 10_000, person: "elisa")
        try await expense(day(march, 14), spesa, "Spesa", 30_000)

        var move = MoveDraft(sourceId: unallocated, day: "2025-04-10")
        move.add(spesa)
        move.setAmount("200", for: spesa)
        #expect(await store.move(move))
        await store.settle()
        #expect(store.presentedError == nil)
        return store
    }

    private static func titles(_ sankey: CashFlowSankey, column: Int) -> [String] {
        sankey.nodes(in: column).map(\.title)
    }

    private static func node(_ sankey: CashFlowSankey, column: Int, _ title: String) throws -> CashFlowSankey.Node {
        try #require(sankey.nodes(in: column).first { $0.title == title })
    }

    @Test("One load fills the month and the year: income by category, what each flow spent, and a category paid by two flows as one node")
    func oneLoadFillsBoth() async throws {
        let store = try await Self.household()

        let month = try #require(store.sankeyMonth)
        #expect(month.income == 201_000)
        #expect(Self.titles(month, column: 0) == ["Stipendio", "Interessi"])
        #expect(Self.titles(month, column: 2) == ["Casa", "Spesa", String(localized: "Savings")])
        #expect(month.nodes(in: 2).map(\.value) == [90_000, 30_000, 81_000])
        #expect(Self.titles(month, column: 3) == ["Mutuo", "Spesa"])
        let groceries = try Self.node(month, column: 3, "Spesa")
        #expect(groceries.value == 40_000)
        #expect(month.incoming(groceries.id).map(\.value).sorted() == [10_000, 30_000])

        // January to March: February's bonus comes in too.
        let year = try #require(store.sankeyYear)
        #expect(year.income == 251_000)
        #expect(Self.titles(year, column: 0) == ["Stipendio", "Bonus", "Interessi"])
        #expect(year.node(CashFlowSankey.savingsId)?.value == 131_000)
    }

    @Test("PERSONA narrows both to that person's rows: elisa's interest does not cover her expense, so the rest comes from the balance")
    func personNarrowsBoth() async throws {
        let store = try await Self.household()
        store.person = "elisa"
        await store.settle()

        for sankey in [try #require(store.sankeyMonth), try #require(store.sankeyYear)] {
            #expect(sankey.income == 1_000)
            // One income category and the balance: the column stays.
            #expect(Self.titles(sankey, column: 0) == ["Interessi", String(localized: "From the balance")])
            #expect(sankey.node(CashFlowSankey.fromBalanceId)?.value == 9_000)
            #expect(Self.titles(sankey, column: 2) == ["Casa"])
            #expect(Self.titles(sankey, column: 3) == ["Spesa"])
            #expect(sankey.node(CashFlowSankey.savingsId) == nil)
        }
    }

    @Test("A month with only a move between flows has no graph, and the move is in no flow's spending for the year")
    func transfersOnly() async throws {
        let store = try await Self.household()
        store.month = Self.april
        await store.settle()

        #expect(store.sankeyMonth == nil)
        let year = try #require(store.sankeyYear)
        #expect(try Self.node(year, column: 2, "Spesa").value == 30_000)
        #expect(year.income == 251_000)
    }
}
