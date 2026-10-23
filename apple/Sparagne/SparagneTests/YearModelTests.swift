import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The RIEPILOGO's arithmetic. `YearSummary.build` is
/// pure, so the year is checked here without a database: the query that feeds
/// it is tested in the core.
struct YearModelTests {
    private static let upTo = MonthKey(year: 2026, month: 9)

    private static func row(
        _ bucket: UInt32,
        _ person: String,
        income: Int64 = 0,
        opening: Int64 = 0,
        expense: Int64 = 0
    ) -> BucketPersonTotals {
        BucketPersonTotals(
            bucket: bucket,
            person: person,
            income: income,
            opening: opening,
            expense: expense
        )
    }

    /// Two people with money already in the vault before January, a January
    /// that earns and spends, a wallet opened in March and a quiet September.
    private static var household: [BucketPersonTotals] {
        [
            row(0, "matteo", income: 100_000, opening: 500_000, expense: 50_000),
            row(0, "elisa", opening: 200_000),
            row(1, "elisa", income: 200_000, expense: 50_000),
            row(1, "matteo", income: 300_000, expense: 115_000),
            row(3, "matteo", income: 300_000, opening: 100_000, expense: 100_000),
            row(9, "elisa", income: 100_000, expense: 5_000),
        ]
    }

    private static func build(
        year: Int = 2026,
        rows: [BucketPersonTotals],
        previousRows: [BucketPersonTotals] = [],
        upTo: MonthKey = upTo,
        vaultPeople: [String] = [],
        person: String? = nil,
        today: MonthKey? = nil
    ) -> YearSummary {
        YearSummary.build(
            year: year,
            upTo: upTo,
            rows: rows,
            previousRows: previousRows,
            vaultPeople: vaultPeople,
            person: person,
            today: today
        )
    }

    // MARK: - People and the opening cash fund

    @Test("The columns are the vault's people, case-insensitively ordered")
    func peopleAreColumns() {
        let year = Self.build(rows: Self.household)
        #expect(year.people == ["elisa", "matteo"])
    }

