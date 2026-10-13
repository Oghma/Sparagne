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

    @Test("TASSO reads as the savings-rate card reads the same two figures")
    func rateMatchesTheCard() {
        let figures: [(savings: Int64, income: Int64)] = [(731, 1_000), (-260, 1_000), (0, 500), (-1_000, 1_000), (1_000, 1_000)]
        for (savings, income) in figures {
            #expect(SummaryText.rate(Double(savings) / Double(income)) == LedgerMoney.savingsRate(savings, of: income))
        }
    }

    @Test("Past -100% a rate only says how little came in, as the card says it")
    func rateBelowMinusOneHundredIsClamped() {
        #expect(SummaryText.rate(-3_945) == "< -100%")
        #expect(SummaryText.rate(-1.0001) == "< -100%")
        #expect(SummaryText.rate(-1) == "-100%")
    }

    @Test func noRateIsNilForTheCallerToDash() {
        #expect(SummaryText.rate(nil) == nil)
    }
}
