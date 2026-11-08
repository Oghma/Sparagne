import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// A loan's template in step with its schedule (`LoanLink`), on schedules
/// made by hand: the installment it records from now on, the patch that
/// brings it in line, the template a new loan gets, the fields an edit
/// sends, the templates a loan may link. None of it reaches the core.
struct LoanLinkTests {
    /// Three installments on the 5th from January 2026, 2,00 of fee each;
    /// the last one smaller.
    private static let schedule = LoanSchedule(
        rows: [
            row(1, "2026-01-05", payment: 34_000),
            row(2, "2026-02-05", payment: 34_000),
            row(3, "2026-03-05", payment: 33_950),
        ],
        eventIdsBeforeFirst: [],
        totalInterest: 1_950,
        totalFees: 600,
        totalPrepaid: 0,
        totalPenalties: 0,
        totalCost: 2_550,
        taegPpm: nil,
        warnings: []
    )

    private static func row(_ number: UInt32, _ due: NaiveDate, payment: Int64) -> LoanScheduleRow {
        LoanScheduleRow(
            number: number,
            dueDate: due,
            payment: payment,
            interest: 0,
            capital: payment,
            fee: 200,
            residual: 0,
            annualRatePpm: 30_000,
            eventIds: []
        )
    }

    private static let terms = LoanTerms(
        principal: 100_000,
        annualRatePpm: 30_000,
        months: 3,
        firstDue: "2026-01-05",
        installment: .computed,
        upfrontFees: 0,
        installmentFee: 200
    )

    private static func template(
        _ id: Uuid = "template",
        amount: Int64 = 34_200,
        kind: TransactionKind = .expense,
        start: NaiveDate = "2026-01-05",
        end: NaiveDate? = "2026-03-05",
        archived: Bool = false
    ) -> RecurringView {
        RecurringView(
            id: id,
            kind: kind,
            amount: amount,
            walletId: nil,
            flowId: nil,
            category: "Mutuo",
            note: "Mutuo",
            schedule: Schedule(frequency: .monthly(day: 5), interval: 1, startDate: start, endDate: end),
            enabled: true,
            archived: archived,
            owner: "matteo"
        )
    }

    private static func loan(_ id: Uuid, linking recurringId: Uuid?, archived: Bool = false) -> LoanView {
        LoanView(
            id: id,
            name: "Mutuo",
            note: nil,
            terms: terms,
            events: [],
            recurringId: recurringId,
            archived: archived,
            createdBy: "matteo"
        )
    }

    // MARK: - The installment from now on

    @Test("The current installment is the first one due after today, with its fee; the last one once all are due")
    func currentInstallment() {
        #expect(LoanLink.currentInstallment(Self.schedule, today: "2025-12-01") == 34_200)
        // Due today is paid today: the next one is the current.
        #expect(LoanLink.currentInstallment(Self.schedule, today: "2026-02-05") == 34_150)
        #expect(LoanLink.currentInstallment(Self.schedule, today: "2026-02-04") == 34_200)
        #expect(LoanLink.currentInstallment(Self.schedule, today: "2026-09-01") == 34_150)
        var empty = Self.schedule
        empty.rows = []
        #expect(LoanLink.currentInstallment(empty, today: "2026-01-01") == nil)
    }

    // MARK: - Bringing the template in line

    @Test("A template in line with its schedule needs nothing")
    func alignedNeedsNothing() {
        #expect(LoanLink.alignment(of: Self.template(), to: Self.schedule, today: "2025-12-01").isEmpty)
    }

    @Test("Another amount and another end are patched, the rest of the schedule kept")
    func misalignedIsPatched() {
        let typed = Self.template(amount: 30_000, start: "2025-06-05", end: nil)
        let patch = LoanLink.alignment(of: typed, to: Self.schedule, today: "2025-12-01")
        #expect(patch.amount == 34_200)
        #expect(patch.schedule == Schedule(frequency: .monthly(day: 5), interval: 1, startDate: "2025-06-05", endDate: "2026-03-05"))
        #expect(patch.walletId == nil)
        #expect(patch.category == nil)
        #expect(patch.note == nil)
        #expect(patch.owner == nil)
        #expect(patch.enabled == nil)
    }

