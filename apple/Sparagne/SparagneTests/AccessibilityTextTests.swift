import Foundation
import Testing

@testable import Sparagne

/// `AccessibilityText` is pure string assembly (`Views/Summary/AccessibilityText.swift`),
/// so it is checked here without a window.
struct AccessibilityTextTests {
    @Test func cardJoinsTitleValueAndCaption() {
        let label = AccessibilityText.card(title: "Income · September", value: "€1,234.56", caption: "2 people")
        #expect(label == "Income · September, €1,234.56, 2 people")
    }

    @Test func cardDropsANilCaption() {
        let label = AccessibilityText.card(title: "Rate · September", value: "42%", caption: nil)
        #expect(label == "Rate · September, 42%")
    }

    @Test func fundGaugeReadsPercentThenFilledOfCap() {
        let value = AccessibilityText.fundGauge(percent: "99.8%", filled: "€29,931.00", cap: "€30,000.00")
        #expect(value == "99.8%, €29,931.00 of €30,000.00")
    }

    @Test func monthRowJoinsTheMonthAndEveryFigure() {
        let label = AccessibilityText.monthRow(
            month: "September",
            figures: [("Income", "€9,200.00"), ("Expenses", "€3,100.00")]
        )
        #expect(label == "September, Income €9,200.00, Expenses €3,100.00")
    }

    @Test func monthRowWithNoFiguresIsJustTheMonth() {
        #expect(AccessibilityText.monthRow(month: "October", figures: []) == "October")
    }

    @Test func figureWithoutAPersonIsLabelThenAmount() {
        #expect(AccessibilityText.figure("Income", amount: "€1,200.00") == "Income: €1,200.00")
    }

    @Test func figureWithAPersonNamesItBetweenLabelAndAmount() {
        let label = AccessibilityText.figure("Income", person: "Elisa", amount: "€1,200.00")
        #expect(label == "Income, Elisa: €1,200.00")
    }
}

/// `SummaryText` is the RIEPILOGO's small pure text
/// (`Views/Summary/SummaryText.swift`), checked the same way.
struct SummaryTextTests {
    @Test("A share is written with the ledger's comma, without decimals unless asked")
    func percentOfAFraction() {
        #expect(SummaryText.percent(0.38) == "38%")
        #expect(SummaryText.percent(0) == "0%")
        #expect(SummaryText.percent(0.125, decimals: 1) == "12,5%")
    }

    @Test("The two system categories are named in the user's language, whatever the core's case; the vault's own keep their name, even one called like a system category")
    func systemCategoriesAreLocalized() {
        #expect(SummaryText.categoryName("spesa", isSystem: false) == "spesa")
        #expect(SummaryText.categoryName("Uncategorized", isSystem: false) == "Uncategorized")
        #expect(SummaryText.categoryName("uncategorized", isSystem: true) == String(localized: "Uncategorized"))
        #expect(SummaryText.categoryName("Opening", isSystem: true) == String(localized: "Opening"))
        #expect(SummaryText.categoryName("Something new", isSystem: true) == "Something new")
    }

    @Test func theFoldedLineIsOtherWhateverItsName() {
        #expect(SummaryText.categoryName("", isSystem: false, isOther: true) == String(localized: "Other"))
    }

    // MARK: - The strip of things to do

    private static let english = Locale(identifier: "en_US")
    private static let italian = Locale(identifier: "it_IT")

    @Test("A chip's count reads as one or many, never \"1 rows\"")
    func englishChips() throws {
        let catalog = try SummaryCatalog.english()
        func chip(_ kind: SummaryTodoItem.Kind) -> String {
            SummaryText.todo(kind, bundle: catalog, locale: Self.english)
        }
        #expect(chip(.dueRecurring(count: 1)) == "1 recurring to confirm")
        #expect(chip(.dueRecurring(count: 3)) == "3 recurring to confirm")
        #expect(chip(.uncategorized(count: 1)) == "1 row without a category")
        #expect(chip(.uncategorized(count: 14)) == "14 rows without a category")
    }

    @Test("The allocation's period is written as the due card writes it, the older ones after it")
    func allocationChip() throws {
        let catalog = try SummaryCatalog.english()
        let day = RecurringDayText.weekday("2026-09-27", locale: Self.english)
        let due = SummaryTodoItem.Kind.pendingAllocation(period: "2026-09-27", missed: 0)
        let behind = SummaryTodoItem.Kind.pendingAllocation(period: "2026-09-27", missed: 2)
        #expect(SummaryText.todo(due, bundle: catalog, locale: Self.english) == "Allocation due \u{00B7} \(day)")
        #expect(
            SummaryText.todo(behind, bundle: catalog, locale: Self.english)
                == "Allocation due \u{00B7} \(day) \u{00B7} 2 earlier"
        )
    }

    @Test("The same chips in Italian")
    func italianChips() throws {
        let catalog = try SummaryCatalog.italian()
        let day = RecurringDayText.weekday("2026-09-27", locale: Self.italian)
        func chip(_ kind: SummaryTodoItem.Kind) -> String {
            SummaryText.todo(kind, bundle: catalog, locale: Self.italian)
        }
        #expect(chip(.dueRecurring(count: 1)) == "1 ricorrenza da confermare")
        #expect(chip(.dueRecurring(count: 2)) == "2 ricorrenze da confermare")
        #expect(chip(.uncategorized(count: 1)) == "1 riga senza categoria")
        #expect(
            chip(.pendingAllocation(period: "2026-09-27", missed: 1))
                == "Riparto in attesa \u{00B7} \(day) \u{00B7} 1 precedente"
        )
    }
}

/// The strip's keys in a catalog of their own (`CatalogBundle`).
private enum SummaryCatalog {
    static func english() throws -> Bundle {
        try CatalogBundle.make(
            language: "en",
            plurals: [
                "%lld recurring to confirm": ("%lld recurring to confirm", "%lld recurring to confirm"),
                "%lld rows without a category": ("%lld row without a category", "%lld rows without a category"),
                "%lld earlier": ("%lld earlier", "%lld earlier"),
            ],
            name: "summary"
        )
    }

    static func italian() throws -> Bundle {
        try CatalogBundle.make(
            language: "it",
            plurals: [
                "%lld recurring to confirm": ("%lld ricorrenza da confermare", "%lld ricorrenze da confermare"),
                "%lld rows without a category": ("%lld riga senza categoria", "%lld righe senza categoria"),
                "%lld earlier": ("%lld precedente", "%lld precedenti"),
            ],
            strings: ["Allocation due \u{00B7} %@": "Riparto in attesa \u{00B7} %@"],
            name: "summary"
        )
    }
}
