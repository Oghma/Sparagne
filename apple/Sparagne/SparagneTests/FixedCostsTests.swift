import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The fixed costs card: what the recurring expenses commit a month, against
/// the income of the year's finished months.
struct FixedCostsTests {
    private static let april = MonthKey(year: 2026, month: 4)

    /// 4.000, 2.000 and 3.000 in the first three months, 1.000 so far in
    /// April.
    private static func year(today: MonthKey?, upTo: MonthKey = april) -> YearSummary {
        YearSummary.build(
            year: 2026,
            upTo: upTo,
            rows: [
                YearRow(bucket: 1, person: "matteo", income: 400_000, opening: 0, cashExpense: 0, fundExpense: 0),
                YearRow(bucket: 2, person: "matteo", income: 200_000, opening: 0, cashExpense: 0, fundExpense: 0),
                YearRow(bucket: 3, person: "matteo", income: 300_000, opening: 0, cashExpense: 0, fundExpense: 0),
                YearRow(bucket: 4, person: "matteo", income: 100_000, opening: 0, cashExpense: 0, fundExpense: 0),
            ],
            flows: [],
            today: today
        )
    }

    /// The mortgage, plus what the card must not count: an archived template,
    /// a paused one and an income.
    private static var templates: [RecurringView] {
        [
            RecurringFixture.template(amount: 100_000),
            RecurringFixture.template(amount: 50_000, archived: true),
            RecurringFixture.template(amount: 30_000, enabled: false),
            RecurringFixture.template(kind: .income, amount: 200_000),
        ]
    }

    @Test("The month's commitment is the enabled expense templates at their monthly equivalent")
    func monthly() throws {
        let costs = try #require(FixedCosts.build(templates: Self.templates, year: Self.year(today: Self.april)))
        #expect(costs.monthly == 100_000)
        // A weekly expense is spread over the average month, as the
        // Ricorrenze status line counts it.
        let weekly = try #require(
            FixedCosts.build(
                templates: [RecurringFixture.template(amount: 1_000, schedule: RecurringFixture.schedule(.weekly(weekday: 1)))],
                year: Self.year(today: Self.april)
            )
        )
        #expect(weekly.monthly == RecurringMonthly.totals([
            RecurringFixture.template(amount: 1_000, schedule: RecurringFixture.schedule(.weekly(weekday: 1))),
        ]).expenses)
        #expect(weekly.monthly == 4_348)
    }

    @Test("The share is on the average income of the finished months, the live one left out")
    func shareOfTheFinishedMonths() throws {
        let live = try #require(FixedCosts.build(templates: Self.templates, year: Self.year(today: Self.april)))
        // 9.000 over three months.
        #expect(live.shareOfIncome == 100_000.0 / 300_000.0)
        // April gone by is a finished month too: 10.000 over four.
        let past = try #require(
            FixedCosts.build(templates: Self.templates, year: Self.year(today: MonthKey(year: 2026, month: 5)))
        )
        #expect(past.shareOfIncome == 0.4)
    }

    @Test("Without a finished month or an income to measure on, there is no share")
    func noShare() throws {
        let january = MonthKey(year: 2026, month: 1)
        let live = try #require(
            FixedCosts.build(templates: Self.templates, year: Self.year(today: january, upTo: january))
        )
        #expect(live.monthly == 100_000)
        #expect(live.shareOfIncome == nil)

        let noIncome = YearSummary.build(year: 2026, upTo: Self.april, rows: [], flows: [])
        let costs = try #require(FixedCosts.build(templates: Self.templates, year: noIncome))
        #expect(costs.shareOfIncome == nil)
    }

    @Test("Without an enabled expense template there is no card figure at all")
    func noExpenseTemplate() {
        let year = Self.year(today: Self.april)
        #expect(FixedCosts.build(templates: [], year: year) == nil)
        #expect(FixedCosts.build(templates: Array(Self.templates.dropFirst()), year: year) == nil)
    }

    // MARK: - PERSONA

    /// Elisa earns 1.500 a month and Matteo 2.500, January to March; the rent
    /// is Matteo's, the gym Elisa's.
    private static func household(person: String?) -> YearSummary {
        let rows = (1...3).flatMap { month in
            [
                YearRow(bucket: month, person: "elisa", income: 150_000, opening: 0, cashExpense: 0, fundExpense: 0),
                YearRow(bucket: month, person: "matteo", income: 250_000, opening: 0, cashExpense: 0, fundExpense: 0),
            ]
        }
        return YearSummary.build(year: 2026, upTo: april, rows: rows, flows: [], person: person, today: april)
    }

    private static var householdTemplates: [RecurringView] {
        [
            RecurringFixture.template(amount: 100_000, note: "affitto", owner: "matteo"),
            RecurringFixture.template(amount: 15_000, note: "palestra", owner: "elisa"),
        ]
    }

    @Test("Everybody's fixed costs are measured on everybody's income")
    func everybody() throws {
        let costs = try #require(FixedCosts.build(templates: Self.householdTemplates, year: Self.household(person: nil)))
        #expect(costs.monthly == 115_000)
        #expect(costs.shareOfIncome == 115_000.0 / 400_000.0)
    }

    @Test("Under PERSONA only the person's own templates are measured on the person's income")
    func underPersona() throws {
        let elisa = try #require(FixedCosts.build(templates: Self.householdTemplates, year: Self.household(person: "elisa")))
        #expect(elisa.monthly == 15_000)
        #expect(elisa.shareOfIncome == 0.1)
        let matteo = try #require(FixedCosts.build(templates: Self.householdTemplates, year: Self.household(person: "matteo")))
        #expect(matteo.monthly == 100_000)
        #expect(matteo.shareOfIncome == 0.4)
        // Somebody with no template of their own has no fixed costs.
        #expect(FixedCosts.build(templates: Self.householdTemplates, year: Self.household(person: "giulia")) == nil)
    }
}