    @Test("A year with no rows for someone in the vault still has their column, at zero")
    func everyYearHasTheVaultsColumns() {
        let empty = Self.build(year: 2025, rows: [], upTo: MonthKey(year: 2025, month: 12), vaultPeople: ["matteo"])
        #expect(empty.people == ["matteo"])
        #expect(empty.initialByPerson == [0])
        #expect(empty.months.allSatisfy { $0.totalByPerson == [0] })

        let merged = Self.build(rows: [Self.row(1, "Elisa", income: 100)], vaultPeople: ["matteo", "Elisa"])
        #expect(merged.people == ["Elisa", "matteo"])
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

    @Test("RISPARMIO is income less USCITE, everybody's expenses together")
    func savingsAreIncomeLessExpenses() {
        let january = Self.build(rows: Self.household).months[0]
        #expect(january.income == 500_000)
        #expect(january.expense == 165_000)
        #expect(january.savings == 335_000)
    }

    @Test("TOTALE is FONDO CASSA plus RISPARMIO, every month")
    func totalIdentityHolds() {
        let year = Self.build(rows: Self.household)
        for month in year.months {
            #expect(month.total == month.carried + month.savings)
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
                Self.row(1, "matteo", income: 50_000, expense: 20_000),
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
        #expect(opening.expense == nil)
        #expect(opening.savings == nil)
        #expect(opening.carried == nil)
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
        let rows = Self.build(rows: Self.household + [Self.row(2, "matteo", expense: 100_000)]).tableRows
        #expect(rows[1].trendUp)
        #expect(!rows[2].trendUp)
        #expect(rows[3].trendUp)
        let january = Self.build(rows: [Self.row(0, "elisa", opening: 100_000), Self.row(1, "elisa", expense: 1)])
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
        #expect(sum.expense == 270_000)
        #expect(sum.savings == 630_000)
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
            trailingIncome: 1_000_000, trailingSavings: 290_000
        )
        #expect(kpis.savings == 123_006)
        #expect(kpis.previous?.savings == 141_000)
        #expect(kpis.savingsDelta == -17_994)
    }

    @Test("The cards are the table's rows: expenses are USCITE")
    func cardsAreTheRows() throws {
        let august = MonthKey(year: 2026, month: 8)
        let year = Self.build(
            rows: [
                Self.row(7, "matteo", income: 400_000, expense: 250_000),
                Self.row(8, "matteo", income: 500_000, expense: 122_000),
            ],
            upTo: august
        )
        let kpis = try #require(year.kpis)
        let row = try #require(year.tableRows.first { $0.kind == .month(august) })
        #expect(kpis.income == row.income)
        #expect(kpis.expenses == row.expense)
        #expect(kpis.savings == row.savings)
        #expect(kpis.expenses == 122_000)
        #expect(kpis.previous == .init(income: 400_000, expenses: 250_000))
        #expect(kpis.savingsDelta == Int64(228_000))
        // With nothing in the year before, the last twelve months are the
        // year so far, counted the same way.
        #expect(kpis.trailingSavings == year.yearSavings)
        #expect(kpis.trailingRate == Double(year.yearSavings) / Double(year.yearIncome))
    }

    @Test("January has no month before it to compare with")
    func januaryHasNoPrevious() throws {
        let year = Self.build(
            rows: [Self.row(0, "matteo", opening: 100_000), Self.row(1, "matteo", income: 50_000, expense: 20_000)],
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
            trailingIncome: 1_000_000, trailingSavings: 290_000
        )
        #expect(kpis.rate == 0.75)
        #expect(kpis.trailingRate == 0.29)
        let none = MonthKPIs(
            income: 0, expenses: 10, previous: nil, trailingIncome: 0, trailingSavings: 0
        )
        #expect(none.rate == nil)
        #expect(none.trailingRate == nil)
    }

    // MARK: - The last twelve months

    @Test("The rate's last twelve months reach into the year before and stop a year back")
    func trailingRateReachesTheYearBefore() throws {
        let march = MonthKey(year: 2026, month: 3)
        let year = Self.build(
            rows: [
                Self.row(1, "matteo", income: 400_000, expense: 100_000),
                Self.row(3, "matteo", income: 400_000, expense: 100_000),
            ],
            previousRows: [
                // March 2025 is thirteen months back: out.
                Self.row(2, "matteo", income: 1_000_000),
                // August 2025, the holidays: in.
                Self.row(7, "matteo", income: 400_000, expense: 300_000),
            ],
            upTo: march
        )
        let kpis = try #require(year.kpis)
        #expect(kpis.rate == 0.75)
        #expect(kpis.trailingIncome == 1_200_000)
        #expect(kpis.trailingSavings == 700_000)
        // The table's year is still January to March.
        #expect(year.yearRate == 0.75)
    }

    @Test("Under PERSONA the year before is that person's too")
    func trailingFollowsPersona() throws {
        let year = Self.build(
            rows: [Self.row(1, "elisa", income: 100_000, expense: 50_000)],
            previousRows: [
                Self.row(11, "elisa", income: 100_000, expense: 90_000),
                Self.row(11, "matteo", income: 900_000, expense: 10_000),
            ],
            upTo: MonthKey(year: 2026, month: 1),
            person: "elisa"
        )
        let kpis = try #require(year.kpis)
        #expect(kpis.trailingIncome == 200_000)
        #expect(kpis.trailingSavings == 60_000)
        #expect(year.previousYear.count == 12)
    }

    @Test("Autonomy is TOTALE over the average month of the last twelve finished, from the first that moved money")
    func autonomy() throws {
        let april = MonthKey(year: 2026, month: 4)
        let year = Self.build(
            rows: [
                Self.row(0, "matteo", opening: 2_000_000),
                Self.row(1, "matteo", income: 300_000, expense: 150_000),
                // February moved nothing: a month of no spending, counted.
                Self.row(3, "matteo", income: 300_000, expense: 150_000),
                // April is still going: TOTALE has it, the average does not.
                Self.row(4, "matteo", income: 300_000, expense: 900_000),
            ],
            previousRows: [
                // Thirteen months back from March: out.
                Self.row(2, "matteo", income: 1_000_000, expense: 5_000_000),
                // The vault's first months; April to October 2025 are empty
                // and before them, so they are no months of no spending.
                Self.row(10, "matteo", income: 300_000, expense: 200_000),
                Self.row(11, "matteo", income: 300_000, expense: 400_000),
            ],
            upTo: april,
            today: april
        )
        let autonomy = try #require(year.autonomy)
        // November to March: 900,00 over five months.
        #expect(autonomy == Autonomy(total: 1_700_000, monthlyExpense: 180_000))
        #expect(autonomy.months == 1_700_000.0 / 180_000.0)
    }

    @Test("A live January reads the whole year before, and a vault in the red lasts no months")
    func autonomyInJanuary() throws {
        let january = MonthKey(year: 2026, month: 1)
        let year = Self.build(
            rows: [
                Self.row(0, "matteo", opening: 1_500_000),
                Self.row(1, "matteo", income: 300_000, expense: 2_000_000),
            ],
            previousRows: (0..<12).map { Self.row(UInt32($0), "matteo", income: 300_000, expense: $0 == 5 ? 700_000 : 100_000) },
            upTo: january,
            today: january
        )
        let autonomy = try #require(year.autonomy)
        #expect(autonomy.monthlyExpense == 150_000)
        #expect(autonomy.total == -200_000)
        #expect(autonomy.months == 0)
    }

    @Test("No autonomy without an expense to measure it on, nor in a year to come")
    func noAutonomy() {
        #expect(Self.build(rows: [Self.row(1, "matteo", income: 100_000)]).autonomy == nil)
        #expect(Self.build(year: 2027, rows: Self.household).autonomy == nil)
        // Nobody said what today is: every month drawn is finished.
        #expect(Self.build(rows: Self.household).autonomy == Autonomy(total: 1_480_000, monthlyExpense: 30_000))
    }

    // MARK: - PERSONA

    @Test("Under PERSONA the other person's rows are gone before anything is summed, and the one column is theirs")
    func personFilter() {
        let year = Self.build(rows: Self.household, vaultPeople: ["matteo", "elisa", "giulia"], person: "elisa")
        #expect(year.person == "elisa")
        #expect(year.people == ["elisa"])
        #expect(year.initialByPerson == [200_000])
        #expect(year.initial == 200_000)
        // January is elisa's alone: matteo's 3.000,00 are not there.
        #expect(year.months[0].income == 200_000)
        #expect(year.months[0].expense == 50_000)
        #expect(year.months[0].total == 350_000)
        #expect(year.months[0].totalByPerson == [350_000])
        // March was matteo's, opening included: nothing moves for elisa.
        #expect(year.months[2].income == 0)
        #expect(year.months[2].total == 350_000)
        #expect(year.closingTotal == 445_000)
    }

    @Test("A person with no rows in the year gets a column of zeros under PERSONA")
    func personFilterWithoutRows() {
        let year = Self.build(rows: Self.household, person: "giulia")
        #expect(year.people == ["giulia"])
        #expect(year.initialByPerson == [0])
        let zeros = year.months.allSatisfy { $0.income == 0 && $0.total == 0 && $0.totalByPerson == [0] }
        #expect(zeros)
        #expect(year.personFigures == [PersonYear(person: "giulia", income: 0, expenses: 0, growth: 0)])
        #expect(year.personFigures[0].rate == nil)
    }

    @Test("Each person's figures add up to the month's, every month")
    func byPersonSumsToTheMonth() {
        let year = Self.build(rows: Self.household + [Self.row(5, "elisa", income: 1_000, expense: 70_000)])
        for month in year.months {
            #expect(month.byPerson.count == year.people.count)
            #expect(month.byPerson.reduce(0) { $0 + $1.income } == month.income)
            #expect(month.byPerson.reduce(0) { $0 + $1.expense } == month.expense)
            #expect(month.byPerson.reduce(0) { $0 + $1.total } == month.total)
        }
        // January: elisa, then matteo.
        #expect(
            year.months[0].byPerson == [
                PersonMonth(income: 200_000, expense: 50_000, total: 350_000),
                PersonMonth(income: 300_000, expense: 115_000, total: 735_000),
            ]
        )
    }

