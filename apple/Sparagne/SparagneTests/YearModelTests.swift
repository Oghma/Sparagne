import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The RIEPILOGO's arithmetic (`docs/v2/UI.md` §2.2). `YearSummary.build` is
/// pure, so the year is checked here without a database: the query that feeds
/// it is tested in the core.
struct YearModelTests {
    private static let upTo = MonthKey(year: 2026, month: 9)

    private static func row(
        _ bucket: Int,
        _ person: String,
        income: Int64 = 0,
        opening: Int64 = 0,
        cashExpense: Int64 = 0,
        fundExpense: Int64 = 0
    ) -> YearRow {
        YearRow(
            bucket: bucket,
            person: person,
            income: income,
            opening: opening,
            cashExpense: cashExpense,
            fundExpense: fundExpense
        )
    }

    /// Two people with money already in the vault before January, a January
    /// that earns and spends on both kinds of envelope, a wallet opened in
    /// March and a quiet September.
    private static var household: [YearRow] {
        [
            row(0, "matteo", income: 100_000, opening: 500_000, cashExpense: 40_000, fundExpense: 10_000),
            row(0, "elisa", opening: 200_000),
            row(1, "elisa", income: 200_000, cashExpense: 50_000),
            row(1, "matteo", income: 300_000, cashExpense: 95_000, fundExpense: 20_000),
            row(3, "matteo", income: 300_000, opening: 100_000, cashExpense: 100_000),
            row(9, "elisa", income: 100_000, fundExpense: 5_000),
        ]
    }

    private static func build(year: Int = 2026, rows: [YearRow], flows: [FlowView] = []) -> YearSummary {
        YearSummary.build(year: year, upTo: upTo, rows: rows, flows: flows)
    }

    // MARK: - People and the opening cash fund

    @Test("The columns are the vault's people, case-insensitively ordered")
    func peopleAreColumns() {
        let year = Self.build(rows: Self.household)
        #expect(year.people == ["elisa", "matteo"])
    }

    @Test("FONDO CASSA INIZIALE is everything that happened before January")
    func initialIsBucketZero() {
        let year = Self.build(rows: Self.household)
        // matteo: 100.000 + 500.000 - 40.000 - 10.000; elisa: her opening.
        #expect(year.initialByPerson == [200_000, 550_000])
        #expect(year.initial == 750_000)
    }

    // MARK: - The months

    @Test("The year is always twelve months, January first")
    func twelveMonths() {
        let year = Self.build(rows: Self.household)
        #expect(year.months.count == 12)
        #expect(year.months.map(\.month.month) == Array(1...12))
        let sameYear = year.months.allSatisfy { $0.month.year == 2026 }
        #expect(sameYear)
    }

    @Test("January carries the initial fund, and a month with no rows carries the one before")
    func carriedIsTheRunningBalance() {
        let year = Self.build(rows: Self.household)
        #expect(year.months[0].carried == 750_000)
        #expect(year.months[0].total == 1_085_000)
        // February has no rows at all: it repeats January's total.
        #expect(year.months[1].carried == 1_085_000)
        #expect(year.months[1].total == 1_085_000)
    }

    @Test("A wallet opened in March lands on FONDO CASSA, not on ENTRATE")
    func openingIsNotIncome() {
        let march = Self.build(rows: Self.household).months[2]
        #expect(march.income == 300_000)
        #expect(march.carried == 1_185_000)
        #expect(march.total == 1_385_000)
    }

    @Test("USCITE and USCITE FONDI split by the envelope's cap, and RISPARMIO ignores the funds")
    func cashAndFundExpensesAreSeparate() {
        let january = Self.build(rows: Self.household).months[0]
        #expect(january.income == 500_000)
        #expect(january.cashExpense == 145_000)
        #expect(january.fundExpense == 20_000)
        #expect(january.savings == 355_000)
    }

    @Test("TOTALE is RISPARMIO plus FONDO CASSA less USCITE FONDI, every month")
    func totalIdentityHolds() {
        let year = Self.build(rows: Self.household)
        for month in year.months {
            #expect(month.total == month.savings + month.carried - month.fundExpense)
        }
    }

