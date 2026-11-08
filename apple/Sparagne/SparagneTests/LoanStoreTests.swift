import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The Patrimonio tab's commands on the store (`AppStore+Loans.swift`),
/// against the in-memory core: a loan made with a new template or linked to
/// one, the template kept at the schedule's installment and end by every
/// write in the same batch, an early repayment recorded as an expense, the
/// due periods recorded with the schedule's amount, and a vault only read.
/// The schedule's own figures are core/tests/loans.rs's; here they are read
/// back from the core and compared with what the store wrote.
struct LoanStoreTests {
    private static var today: NaiveDate { CoreDate.day(Date()) }

    private static func day(_ offset: Int) throws -> NaiveDate {
        try #require(NaiveDay.adding(offset, to: today))
    }

    /// Vault `Casa`, wallet `Conto` at 1.000,00, envelope `Casa` allowed
    /// below zero.
    private static func vault() async throws -> AppStore {
        let store = try await TestStore.make()
        await store.createEnvelope(name: "Casa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        #expect(store.presentedError == nil)
        return store
    }

    /// 10.000,00 at 7,5% in 60 months, 1,50 of fee per installment: 200,38
    /// each (B2's reference), 201,88 recorded.
    private static func kitchen(firstDue: NaiveDate) -> LoanTerms {
        LoanTerms(
            principal: 1_000_000,
            annualRatePpm: 75_000,
            months: 60,
            firstDue: firstDue,
            installment: .computed,
            upfrontFees: 0,
            installmentFee: 150
        )
    }

    private static func target(_ store: AppStore) throws -> LoanLedgerTarget {
        LoanLedgerTarget(walletId: nil, flowId: try TestStore.flow("Casa", in: store).id, category: "Prestiti", owner: nil)
    }

    private static func outbox(_ store: AppStore) async throws -> UInt32 {
        let vaultId = try #require(store.currentVault).id
        return try await store.core.read { try $0.syncState(vaultId: vaultId) }.outbox
    }

    private static func loan(_ store: AppStore, _ name: String) throws -> LoanView {
        try #require(store.loans.first { $0.name == name })
    }

    private static func template(_ store: AppStore, _ id: Uuid?) throws -> RecurringView {
        let id = try #require(id)
        return try #require(store.recurringTemplates.first { $0.id == id })
    }

    private static func schedule(_ store: AppStore, _ loan: LoanView) async throws -> LoanSchedule {
        try #require(await store.loanSchedule(loan.id))
    }

    /// What the template should say for `schedule` now: the installment
    /// after today with its fee, and the last due date.
    private static func expected(_ schedule: LoanSchedule) throws -> (amount: Int64, end: NaiveDate) {
        let row = try #require(schedule.rows.first { $0.dueDate > today } ?? schedule.rows.last)
        return (row.payment + row.fee, try #require(schedule.rows.last?.dueDate))
    }

    private static func monthly(on firstDue: NaiveDate) throws -> Frequency {
        .monthly(day: UInt8(try #require(NaiveDay.components(firstDue)).day))
    }

    private static func everyRow(_ store: AppStore) async throws -> [TransactionView] {
        try await VaultExporter.allTransactions(core: store.core, vaultId: try #require(store.currentVault).id)
    }

    /// A monthly expense template of 900,00 under Prestiti from `start`,
    /// open-ended, as typed by hand in the Ricorrenze tab.
    private static func handTemplate(_ store: AppStore, from start: NaiveDate) async throws -> RecurringView {
        let id = await store.createRecurring(
            kind: .expense,
            amount: 90_000,
            walletId: nil,
            flowId: try TestStore.flow("Casa", in: store).id,
            category: "Prestiti",
            note: "Rata cucina",
            schedule: Schedule(frequency: try Self.monthly(on: start), interval: 1, startDate: start, endDate: nil)
        )
        #expect(store.presentedError == nil)
        return try Self.template(store, id)
    }

    // MARK: - Making a loan

    @Test("A loan with a new template is one batch: an expense of the installment and its fee, monthly on the first due day, to the last one")
    func createWithTemplate() async throws {
        let store = try await Self.vault()
        let terms = Self.kitchen(firstDue: try Self.day(10))
        let before = try await Self.outbox(store)

        #expect(await store.createLoan(name: "Cucina", note: "Tasso fisso", terms: terms, payment: .create(try Self.target(store))))

        #expect(store.presentedError == nil)
        let loan = try Self.loan(store, "Cucina")
        #expect(loan.note == "Tasso fisso")
        #expect(loan.terms == terms)
        let template = try Self.template(store, loan.recurringId)
        let schedule = try await Self.schedule(store, loan)
        #expect(schedule.rows.first?.payment == 20_038)
        #expect(template.kind == .expense)
        #expect(template.amount == 20_188)
        #expect(template.flowId == (try TestStore.flow("Casa", in: store).id))
        #expect(template.category == "Prestiti")
        #expect(template.note == "Cucina")
        #expect(template.schedule == Schedule(
            frequency: try Self.monthly(on: terms.firstDue),
            interval: 1,
            startDate: terms.firstDue,
            endDate: try Self.expected(schedule).end
        ))
        #expect(try await Self.outbox(store) == before + 2)
        // What it records, due date by due date, is the schedule's.
        let first = try #require(schedule.rows.first)
        #expect(store.installmentAmount(recurringId: template.id, periodDate: first.dueDate) == 20_188)
        #expect(store.linkableRecurring(for: nil).allSatisfy { $0.id != template.id })
        #expect(store.linkableRecurring(for: loan).contains { $0.id == template.id })
    }

    @Test("A loan the core refuses leaves no template behind: the two go in one batch")
    func createIsOneBatch() async throws {
        let store = try await Self.vault()
        let before = try await Self.outbox(store)

        #expect(!(await store.createLoan(name: "  ", note: nil, terms: Self.kitchen(firstDue: try Self.day(10)), payment: .create(try Self.target(store)))))

        #expect(store.presentedError != nil)
        #expect(store.loans.isEmpty)
        #expect(store.recurringTemplates.isEmpty)
        #expect(try await Self.outbox(store) == before)
    }

    @Test("A loan linked to a template typed by hand takes it over: its amount and end become the schedule's")
    func createLinked() async throws {
        let store = try await Self.vault()
        let terms = Self.kitchen(firstDue: try Self.day(10))
        let typed = try await Self.handTemplate(store, from: terms.firstDue)
        let before = try await Self.outbox(store)

        #expect(await store.createLoan(name: "Cucina", note: nil, terms: terms, payment: .link(typed.id)))

        #expect(store.presentedError == nil)
        let loan = try Self.loan(store, "Cucina")
        #expect(loan.recurringId == typed.id)
        let template = try Self.template(store, typed.id)
        let expected = try Self.expected(try await Self.schedule(store, loan))
        #expect(template.amount == expected.amount)
        #expect(template.schedule.endDate == expected.end)
        #expect(template.schedule.startDate == typed.schedule.startDate)
        #expect(template.note == "Rata cucina")
        // CreateLoan and the UpdateRecurring that brings the template in line.
        #expect(try await Self.outbox(store) == before + 2)
    }

    @Test("A loan without a template, linked later, then unlinked; the template made from the inspector")
    func linkLater() async throws {
        let store = try await Self.vault()
        let terms = Self.kitchen(firstDue: try Self.day(10))
        #expect(await store.createLoan(name: "Cucina", note: nil, terms: terms, payment: .none))
        #expect(try Self.loan(store, "Cucina").recurringId == nil)
        #expect(store.recurringTemplates.isEmpty)
        let typed = try await Self.handTemplate(store, from: terms.firstDue)

        #expect(await store.linkLoan(try Self.loan(store, "Cucina"), to: typed.id))
        #expect(store.presentedError == nil)
        #expect(try Self.loan(store, "Cucina").recurringId == typed.id)
        #expect(try Self.template(store, typed.id).amount == 20_188)

        #expect(await store.linkLoan(try Self.loan(store, "Cucina"), to: nil))
        #expect(try Self.loan(store, "Cucina").recurringId == nil)
        #expect(store.linkedInstallments.isEmpty)

        #expect(await store.createLoanRecurring(for: try Self.loan(store, "Cucina"), target: try Self.target(store)))
        #expect(store.presentedError == nil)
        let made = try Self.template(store, try Self.loan(store, "Cucina").recurringId)
        #expect(made.id != typed.id)
        #expect(made.amount == 20_188)
        #expect(made.note == "Cucina")
    }

    // MARK: - Keeping the template in line

    @Test("An event that changes the installment updates the template in the same batch, and removing it restores it")
    func eventMovesTemplate() async throws {
        let store = try await Self.vault()
        let terms = Self.kitchen(firstDue: try Self.day(10))
        #expect(await store.createLoan(name: "Cucina", note: nil, terms: terms, payment: .create(try Self.target(store))))
        let templateId = try #require(try Self.loan(store, "Cucina").recurringId)
        #expect(try Self.template(store, templateId).amount == 20_188)
        let before = try await Self.outbox(store)

        // 2.000,00 repaid before the first installment, the end kept: every
        // installment is lower.
        let repayment = LoanEvent.prepayment(date: Self.today, amount: 200_000, mode: .lower, fee: 0)
        #expect(await store.addLoanEvent(repayment, to: try Self.loan(store, "Cucina"), expense: nil))

        #expect(store.presentedError == nil)
        let loan = try Self.loan(store, "Cucina")
        #expect(loan.events.map(\.event) == [repayment])
        let lowered = try Self.expected(try await Self.schedule(store, loan))
        #expect(lowered.amount < 20_188)
        #expect(try Self.template(store, templateId).amount == lowered.amount)
        #expect(try await Self.outbox(store) == before + 2)

        let eventId = try #require(loan.events.first?.id)
        #expect(await store.removeLoanEvent(eventId, from: loan))

        #expect(store.presentedError == nil)
        #expect(try Self.loan(store, "Cucina").events.isEmpty)
        #expect(try Self.template(store, templateId).amount == 20_188)
        #expect(try await Self.outbox(store) == before + 4)
    }

    @Test("Shortening the loan moves the template's end; an edit that changes nothing writes nothing")
    func editMovesEnd() async throws {
        let store = try await Self.vault()
        let terms = Self.kitchen(firstDue: try Self.day(10))
        #expect(await store.createLoan(name: "Cucina", note: nil, terms: terms, payment: .create(try Self.target(store))))
        let before = try await Self.outbox(store)

        #expect(await store.updateLoan(try Self.loan(store, "Cucina"), name: "Cucina", note: nil, terms: terms))
        #expect(try await Self.outbox(store) == before)

        var shorter = terms
        shorter.months = 48
        #expect(await store.updateLoan(try Self.loan(store, "Cucina"), name: "Cucina e bagno", note: nil, terms: shorter))

        #expect(store.presentedError == nil)
        let loan = try Self.loan(store, "Cucina e bagno")
        #expect(loan.terms.months == 48)
        let expected = try Self.expected(try await Self.schedule(store, loan))
        let template = try Self.template(store, loan.recurringId)
        #expect(template.schedule.endDate == expected.end)
        #expect(template.amount == expected.amount)
        #expect(template.amount > 20_188)
        #expect(try await Self.outbox(store) == before + 2)
    }

    @Test("An archived loan stays in the list, and a restored one drives its template again")
    func archiveAndRestore() async throws {
        let store = try await Self.vault()
        #expect(await store.createLoan(name: "Cucina", note: nil, terms: Self.kitchen(firstDue: try Self.day(10)), payment: .create(try Self.target(store))))

        #expect(await store.setLoanArchived(try Self.loan(store, "Cucina"), true))
        #expect(store.presentedError == nil)
        #expect(try Self.loan(store, "Cucina").archived)
        // An archived loan still holds its template: no other loan may link it.
        let templateId = try #require(try Self.loan(store, "Cucina").recurringId)
        #expect(store.linkableRecurring(for: nil).allSatisfy { $0.id != templateId })

        #expect(await store.setLoanArchived(try Self.loan(store, "Cucina"), false))
        #expect(store.presentedError == nil)
        #expect(!(try Self.loan(store, "Cucina").archived))
    }

    // MARK: - An early repayment's expense

    @Test("An early repayment recorded in the Mastro is one expense of the amount and its penalty, on its day, before the event, in one batch")
    func prepaymentExpense() async throws {
        let store = try await Self.vault()
        #expect(await store.createLoan(name: "Cucina", note: nil, terms: Self.kitchen(firstDue: try Self.day(10)), payment: .none))
        let before = try await Self.outbox(store)
        let repayment = LoanEvent.prepayment(date: Self.today, amount: 50_000, mode: .shorten, fee: 1_000)

        #expect(await store.addLoanEvent(repayment, to: try Self.loan(store, "Cucina"), expense: try Self.target(store)))

        #expect(store.presentedError == nil)
        #expect(try Self.loan(store, "Cucina").events.map(\.event) == [repayment])
        let note = String(localized: "\("Cucina"): early repayment")
        let row = try #require(try await Self.everyRow(store).first { $0.note == note })
        #expect(row.kind == .expense)
        #expect(row.amount == 51_000)
        #expect(row.flowId == (try TestStore.flow("Casa", in: store).id))
        #expect(row.category == "Prestiti")
        #expect(CoreDate.date(row.occurredAt).map { CoreDate.day($0) } == Self.today)
        #expect(try await Self.outbox(store) == before + 2)
    }

    @Test("A repayment the core refuses records no expense either")
    func refusedRepaymentRecordsNothing() async throws {
        let store = try await Self.vault()
        #expect(await store.createLoan(name: "Cucina", note: nil, terms: Self.kitchen(firstDue: try Self.day(10)), payment: .none))
        let before = try await Self.outbox(store)
        // Twenty years after a five-year loan's first installment.
        let late = LoanEvent.prepayment(date: try Self.day(7_300), amount: 50_000, mode: .shorten, fee: 0)

        #expect(!(await store.addLoanEvent(late, to: try Self.loan(store, "Cucina"), expense: try Self.target(store))))

        #expect(store.presentedError != nil)
        #expect(try Self.loan(store, "Cucina").events.isEmpty)
        #expect(try await Self.outbox(store) == before)
    }

    // MARK: - Recording the installments

    @Test("Registra and Registra tutte record each due period with the schedule's installment for that date, not the template's amount")
    func executionSendsTheSchedulesAmount() async throws {
        let store = try await Self.vault()
        // A loan running for 45 days: two installments due, the first on
        // its first day.
        let start = try Self.day(-45)
        let typed = try await Self.handTemplate(store, from: start)
        #expect(store.dueDates(of: typed.id).count == 2)
        #expect(await store.createLoan(name: "Cucina", note: nil, terms: Self.kitchen(firstDue: start), payment: .link(typed.id)))
        // From the first installment on the rate drops to 3%: the first one
        // accrued at 7,5%, the next ones less.
        let drop = LoanEvent.rateChange(date: start, annualRatePpm: 30_000, installment: .computed)
        #expect(await store.addLoanEvent(drop, to: try Self.loan(store, "Cucina"), expense: nil))
        #expect(store.presentedError == nil)

        let schedule = try await Self.schedule(store, try Self.loan(store, "Cucina"))
        let rows = Array(schedule.rows.prefix(2))
        let amounts = rows.map { $0.payment + $0.fee }
        #expect(amounts[0] == 20_188)
        #expect(amounts[1] < amounts[0])
        let template = try Self.template(store, typed.id)
        #expect(template.amount != amounts[0])
        let periods = store.duePeriods.filter { $0.template.id == typed.id }
        #expect(periods.map(\.date) == rows.map(\.dueDate))
        #expect(periods.map { store.amount(of: $0) } == amounts)

        await store.executeRecurring(typed.id, periodDate: rows[0].dueDate)
        #expect(store.presentedError == nil)
        await store.executeAllDueRecurring()
        #expect(store.presentedError == nil)
        #expect(store.dueDates(of: typed.id).isEmpty)

        let vaultId = try #require(store.currentVault).id
        let runs = try await store.core.read { try $0.recurringRuns(vaultId: vaultId, recurringId: typed.id) }
        let recorded = try await Self.everyRow(store)
        let byPeriod = try rows.map { row in
            let run = try #require(runs.first { $0.periodDate == row.dueDate })
            return try #require(recorded.first { $0.id == run.transactionId }).amount
        }
        #expect(byPeriod == amounts)
    }

    // MARK: - The template's kind

    @Test("A template a loan records through cannot change kind, and the menu says why")
    func kindChangeRefused() async throws {
        let store = try await Self.vault()
        #expect(await store.createLoan(name: "Cucina", note: nil, terms: Self.kitchen(firstDue: try Self.day(10)), payment: .create(try Self.target(store))))
        let templateId = try #require(try Self.loan(store, "Cucina").recurringId)

        #expect(await store.recurringKindRefusal(templateId) == .linkedToLoan)
        #expect(await store.changeRecurringKind(templateId) == nil)
        #expect(store.presentedError?.message == RecurringKindChange.Refusal.linkedToLoan.reason)
        #expect(try Self.template(store, templateId).kind == .expense)
        #expect(!(try Self.template(store, templateId).archived))
    }

    // MARK: - A vault only read

    @Test("A vault only read refuses every loan command before the core")
    func readOnly() async throws {
        let store = try await Self.vault()
        let vault = try #require(store.currentVault)
        store.setReadOnlyVaults([vault.id])
        let terms = Self.kitchen(firstDue: try Self.day(10))
        let loan = LoanView(
            id: "loan",
            name: "Cucina",
            note: nil,
            terms: terms,
            events: [LoanEventView(id: "event", event: .prepayment(date: Self.today, amount: 100, mode: .shorten, fee: 0))],
            recurringId: "template",
            archived: false,
            createdBy: "matteo"
        )
        let before = try await Self.outbox(store)
        let writes: [(String, () async -> Bool)] = [
            ("create", { await store.createLoan(name: "Cucina", note: nil, terms: terms, payment: .none) }),
            ("create with template", { await store.createLoan(name: "Cucina", note: nil, terms: terms, payment: .create(LoanLedgerTarget(walletId: nil, flowId: nil, category: "", owner: nil))) }),
            ("update", { await store.updateLoan(loan, name: "Altro", note: nil, terms: terms) }),
            ("add event", { await store.addLoanEvent(.prepayment(date: Self.today, amount: 100, mode: .lower, fee: 0), to: loan, expense: nil) }),
            ("remove event", { await store.removeLoanEvent("event", from: loan) }),
            ("link", { await store.linkLoan(loan, to: nil) }),
            ("link the same", { await store.linkLoan(loan, to: "template") }),
            ("make the template", { await store.createLoanRecurring(for: loan, target: LoanLedgerTarget(walletId: nil, flowId: nil, category: "", owner: nil)) }),
            ("archive", { await store.setLoanArchived(loan, true) }),
        ]
        for (name, write) in writes {
            #expect(!(await write()), "\(name)")
            #expect(store.presentedError?.code == "forbidden", "\(name)")
            store.presentedError = nil
        }
        #expect(store.loans.isEmpty)
        #expect(try await Self.outbox(store) == before)
    }
}