    @Test("An end before the template's start is left out: the core would refuse it")
    func endBeforeStartIsLeftOut() {
        let late = Self.template(amount: 34_200, start: "2026-06-05", end: nil)
        #expect(LoanLink.alignment(of: late, to: Self.schedule, today: "2025-12-01").isEmpty)
    }

    // MARK: - A new loan's template

    @Test("A new template starts with the first installment, or for a loan already running with the next one due")
    func templateStart() {
        #expect(LoanLink.templateStart(Self.schedule, firstDue: "2026-01-05", today: "2025-12-01") == "2026-01-05")
        #expect(LoanLink.templateStart(Self.schedule, firstDue: "2026-01-05", today: "2026-01-05") == "2026-01-05")
        #expect(LoanLink.templateStart(Self.schedule, firstDue: "2026-01-05", today: "2026-01-06") == "2026-02-05")
        #expect(LoanLink.templateStart(Self.schedule, firstDue: "2026-01-05", today: "2026-02-05") == "2026-02-05")
        #expect(LoanLink.templateStart(Self.schedule, firstDue: "2026-01-05", today: "2026-03-06") == nil)
    }

    @Test("The new template: an expense of the installment with its fee, monthly on the first one's day to the last, noted with the loan's name")
    func newTemplate() throws {
        let target = LoanLedgerTarget(walletId: "conto", flowId: "casa", category: "  ", owner: "elisa")
        let command = try #require(LoanLink.template(
            named: " Mutuo ",
            terms: Self.terms,
            schedule: Self.schedule,
            target: target,
            owner: "elisa",
            today: "2025-12-01"
        ))
        #expect(command == .createRecurring(
            transactionKind: .expense,
            amount: 34_200,
            walletId: "conto",
            flowId: "casa",
            category: nil,
            note: "Mutuo",
            schedule: Schedule(frequency: .monthly(day: 5), interval: 1, startDate: "2026-01-05", endDate: "2026-03-05"),
            owner: "elisa"
        ))
        // A loan with nothing left to record gets no template.
        #expect(LoanLink.template(
            named: "Mutuo",
            terms: Self.terms,
            schedule: Self.schedule,
            target: target,
            owner: nil,
            today: "2026-04-01"
        ) == nil)
    }

    // MARK: - Editing

    @Test("An edit sends only what changed; a blank note clears it; nothing changed is an empty patch")
    func patch() {
        var loan = Self.loan("loan", linking: nil)
        loan.note = "Fisso"
        #expect(LoanLink.patch(loan, name: " Mutuo ", note: "Fisso", terms: Self.terms).isEmpty)
        var terms = Self.terms
        terms.annualRatePpm = 35_000
        terms.installment = .contract(amount: 40_000)
        let patch = LoanLink.patch(loan, name: "Mutuo casa", note: " ", terms: terms)
        #expect(patch == LoanPatch(name: "Mutuo casa", note: "", annualRatePpm: 35_000, installment: .contract(amount: 40_000)))
    }

    // MARK: - What may be linked

    @Test("A loan may link the active expense templates no other loan links, its own included, archived loans holding theirs")
    func linkable() {
        let templates = [
            Self.template("free"),
            Self.template("mine"),
            Self.template("theirs"),
            Self.template("archivedLoan's"),
            Self.template("salary", kind: .income),
            Self.template("archived", archived: true),
        ]
        let loans = [
            Self.loan("me", linking: "mine"),
            Self.loan("other", linking: "theirs"),
            Self.loan("old", linking: "archivedLoan's", archived: true),
        ]
        #expect(LoanLink.linkable(templates, loans: loans, for: loans[0]).map(\.id) == ["free", "mine"])
        #expect(LoanLink.linkable(templates, loans: loans, for: nil).map(\.id) == ["free"])
    }

    // MARK: - What an early repayment can take

    @Test("A repayment the schedule cut to the debt, or dated after the payoff, asks for too much; other warnings do not")
    func overpays() {
        var schedule = Self.schedule
        schedule.warnings = [.prepaymentClipped(eventId: "new"), .balloon(amount: 5), .installmentBelowInterest(number: 2)]
        #expect(LoanLink.overpays(schedule, eventId: "new"))
        #expect(!LoanLink.overpays(schedule, eventId: "old"))
        schedule.warnings = [.eventAfterEnd(eventId: "new")]
        #expect(LoanLink.overpays(schedule, eventId: "new"))
        schedule.warnings = [.balloon(amount: 5), .prepaymentClipped(eventId: "old")]
        #expect(!LoanLink.overpays(schedule, eventId: "new"))
    }

    @Test("The debt on a day is the residual after the last installment due by then; before the first, the amount less earlier repayments")
    func residual() {
        var schedule = Self.schedule
        schedule.rows[0].residual = 66_000
        schedule.rows[1].residual = 33_950
        schedule.rows[2].residual = 0
        let none: [LoanEventView] = []
        #expect(LoanLink.residual(on: "2026-01-05", schedule: schedule, terms: Self.terms, events: none) == 66_000)
        #expect(LoanLink.residual(on: "2026-02-04", schedule: schedule, terms: Self.terms, events: none) == 66_000)
        #expect(LoanLink.residual(on: "2026-02-05", schedule: schedule, terms: Self.terms, events: none) == 33_950)
        #expect(LoanLink.residual(on: "2026-09-01", schedule: schedule, terms: Self.terms, events: none) == 0)
        #expect(LoanLink.residual(on: "2025-12-01", schedule: schedule, terms: Self.terms, events: none) == 100_000)
        // Two repayments before the first installment, the second cut to
        // what the first left; a rate change listed there counts nothing.
        let early = [
            LoanEventView(id: "a", event: .prepayment(date: "2025-11-01", amount: 30_000, mode: .lower, fee: 0)),
            LoanEventView(id: "b", event: .prepayment(date: "2025-11-15", amount: 90_000, mode: .lower, fee: 0)),
            LoanEventView(id: "c", event: .rateChange(date: "2025-11-20", annualRatePpm: 1, installment: .computed)),
        ]
        schedule.eventIdsBeforeFirst = ["a"]
        #expect(LoanLink.residual(on: "2025-12-01", schedule: schedule, terms: Self.terms, events: early) == 70_000)
        schedule.eventIdsBeforeFirst = ["a", "b", "c"]
        #expect(LoanLink.residual(on: "2025-12-01", schedule: schedule, terms: Self.terms, events: early) == 0)
    }

    // MARK: - Events and installments by period

    @Test("Events go to the core by date, then by id")
    func ordered() {
        let events = [
            LoanEventView(id: "b", event: .prepayment(date: "2026-02-01", amount: 100, mode: .shorten, fee: 0)),
            LoanEventView(id: "c", event: .rateChange(date: "2026-01-10", annualRatePpm: 1, installment: .computed)),
            LoanEventView(id: "a", event: .rateChange(date: "2026-02-01", annualRatePpm: 1, installment: .computed)),
        ]
        #expect(LoanLink.ordered(events).map(\.id) == ["c", "a", "b"])
    }

    @Test("The installments are found by template and due date, the key of a due period")
    func index() {
        let index = LinkedInstallment.index([
            LinkedInstallment(recurringId: "t", dueDate: "2026-01-05", amount: 34_200),
            LinkedInstallment(recurringId: "t", dueDate: "2026-02-05", amount: 34_100),
            LinkedInstallment(recurringId: "t", dueDate: "2026-02-05", amount: 1),
        ])
        #expect(index.count == 2)
        #expect(index[DuePeriod.key("t", "2026-02-05")] == 34_100)
        let period = DuePeriod(template: Self.template("t"), date: "2026-01-05")
        #expect(index[period.id] == 34_200)
    }
}
