import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The Patrimonio's table rows, what it reads off a loan beside the core's
/// overview, the residual-debt chart and the event preview.
struct WealthLoansTests {
    private typealias F = WealthFixture

    private static let rateChange = LoanEventView(
        id: "event-rate",
        event: .rateChange(date: "2026-01-01", annualRatePpm: 28_500, installment: .computed)
    )

    // MARK: - Rows

    @Test("Running loans come first in the vault's order, archived ones after them")
    func rowOrder() {
        let rows = LoanRow.rows(
            loans: [
                F.loan(id: "a", name: "Arredo", archived: true),
                F.loan(id: "m", name: "Mutuo casa"),
                F.loan(id: "p", name: "Prestito auto"),
            ],
            overview: [F.overview(loanId: "m"), F.overview(loanId: "p")],
            schedules: [:],
            today: F.today
        )
        #expect(rows.map(\.id) == ["m", "p", "a"])
    }

    @Test("A rate change makes a loan variable, and its rate is the one in force today")
    func variableAndCurrentRate() {
        let later = LoanEventView(
            id: "event-later",
            event: .rateChange(date: "2027-01-01", annualRatePpm: 40_000, installment: .computed)
        )
        let loan = F.loan(events: [Self.rateChange, later])
        #expect(LoanFacts.isVariable(loan))
        #expect(LoanFacts.currentRate(loan, today: F.today) == 28_500)
        #expect(LoanFacts.currentRate(loan, today: "2027-01-01") == 40_000)
        #expect(LoanFacts.currentRate(loan, today: "2025-12-31") == 32_500)
        #expect(LoanFacts.previousRates(loan) == ["event-rate": 32_500, "event-later": 28_500])

        let fixed = F.loan(events: [
            LoanEventView(id: "event-pay", event: .prepayment(date: "2026-06-01", amount: 100_000, mode: .shorten, fee: 0)),
        ])
        #expect(!LoanFacts.isVariable(fixed))

        let row = LoanRow.rows(loans: [loan], overview: [F.overview()], schedules: [:], today: F.today)[0]
        #expect(row.variable)
        #expect(row.rate == 28_500)
        #expect(!row.paidOff)
    }

    @Test("Progress is the installments due over the schedule's, and the row reads the overview")
    func progressAndFigures() throws {
        let row = try #require(LoanRow.rows(loans: [F.loan()], overview: [F.overview()], schedules: [:], today: F.today).first)
        #expect(row.paid == 33)
        #expect(row.total == 300)
        #expect(row.progress == 0.11)
        #expect(row.installment == 77_188)
        #expect(row.residual == 15_249_096)
        #expect(row.end == "2049-02-01")
    }

    @Test("A paid-off loan is tagged, and shows the contract's installment when no next one is left")
    func paidOff() throws {
        let loan = F.loan(terms: F.terms(installment: .contract(amount: 20_000), installmentFee: 150))
        let overview = F.overview(residual: 0, paid: 24, total: 24, nextPayment: nil)
        let row = try #require(LoanRow.rows(loans: [loan], overview: [overview], schedules: [:], today: F.today).first)
        #expect(row.paidOff)
        #expect(row.progress == 1)
        #expect(row.installment == 20_150)
    }

    @Test("A loan the overview leaves out reads its schedule, and one with neither shows nothing")
    func archivedFromSchedule() throws {
        let schedule = F.schedule([
            F.row(1, due: "2026-08-01", residual: 18_000),
            F.row(2, due: "2026-09-01", residual: 9_000),
            F.row(3, due: "2026-10-01", payment: 9_500, interest: 500, residual: 0),
        ])
        let archived = F.loan(id: "cucina", name: "Rata cucina", terms: F.terms(principal: 27_000, months: 3), archived: true)
        let rows = LoanRow.rows(
            loans: [archived, F.loan(id: "lost", name: "Senza piano", archived: true)],
            overview: [],
            schedules: ["cucina": schedule],
            today: F.today
        )
        let row = try #require(rows.first)
        #expect(row.paid == 3)
        #expect(row.total == 3)
        #expect(row.residual == 0)
        #expect(row.paidOff)
        #expect(row.end == "2026-10-01")
        // No next installment: the last one stands in.
        #expect(row.installment == 9_500)
        #expect(row.issues.isEmpty)

        let unknown = rows[1]
        #expect(unknown.paid == nil)
        #expect(unknown.residual == nil)
        #expect(unknown.progress == nil)
        #expect(!unknown.paidOff)
    }

