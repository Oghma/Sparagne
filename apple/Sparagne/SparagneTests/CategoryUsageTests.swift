import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The usage column and the envelope bars of the setup tab:
/// both are plain functions, tested without a view.
struct CategoryUsageTests {
    private static func totals(_ id: String, count: UInt32) -> CategoryTotals {
        CategoryTotals(
            categoryId: id, name: id, isSystem: false,
            income: 0, expense: 0, refund: 0, netExpense: 0, count: count
        )
    }

    @Test("The window is the 90 days up to now")
    func windowBounds() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try #require(TimeZone(identifier: "UTC"))
        let now = try #require(calendar.date(from: DateComponents(year: 2026, month: 10, day: 7, hour: 12)))
        let window = CategoryUsage.window(endingAt: now, calendar: calendar)
        #expect(window.end == now)
        #expect(window.start == calendar.date(from: DateComponents(year: 2026, month: 7, day: 9, hour: 12)))
        #expect(window.end.timeIntervalSince(window.start) == 90 * 86_400)
    }

    @Test("Counts map to the category ids; a category the core omits has none")
    func mapping() {
        let counts = CategoryUsage.counts(from: [Self.totals("a", count: 38), Self.totals("b", count: 1)])
        #expect(counts == ["a": 38, "b": 1])
        #expect(counts["c"] == nil)
        #expect(CategoryUsage.counts(from: []).isEmpty)
    }

    @Test("A net cap fills with the balance")
    func netCap() throws {
        let fill = EnvelopeCapFill.fraction(mode: .netCapped(cap: 300_000), balance: 124_000, incomeTotal: 900_000)
        #expect(abs(try #require(fill) - 124_000.0 / 300_000.0) < 1e-9)
    }

    @Test("An income cap fills with the cumulative income, or the balance without one")
    func incomeCap() throws {
        let fill = EnvelopeCapFill.fraction(mode: .incomeCapped(cap: 1_000_000), balance: 680_000, incomeTotal: 250_000)
        #expect(abs(try #require(fill) - 0.25) < 1e-9)
        let fallback = EnvelopeCapFill.fraction(mode: .incomeCapped(cap: 1_000_000), balance: 680_000, incomeTotal: nil)
        #expect(abs(try #require(fallback) - 0.68) < 1e-9)
    }

    @Test("Over the cap the bar stops at full; overdrawn stops at empty")
    func clamps() {
        #expect(EnvelopeCapFill.fraction(mode: .netCapped(cap: 100), balance: 250, incomeTotal: nil) == 1)
        #expect(EnvelopeCapFill.fraction(mode: .netCapped(cap: 100), balance: -50, incomeTotal: nil) == 0)
    }

    @Test("No cap, no bar")
    func noCap() {
        #expect(EnvelopeCapFill.fraction(mode: .unlimited, balance: 100, incomeTotal: nil) == nil)
    }
}