    // MARK: - TASSO

    @Test("TASSO is savings over income, absent without income and negative when the month spent more")
    func monthRate() {
        let year = Self.build(rows: Self.household + [Self.row(4, "matteo", income: 100_000, expense: 150_000)])
        #expect(year.months[0].rate == 335_000.0 / 500_000.0)
        // February has no rows, and a month with expenses only has no rate.
        #expect(year.months[1].rate == nil)
        #expect(Self.build(rows: [Self.row(1, "matteo", expense: 1)]).months[0].rate == nil)
        #expect(year.months[3].rate == -0.5)
        // The table prints the month's own.
        #expect(year.tableRows[4].rate == -0.5)
        #expect(year.tableRows[0].rate == nil)
    }

    // MARK: - The month of today

    @Test("The month on screen is live only when it is the month of today")
    func liveMonth() {
        #expect(Self.build(rows: Self.household, today: Self.upTo).isLive)
        #expect(!Self.build(rows: Self.household, today: MonthKey(year: 2026, month: 10)).isLive)
        #expect(!Self.build(rows: Self.household).isLive)
        // A live month is not finished; a past one is.
        #expect(Self.build(rows: Self.household, today: Self.upTo).finished.count == 8)
        #expect(Self.build(rows: Self.household, today: MonthKey(year: 2026, month: 10)).finished.count == 9)
    }

    @Test("A month on screen after today is not over: the finished months stop before today")
    func finishedStopsAtToday() {
        let december = MonthKey(year: 2026, month: 12)
        let year = Self.build(rows: Self.household, upTo: december, today: MonthKey(year: 2026, month: 10))
        // Every month is drawn, but October is going and the two after it
        // have not begun.
        #expect(year.elapsed.count == 12)
        #expect(year.finished.map(\.month.month) == Array(1...9))
        #expect(!year.isLive)
        // A past year is over whole, a year to come not at all.
        let past = Self.build(year: 2025, rows: Self.household, upTo: december, today: MonthKey(year: 2026, month: 10))
        #expect(past.finished.count == 12)
        // Nobody said what today is: every month drawn counts.
        #expect(Self.build(rows: Self.household, upTo: december).finished.count == 12)
    }