    @Test("The derived overview counts the installments due by today as paid")
    func derivedOverview() {
        let schedule = F.schedule([
            F.row(1, due: "2026-10-01", interest: 1_000, residual: 21_000),
            F.row(2, due: "2026-11-01", interest: 800, residual: 11_800),
            F.row(3, due: "2026-12-01", interest: 600, fee: 50, residual: 2_400),
            F.row(4, due: "2027-01-01", payment: 2_420, interest: 20, residual: 0),
        ])
        let loan = F.loan(terms: F.terms(principal: 30_000, months: 4))
        let overview = LoanFacts.overview(of: loan, schedule: schedule, today: F.today)
        #expect(overview.paidCount == 2)
        #expect(overview.totalCount == 4)
        #expect(overview.residual == 11_800)
        #expect(overview.capitalRepaid == 18_200)
        #expect(overview.interestPaid == 1_800)
        #expect(overview.interestLeft == 620)
        #expect(overview.nextDue == "2026-12-01")
        #expect(overview.nextPayment == 10_050)
        #expect(overview.endDate == "2027-01-01")
        #expect(overview.registeredCount == nil)

        // Before the first installment the whole principal is owed.
        let early = LoanFacts.overview(of: loan, schedule: schedule, today: "2026-09-15")
        #expect(early.residual == 30_000)
        #expect(early.paidCount == 0)
    }

    @Test("Warnings and installments the ledger misses are issues; an archived loan has none")
    func issues() throws {
        let overview = F.overview(registered: 21, warnings: [.balloon(amount: 120_000)])
        #expect(LoanFacts.issues(overview) == [.schedule(.balloon(amount: 120_000)), .unregistered(registered: 21, due: 33)])
        #expect(LoanFacts.issues(F.overview(registered: 33)).isEmpty)
        // No linked template: nothing to compare.
        #expect(LoanFacts.issues(F.overview(registered: nil)).isEmpty)

        let rows = LoanRow.rows(
            loans: [F.loan(id: "a"), F.loan(id: "b", archived: true)],
            overview: [F.overview(loanId: "a", registered: 21), F.overview(loanId: "b", registered: 0)],
            schedules: [:],
            today: F.today
        )
        #expect(rows[0].issues == [.unregistered(registered: 21, due: 33)])
        #expect(rows[1].issues.isEmpty)
    }

    @Test("Early repayments before the first installment lower the opening debt")
    func openingDebt() {
        let repayment = LoanEventView(
            id: "event-early",
            event: .prepayment(date: "2024-02-15", amount: 1_000_000, mode: .lower, fee: 0)
        )
        let loan = F.loan(events: [repayment])
        let schedule = F.schedule([F.row(1, due: "2024-03-01", residual: 15_400_000)], beforeFirst: ["event-early"])
        #expect(LoanFacts.opening(loan, schedule: schedule) == 15_500_000)
        #expect(LoanFacts.opening(F.loan(), schedule: F.schedule([])) == 16_500_000)

        let on = LoanFacts.residual(on: "2024-03-01", loan: loan, schedule: schedule)
        #expect(on.amount == 15_400_000)
        #expect(on.afterInstallment)
        let before = LoanFacts.residual(on: "2024-02-20", loan: loan, schedule: schedule)
        #expect(before.amount == 15_500_000)
        #expect(!before.afterInstallment)
    }

    // MARK: - Chart

    @Test("The chart starts a month before the first installment and splits at today")
    func chartSplitsAtToday() throws {
        let change = LoanEventView(
            id: "event-rate",
            event: .rateChange(date: "2026-10-15", annualRatePpm: 28_500, installment: .computed)
        )
        let schedule = F.schedule([
            F.row(1, due: "2026-10-01", residual: 20_000, events: ["event-rate"]),
            F.row(2, due: "2026-11-01", residual: 10_000),
            F.row(3, due: "2026-12-01", residual: 0),
        ])
        let loan = F.loan(terms: F.terms(principal: 30_000, months: 3, firstDue: "2026-10-01"), events: [change])
        let chart = LoanChart.build(loan: loan, schedule: schedule, today: F.today)
        #expect(chart.past.map(\.day) == ["2026-09-01", "2026-10-01", "2026-11-01", F.today])
        #expect(chart.past.first?.residual == 30_000)
        #expect(chart.today == LoanChart.Point(day: F.today, residual: 10_000))
        #expect(chart.future.map(\.day) == [F.today, "2026-12-01"])
        #expect(chart.peak == 30_000)
        // The rate change sits on the line, at the debt the installment left.
        #expect(chart.events == [LoanChart.Point(day: "2026-10-15", residual: 20_000)])
    }

