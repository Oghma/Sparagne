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