    /// An opening of 1.000,00 and three finished months, the third with a
    /// cent the pace cannot split; April is still going.
    private static var threeMonths: [BucketPersonTotals] {
        [
            row(0, "matteo", opening: 100_000),
            row(1, "matteo", income: 300_000, expense: 100_000),
            row(2, "matteo", income: 300_000, expense: 200_000),
            row(3, "matteo", income: 300_000, expense: 150_001),
            row(4, "matteo", income: 50_000),
        ]
    }

    @Test("The projection reads the pace of the finished months and runs it to December")
    func projection() throws {
        let april = MonthKey(year: 2026, month: 4)
        let year = Self.build(rows: Self.threeMonths, upTo: april, today: april)
        #expect(year.months[2].total == 549_999)
        let projection = try #require(year.projection)
        #expect(projection.finishedMonths == 3)
        // 4.499,99 over three months, the third of a cent dropped.
        #expect(projection.monthlyGrowth == 149_999)
        // April is not over: nine months still to come, April included.
        #expect(projection.yearEnd == 549_999 + 149_999 * 9)
        #expect(projection.averageExpense == 150_000)
    }

    @Test("A wallet opened in a finished month raises TOTALE, not the pace")
    func projectionIgnoresOpenings() throws {
        let april = MonthKey(year: 2026, month: 4)
        let rows = Self.threeMonths + [Self.row(2, "matteo", opening: 1_200_000)]
        let year = Self.build(rows: rows, upTo: april, today: april)
        #expect(year.months[2].total == 1_749_999)
        let projection = try #require(year.projection)
        #expect(projection.monthlyGrowth == 149_999)
        // The year ends from where March left it, the wallet included.
        #expect(projection.yearEnd == 1_749_999 + 149_999 * 9)

        // A wallet of 12.000 opened in January, with February live: the
        // pace is January's savings alone.
        let february = MonthKey(year: 2026, month: 2)
        let opened = Self.build(
            rows: [Self.row(1, "matteo", income: 200_000, opening: 1_200_000, expense: 150_000)],
            upTo: february,
            today: february
        )
        let pace = try #require(opened.projection)
        #expect(pace.monthlyGrowth == 50_000)
        #expect(pace.yearEnd == 1_250_000 + 50_000 * 11)
    }

    @Test("A pace that loses money rounds toward zero")
    func projectionLosingMoney() throws {
        let april = MonthKey(year: 2026, month: 4)
        let rows = [
            Self.row(0, "matteo", opening: 1_000),
            Self.row(1, "matteo", expense: 4),
            Self.row(2, "matteo", expense: 3),
            Self.row(3, "matteo", expense: 3),
        ]
        let projection = try #require(Self.build(rows: rows, upTo: april, today: april).projection)
        #expect(projection.monthlyGrowth == -3)
        #expect(projection.yearEnd == 990 - 3 * 9)
    }

    @Test("No projection for a live January, for a month gone by, or when nobody said what today is")
    func noProjection() {
        let january = MonthKey(year: 2026, month: 1)
        #expect(Self.build(rows: Self.threeMonths, upTo: january, today: january).projection == nil)
        let april = MonthKey(year: 2026, month: 4)
        #expect(Self.build(rows: Self.threeMonths, upTo: april, today: MonthKey(year: 2026, month: 5)).projection == nil)
        #expect(Self.build(rows: Self.threeMonths, upTo: april).projection == nil)
    }

    // MARK: - Per persona

    @Test("Each person's year: income, USCITE and how much their share of TOTALE moved")
    func personFigures() {
        let year = Self.build(rows: Self.household)
        #expect(
            year.personFigures == [
                PersonYear(person: "elisa", income: 300_000, expenses: 55_000, growth: 245_000),
                PersonYear(person: "matteo", income: 600_000, expenses: 215_000, growth: 485_000),
            ]
        )
        for (index, figures) in year.personFigures.enumerated() {
            #expect(figures.growth == year.closingByPerson[index] - year.initialByPerson[index])
        }
        #expect(year.personFigures.reduce(0) { $0 + $1.growth } == year.growth)
        #expect(year.personFigures.reduce(0) { $0 + $1.expenses } == year.yearExpense)
        #expect(year.personFigures[0].rate == 245_000.0 / 300_000.0)
    }

    @Test("A person who earned nothing has no rate")
    func personWithoutIncome() {
        let year = Self.build(rows: [Self.row(0, "elisa", opening: 1_000), Self.row(1, "elisa", expense: 500)])
        #expect(year.personFigures == [PersonYear(person: "elisa", income: 0, expenses: 500, growth: -500)])
        #expect(year.personFigures[0].rate == nil)
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
