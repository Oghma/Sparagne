import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The months x categories grid: the year's category lines, each month's net
/// expense in its cell.
struct CategoryHeatTests {
    private static let september = MonthKey(year: 2026, month: 9)

    private static func bucket(_ bucket: UInt32, _ name: String, _ netExpense: Int64) -> BucketCategoryTotals {
        BucketCategoryTotals(
            bucket: bucket, categoryId: name.lowercased(), name: name, isSystem: false, netExpense: netExpense, count: 1
        )
    }

    @Test("Each month's net expense lands in its own cell, January first")
    func cellPlacement() throws {
        let shares = SummaryCategories.rows([
            SummaryCategoriesTests.totals("Casa", netExpense: 150_000),
            SummaryCategoriesTests.totals("Spesa", netExpense: 30_000),
        ])
        let heat = CategoryHeat.build(
            buckets: [
                Self.bucket(1, "Casa", 100_000),
                Self.bucket(3, "Casa", 50_000),
                Self.bucket(3, "Spesa", 30_000),
                // Before January, and after the month on screen: no cell.
                Self.bucket(0, "Casa", 999_999),
                Self.bucket(10, "Spesa", 70_000),
                Self.bucket(13, "Spesa", 70_000),
            ],
            shares: shares,
            upTo: Self.september
        )
        #expect(heat.elapsedMonths == 9)
        #expect(heat.rows.map(\.name) == ["Casa", "Spesa"])
        let casa = try #require(heat.rows.first)
        #expect(casa.cells.count == 12)
        #expect(casa.cells == [100_000, 0, 50_000, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        #expect(heat.rows[1].cells == [0, 0, 30_000, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        #expect(heat.rows.map(\.id) == ["casa", "spesa"])
    }

    @Test("The other line takes every category the list does not name")
    func otherLine() throws {
        let shares = SummaryCategories.rows(SummaryCategoriesTests.nine + [SummaryCategoriesTests.totals("Varie", netExpense: 5_000)])
        let heat = CategoryHeat.build(
            buckets: [
                Self.bucket(2, "Casa", 90_000),
                Self.bucket(2, "Libri", 10_000),
                Self.bucket(2, "Varie", 5_000),
                Self.bucket(4, "Varie", 1_000),
            ],
            shares: shares,
            upTo: Self.september
        )
        #expect(heat.rows.count == 9)
        let other = try #require(heat.rows.last)
        #expect(other.isOther)
        #expect(other.categoryId == nil)
        #expect(other.cells[1] == 15_000)
        #expect(other.cells[3] == 1_000)
        #expect(heat.rows[0].cells[1] == 90_000)
        // A named line with nothing in the grid is a line of zeros.
        #expect(heat.rows[1].cells.allSatisfy { $0 == 0 })
    }

    @Test("Every shade is measured on the heaviest cell")
    func maximumAndIntensity() {
        let shares = SummaryCategories.rows([
            SummaryCategoriesTests.totals("Casa", netExpense: 150_000),
            SummaryCategoriesTests.totals("Spesa", netExpense: 30_000),
        ])
        let heat = CategoryHeat.build(
            buckets: [Self.bucket(1, "Casa", 100_000), Self.bucket(3, "Casa", 50_000), Self.bucket(3, "Spesa", 30_000)],
            shares: shares,
            upTo: Self.september
        )
        #expect(heat.maximum == 100_000)
        #expect(heat.intensity(100_000) == 1)
        #expect(heat.intensity(50_000) == 0.5)
        #expect(heat.intensity(0) == 0)
        // Clamped either way.
        #expect(heat.intensity(250_000) == 1)
        #expect(heat.intensity(-10) == 0)
    }

    @Test("An empty grid has no lines, no maximum and no shade")
    func emptyGrid() {
        let heat = CategoryHeat.build(buckets: [], shares: [], upTo: Self.september)
        #expect(heat.rows.isEmpty)
        #expect(heat.maximum == 0)
        #expect(heat.intensity(1_000) == 0)
        // Lines with nothing in their months shade nothing either.
        let quiet = CategoryHeat.build(
            buckets: [Self.bucket(11, "Casa", 1_000)],
            shares: SummaryCategories.rows([SummaryCategoriesTests.totals("Casa", netExpense: 1_000)]),
            upTo: Self.september
        )
        #expect(quiet.maximum == 0)
        #expect(quiet.intensity(1_000) == 0)
    }
}
