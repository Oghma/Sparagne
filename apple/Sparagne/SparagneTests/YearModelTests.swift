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

    private static func build(
        year: Int = 2026, rows: [YearRow], flows: [FlowView] = [], upTo: MonthKey = upTo
    ) -> YearSummary {
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

    // MARK: - The table

    @Test("The table is the opening row, twelve months and the sum")
    func tableShape() {
        let rows = Self.build(rows: Self.household).tableRows
        #expect(rows.count == 14)
        #expect(rows.first?.kind == .opening)
        #expect(rows.last?.kind == .sum)
        #expect(rows[1].kind == .month(MonthKey(year: 2026, month: 1)))
        #expect(rows[12].kind == .month(MonthKey(year: 2026, month: 12)))
    }

    @Test("The opening row carries only TOTALE, in total and per person")
    func openingRow() {
        let opening = Self.build(rows: Self.household).tableRows[0]
        #expect(opening.total == 750_000)
        #expect(opening.totalByPerson == [200_000, 550_000])
        #expect(opening.income == nil)
        #expect(opening.cashExpense == nil)
        #expect(opening.savings == nil)
        #expect(opening.carried == nil)
        #expect(opening.fundExpense == nil)
        #expect(!opening.isFuture)
        #expect(!opening.isCurrent)
    }

    @Test("Only the month on screen is current, and the later ones are blank")
    func currentAndFutureFlags() {
        let months = Array(Self.build(rows: Self.household).tableRows[1...12])
        #expect(months.filter(\.isCurrent).map(\.kind) == [.month(Self.upTo)])
        #expect(months.filter(\.isFuture).count == 3)
        let blank = months.filter(\.isFuture).allSatisfy { $0.total == nil && $0.income == nil && !$0.isCurrent }
        #expect(blank)
    }

    @Test("A past year has no current month and no blank one, a year ahead only blank ones")
    func flagsInOtherYears() {
        let past = Array(Self.build(year: 2025, rows: Self.household).tableRows[1...12])
        #expect(past.allSatisfy { !$0.isCurrent && !$0.isFuture })
        let coming = Array(Self.build(year: 2027, rows: Self.household).tableRows[1...12])
        #expect(coming.allSatisfy { $0.isFuture && !$0.isCurrent })
    }

    @Test("TOTALE goes up or down against the line before, January against the opening fund")
    func trend() {
        // February spends 1.000 with nothing coming in: down. March earns.
        let rows = Self.build(rows: Self.household + [Self.row(2, "matteo", cashExpense: 100_000)]).tableRows
        #expect(rows[1].trendUp)
        #expect(!rows[2].trendUp)
        #expect(rows[3].trendUp)
        let january = Self.build(rows: [Self.row(0, "elisa", opening: 100_000), Self.row(1, "elisa", cashExpense: 1)])
        #expect(!january.tableRows[1].trendUp)
    }

    @Test("A month that changes nothing is not a drop")
    func flatIsUp() {
        let rows = Self.build(rows: Self.household).tableRows
        #expect(rows[2].total == rows[1].total)
        #expect(rows[2].trendUp)
    }

    @Test("The sum adds the flows of the months up to the one on screen and closes on TOTALE")
    func sumRow() {
        let year = Self.build(rows: Self.household)
        let sum = year.tableRows[13]
        #expect(sum.income == 900_000)
        #expect(sum.cashExpense == 245_000)
        #expect(sum.savings == 655_000)
        #expect(sum.fundExpense == 25_000)
        #expect(sum.carried == nil)
        #expect(sum.total == 1_480_000)
        #expect(sum.totalByPerson == year.months[8].totalByPerson)
        #expect(year.growth == 730_000)
    }

    @Test("A year with nothing elapsed sums to zero and closes on the opening fund")
    func sumOfAYearAhead() {
        let sum = Self.build(year: 2027, rows: Self.household).tableRows[13]
        #expect(sum.income == 0)
        #expect(sum.total == 750_000)
        #expect(sum.totalByPerson == [200_000, 550_000])
    }

    // MARK: - The month's cards

    @Test("Savings are income less expenses, and the delta is against the month before")
    func kpiDelta() {
        let kpis = MonthKPIs(
            income: 185_000, expenses: 61_994, previous: .init(income: 425_000, expenses: 284_000),
            yearIncome: 1_000_000, yearSavings: 290_000
        )
        #expect(kpis.savings == 123_006)
        #expect(kpis.previous?.savings == 141_000)
        #expect(kpis.savingsDelta == -17_994)
    }

    @Test("The cards are the table's rows: expenses are USCITE, fund expenses apart")
    func cardsAreTheRows() throws {
        // August: 920,00 spent from a fund, 300,00 from the cash envelopes.
        let august = MonthKey(year: 2026, month: 8)
        let year = Self.build(
            rows: [
                Self.row(7, "matteo", income: 400_000, cashExpense: 250_000),
                Self.row(8, "matteo", income: 500_000, cashExpense: 30_000, fundExpense: 92_000),
            ],
            upTo: august
        )
        let kpis = try #require(year.kpis)
        let row = try #require(year.tableRows.first { $0.kind == .month(august) })
        #expect(kpis.income == row.income)
        #expect(kpis.expenses == row.cashExpense)
        #expect(kpis.savings == row.savings)
        #expect(kpis.expenses == 30_000)
        #expect(kpis.previous == .init(income: 400_000, expenses: 250_000))
        #expect(kpis.savingsDelta == Int64(320_000))
        // The year to date counts the same way.
        #expect(kpis.yearSavings == year.yearSavings)
        #expect(kpis.yearRate == Double(year.yearSavings) / Double(year.yearIncome))
    }

    @Test("January has no month before it to compare with")
    func januaryHasNoPrevious() throws {
        let year = Self.build(
            rows: [Self.row(0, "matteo", opening: 100_000), Self.row(1, "matteo", income: 50_000, cashExpense: 20_000)],
            upTo: MonthKey(year: 2026, month: 1)
        )
        let kpis = try #require(year.kpis)
        #expect(kpis.savings == 30_000)
        #expect(kpis.previous == nil)
        #expect(kpis.savingsDelta == nil)
    }

    @Test("A year to come has no cards")
    func noCardsInTheFuture() {
        #expect(Self.build(year: 2027, rows: []).kpis == nil)
    }

    @Test("The rates are over income, and absent without any")
    func kpiRates() {
        let kpis = MonthKPIs(
            income: 200_000, expenses: 50_000, previous: nil,
            yearIncome: 1_000_000, yearSavings: 290_000
        )
        #expect(kpis.rate == 0.75)
        #expect(kpis.yearRate == 0.29)
        let none = MonthKPIs(
            income: 0, expenses: 10, previous: nil, yearIncome: 0, yearSavings: 0
        )
        #expect(none.rate == nil)
        #expect(none.yearRate == nil)
    }

    @Test("A fund knows whether its cap is on the balance or on the income")
    func fundKind() {
        let year = Self.build(
            rows: [],
            flows: [
                Self.flow("Vacanze", balance: 1, mode: .netCapped(cap: 100)),
                Self.flow("Casa", balance: 1, mode: .incomeCapped(cap: 100), incomeTotal: 50),
            ]
        )
        #expect(year.funds.map(\.kind) == [.balance, .income])
    }

    // MARK: - Chart scale

    @Test("The axis covers the data on a 1-2-5 step, and a flow chart keeps its zero")
    func niceScale() {
        let flow = ChartScale.nice([-420, 4_250, 1_850], includeZero: true)
        #expect(flow.lower == -2_000)
        #expect(flow.upper == 6_000)
        #expect(flow.step == 2_000)
        let fund = ChartScale.nice([12_410, 21_550], includeZero: false)
        #expect(fund.lower <= 12_410)
        #expect(fund.upper >= 21_550)
        let flat = ChartScale.nice([500, 500], includeZero: false)
        #expect(flat.upper > flat.lower)
    }

    @Test("Axis labels are compact")
    func compactLabels() {
        let it = Locale(identifier: "it_IT")
        let en = Locale(identifier: "en_US")
        #expect(ChartScale.compact(840, locale: it) == "840")
        #expect(ChartScale.compact(12_000, locale: it) == "12k")
        #expect(ChartScale.compact(1_500, locale: it) == "1,5k")
        #expect(ChartScale.compact(-2_500, locale: it) == "-2,5k")
        #expect(ChartScale.compact(1_500, locale: en) == "1.5k")
        #expect(ChartScale.compact(-2_500, locale: en) == "-2.5k")
    }

    @Test("Month names keep the locale's casing")
    func monthNames() {
        #expect(SummaryText.monthName(10, locale: Locale(identifier: "it_IT")) == "ottobre")
        #expect(SummaryText.monthName(10, locale: Locale(identifier: "en_US")) == "October")
        #expect(SummaryText.monthTitle(8, year: 2026, locale: Locale(identifier: "it_IT")) == "Agosto 2026")
    }
}