    @Test("The person columns add up to TOTALE, every month")
    func personColumnsSumToTheTotal() {
        let year = Self.build(rows: Self.household)
        for month in year.months {
            #expect(month.totalByPerson.count == year.people.count)
            #expect(month.totalByPerson.reduce(0, +) == month.total)
        }
        // elisa first: the columns follow `people`.
        #expect(year.months[0].totalByPerson == [350_000, 735_000])
        #expect(year.months[2].totalByPerson == [350_000, 1_035_000])
    }

    @Test("A vault with one person has one column and no total to repeat")
    func singlePerson() {
        let year = Self.build(
            rows: [
                Self.row(0, "matteo", opening: 100_000),
                Self.row(1, "matteo", income: 50_000, cashExpense: 20_000),
            ]
        )
        #expect(year.people == ["matteo"])
        #expect(year.initialByPerson == [100_000])
        #expect(year.months[0].total == 130_000)
        #expect(year.months[0].totalByPerson == [130_000])
        #expect(year.months[11].total == 130_000)
    }

    // MARK: - Future months

    @Test("In the year on screen the months after the one on screen are blank")
    func futureMonthsOfTheCurrentYear() {
        let year = Self.build(rows: Self.household)
        #expect(year.months.filter(\.isFuture).map(\.month.month) == [10, 11, 12])
    }

    @Test("A past year is full and a year ahead is entirely blank")
    func pastAndComingYears() {
        let past = Self.build(year: 2025, rows: Self.household).months.allSatisfy { !$0.isFuture }
        let coming = Self.build(year: 2027, rows: Self.household).months.allSatisfy(\.isFuture)
        #expect(past)
        #expect(coming)
    }

    // MARK: - Gauges

    private static func flow(
        _ name: String,
        balance: Int64,
        mode: FlowMode,
        incomeTotal: Int64? = nil,
        archived: Bool = false
    ) -> FlowView {
        FlowView(
            id: name,
            name: name,
            balance: balance,
            mode: mode,
            incomeTotal: incomeTotal,
            allowNegative: true,
            archived: archived,
            isUnallocated: false
        )
    }

    @Test("A gauge is an active envelope with a cap, in the vault's order")
    func gaugesAreTheCappedEnvelopes() {
        let year = Self.build(
            rows: Self.household,
            flows: [
                Self.flow("Cash", balance: 10_000, mode: .unlimited),
                Self.flow("Emergenza", balance: 2_993_100, mode: .netCapped(cap: 3_000_000)),
                Self.flow("Casa", balance: 500, mode: .incomeCapped(cap: 15_000_000), incomeTotal: 3_193_500),
                Self.flow("Vecchio", balance: 0, mode: .netCapped(cap: 1_000), archived: true),
            ]
        )
        #expect(year.funds.map(\.name) == ["Emergenza", "Casa"])
        // A net cap fills with the balance, an income cap with what came in.
        #expect(year.funds.map(\.filled) == [2_993_100, 3_193_500])
        #expect(year.funds[0].fraction > 0.99)
        #expect(year.funds[1].fraction < 0.22)
    }

    @Test("An income cap with no cumulative income yet falls back to the balance")
    func incomeCapWithoutTotal() {
        let year = Self.build(
            rows: [],
            flows: [Self.flow("Varie", balance: 461_100, mode: .incomeCapped(cap: 500_000))]
        )
        #expect(year.funds.map(\.filled) == [461_100])
    }

    @Test("A vault with no capped envelope has no gauges")
    func noGauges() {
        let year = Self.build(rows: Self.household, flows: [Self.flow("Cash", balance: 1, mode: .unlimited)])
        #expect(year.funds.isEmpty)
    }

    // MARK: - A fresh vault

    @Test("An empty year has no people, no money and twelve blank months")
    func emptyYear() {
        let year = Self.build(rows: [])
        #expect(year.people.isEmpty)
        #expect(year.initial == 0)
        #expect(year.months.count == 12)
        let blank = year.months.allSatisfy { $0.total == 0 && $0.carried == 0 && $0.totalByPerson.isEmpty }
        #expect(blank)
    }
}
