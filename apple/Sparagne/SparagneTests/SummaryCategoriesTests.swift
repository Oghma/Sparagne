import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The RIEPILOGO's categories of the year, from the core's totals as they
/// come back: heaviest net expense first.
struct SummaryCategoriesTests {
    static func totals(
        _ name: String,
        netExpense: Int64,
        count: UInt32 = 1,
        isSystem: Bool = false
    ) -> CategoryTotals {
        CategoryTotals(
            categoryId: name.lowercased(),
            name: name,
            isSystem: isSystem,
            income: 0,
            expense: max(netExpense, 0),
            refund: max(-netExpense, 0),
            netExpense: netExpense,
            count: count
        )
    }

    /// Nine categories that spent, heaviest first: 900,00 down to 100,00.
    static var nine: [CategoryTotals] {
        ["Casa", "Spesa", "Auto", "Bollette", "Salute", "Svago", "Viaggi", "Regali", "Libri"]
            .enumerated()
            .map { index, name in totals(name, netExpense: Int64(9 - index) * 10_000, count: UInt32(index + 1)) }
    }

    @Test("Fewer than nine categories are all lines of their own, with no other line")
    func fewerThanTheLimit() {
        let rows = SummaryCategories.rows([
            Self.totals("Casa", netExpense: 30_000, count: 2),
            Self.totals("Spesa", netExpense: 10_000, count: 5),
        ])
        #expect(rows.map(\.name) == ["Casa", "Spesa"])
        #expect(rows.map(\.categoryId) == ["casa", "spesa"])
        #expect(rows.map(\.amount) == [30_000, 10_000])
        #expect(rows.map(\.count) == [2, 5])
        #expect(rows.map(\.share) == [0.75, 0.25])
        #expect(!rows.contains { $0.isOther })
    }

    @Test("The ninth category folds into the other line, with its amount and its rows")
    func theNinthIsOther() throws {
        let rows = SummaryCategories.rows(Self.nine)
        #expect(rows.count == SummaryCategories.limit + 1)
        #expect(rows.prefix(8).map(\.name) == ["Casa", "Spesa", "Auto", "Bollette", "Salute", "Svago", "Viaggi", "Regali"])
        let other = try #require(rows.last)
        #expect(other.isOther)
        #expect(other.categoryId == nil)
        #expect(other.id == "other")
        #expect(other.name.isEmpty)
        #expect(!other.isSystem)
        #expect(other.amount == 10_000)
        #expect(other.count == 9)
        // 4.500,00 spent between them.
        #expect(other.share == 10_000.0 / 450_000.0)
    }

    @Test("The shares add up to the whole")
    func sharesAddUp() {
        let shares = SummaryCategories.rows(Self.nine + [Self.totals("Varie", netExpense: 3_333, count: 4)])
        let total = shares.reduce(0) { $0 + $1.share }
        #expect(abs(total - 1) < 1e-9)
        #expect(shares.last?.amount == 13_333)
        #expect(shares.last?.count == 13)
    }

    @Test("A category that spent nothing, or got back more than it spent, has no line and no weight")
    func nothingSpentIsDropped() {
        let rows = SummaryCategories.rows([
            Self.totals("Casa", netExpense: 40_000),
            Self.totals("Rimborsi", netExpense: 0, count: 3),
            Self.totals("Resi", netExpense: -5_000, count: 2),
        ])
        #expect(rows.map(\.name) == ["Casa"])
        // The refund lowers nobody's figure, so it does not lower the base.
        #expect(rows.first?.share == 1)
    }

    @Test("A system category keeps its flag, for the view to name it")
    func systemCategory() {
        let rows = SummaryCategories.rows([
            Self.totals("Casa", netExpense: 40_000),
            Self.totals("Uncategorized", netExpense: 10_000, isSystem: true),
        ])
        #expect(rows.map(\.isSystem) == [false, true])
        #expect(rows.last?.name == "Uncategorized")
    }

    @Test("A year with nothing spent has no lines")
    func empty() {
        #expect(SummaryCategories.rows([]).isEmpty)
        #expect(SummaryCategories.rows([Self.totals("Rimborsi", netExpense: -100)]).isEmpty)
    }
}