    @Test("Before the start the whole line is ahead; after the end it is all behind, with no rule")
    func chartOutsideTheLoan() {
        let schedule = F.schedule([F.row(1, due: "2026-10-01", residual: 0)])
        let loan = F.loan(terms: F.terms(principal: 10_000, months: 1, firstDue: "2026-10-01"))
        let before = LoanChart.build(loan: loan, schedule: schedule, today: "2026-08-01")
        #expect(before.past.isEmpty)
        #expect(before.future.count == 2)
        #expect(before.today == nil)
        let after = LoanChart.build(loan: loan, schedule: schedule, today: "2027-01-01")
        #expect(after.future.isEmpty)
        #expect(after.past.count == 2)
        #expect(after.today == nil)
    }

    @Test("A month before keeps the day, or the month's last when it is shorter")
    func monthBefore() {
        #expect(LoanChart.monthBefore("2024-03-01") == "2024-02-01")
        #expect(LoanChart.monthBefore("2024-03-31") == "2024-02-29")
        #expect(LoanChart.monthBefore("2026-01-15") == "2025-12-15")
    }

    // MARK: - The event preview

    @Test("An event's effect compares the installment after it, the end and the interest")
    func eventEffect() {
        let current = F.schedule([
            F.row(1, due: "2026-11-01", residual: 30_000),
            F.row(2, due: "2026-12-01", payment: 10_000, interest: 600, residual: 20_600),
            F.row(3, due: "2027-01-01", payment: 10_000, interest: 400, residual: 11_000),
            F.row(4, due: "2027-02-01", payment: 11_100, interest: 100, residual: 0),
        ])
        let shortened = F.schedule(
            [
                F.row(1, due: "2026-11-01", residual: 20_000),
                F.row(2, due: "2026-12-01", payment: 10_000, interest: 400, fee: 50, residual: 10_400),
                F.row(3, due: "2027-01-01", payment: 10_500, interest: 100, residual: 0),
            ],
            warnings: [.prepaymentClipped(eventId: "x")]
        )
        let effect = LoanEventEffect.of(current: current, next: shortened, date: "2026-11-01")
        #expect(effect.installment == 10_000)
        #expect(!effect.installmentChanged)
        #expect(effect.fee == 50)
        #expect(effect.end == "2027-01-01")
        #expect(effect.previousEnd == "2027-02-01")
        #expect(effect.endChanged)
        #expect(effect.interestSaved == 2_100 - 1_500)
        #expect(effect.newWarnings == [.prepaymentClipped(eventId: "x")])
    }

    @Test("The other mode of a repayment keeps everything else; a rate change has none")
    func otherMode() {
        let repayment = LoanEvent.prepayment(date: "2026-12-01", amount: 1_000_000, mode: .shorten, fee: 5_000)
        #expect(LoanEventEffect.otherMode(repayment) == .prepayment(date: "2026-12-01", amount: 1_000_000, mode: .lower, fee: 5_000))
        #expect(LoanEventEffect.otherMode(Self.rateChange.event) == nil)
        #expect(Self.rateChange.event.effectiveDate == "2026-01-01")
    }

    // MARK: - Words

    @Test("Rates are written in percent with the ledger's comma")
    func rates() {
        #expect(WealthText.rate(28_500) == "2,85%")
        #expect(WealthText.rate(31_250, decimals: 3) == "3,125%")
        #expect(WealthText.rate(0) == "0,00%")
    }

    @Test("Fine is the month and the year")
    func monthYear() {
        #expect(WealthText.monthYear("2049-02-01", locale: Locale(identifier: "it_IT")) == "feb 2049")
    }

    @Test("A rate change reads old rate to new, a repayment its amount and mode")
    func eventDetail() {
        #expect(WealthText.detail(Self.rateChange.event, previousRate: 32_500) == "3,25% \u{2192} 2,85%")
        let repayment = LoanEvent.prepayment(date: "2026-12-01", amount: 1_000_000, mode: .shorten, fee: 0)
        #expect(WealthText.detail(repayment, previousRate: nil) == "10.000,00 \u{00B7} \(WealthText.mode(.shorten).lowercased())")
    }

    @Test("The cost note says what the total cost holds beside the interest")
    func costNote() {
        #expect(WealthText.costNote(totalCost: 5_000, totalInterest: 5_000, upfrontFees: 0)
            == String(localized: "Interest over the whole schedule."))
        #expect(WealthText.costNote(totalCost: 6_000, totalInterest: 5_000, upfrontFees: 1_000)
            == String(localized: "Interest over the whole schedule plus the upfront fees of \(LedgerMoney.bare(1_000))."))
        #expect(WealthText.costNote(totalCost: 6_500, totalInterest: 5_000, upfrontFees: 1_000)
            == String(localized: "Interest over the whole schedule plus \(LedgerMoney.bare(1_500)) of fees and penalties."))
    }
}
