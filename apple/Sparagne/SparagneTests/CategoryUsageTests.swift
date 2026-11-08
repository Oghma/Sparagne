import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The usage column and the envelope bars of the setup tab:
/// both are plain functions, tested without a view.
struct CategoryUsageTests {
    @Test("The window is the 12 months up to now")
    func windowBounds() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try #require(TimeZone(identifier: "UTC"))
        let now = try #require(calendar.date(from: DateComponents(year: 2026, month: 10, day: 7, hour: 12)))
        let window = CategoryUsage.window(endingAt: now, calendar: calendar)
        #expect(window.end == now)
        #expect(window.start == calendar.date(from: DateComponents(year: 2025, month: 10, day: 7, hour: 12)))
    }

    @Test("A window ending on 29 February starts on the 28th a year earlier")
    func windowFromLeapDay() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try #require(TimeZone(identifier: "UTC"))
        let now = try #require(calendar.date(from: DateComponents(year: 2028, month: 2, day: 29, hour: 9)))
        let window = CategoryUsage.window(endingAt: now, calendar: calendar)
        #expect(window.start == calendar.date(from: DateComponents(year: 2027, month: 2, day: 28, hour: 9)))
    }

    @Test("Counts map to the category ids; a category the core omits has none")
    func mapping() {
        let counts = CategoryUsage.counts(from: [
            SummaryCategoriesTests.totals("a", netExpense: 0, count: 38),
            SummaryCategoriesTests.totals("b", netExpense: 0, count: 1),
        ])
        #expect(counts == ["a": 38, "b": 1])
        #expect(counts["c"] == nil)
        #expect(CategoryUsage.counts(from: []).isEmpty)
    }

    @Test("A net cap fills with the balance")
    func netCap() throws {
        let fill = FundGauge(id: "", name: "", mode: .netCapped(cap: 300_000), balance: 124_000, incomeTotal: 900_000)?.fraction
        #expect(abs(try #require(fill) - 124_000.0 / 300_000.0) < 1e-9)
    }

    @Test("An income cap fills with the cumulative income, or the balance without one")
    func incomeCap() throws {
        let fill = FundGauge(id: "", name: "", mode: .incomeCapped(cap: 1_000_000), balance: 680_000, incomeTotal: 250_000)?.fraction
        #expect(abs(try #require(fill) - 0.25) < 1e-9)
        let fallback = FundGauge(id: "", name: "", mode: .incomeCapped(cap: 1_000_000), balance: 680_000, incomeTotal: nil)?.fraction
        #expect(abs(try #require(fallback) - 0.68) < 1e-9)
    }

    @Test("Over the cap the bar stops at full; overdrawn stops at empty")
    func clamps() {
        #expect(FundGauge(id: "", name: "", mode: .netCapped(cap: 100), balance: 250, incomeTotal: nil)?.fraction == 1)
        #expect(FundGauge(id: "", name: "", mode: .netCapped(cap: 100), balance: -50, incomeTotal: nil)?.fraction == 0)
    }

    @Test("No cap, no bar")
    func noCap() {
        #expect(FundGauge(id: "", name: "", mode: .unlimited, balance: 100, incomeTotal: nil)?.fraction == nil)
    }
}
