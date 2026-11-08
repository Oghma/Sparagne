import Foundation
import SparagneCore

/// How a new loan's installments reach the ledger.
enum LoanPayment: Equatable, Sendable {
    /// Nothing records them: the user types them, or the bank's statement
    /// brings them.
    case none
    /// A new monthly expense template, made with the loan in one batch.
    case create(LoanLedgerTarget)
    /// An expense template of the vault that no other loan links.
    case link(Uuid)
}

/// Where a loan's money is recorded in the ledger: the linked template's
/// installments, or an early repayment's expense. `nil` wallet = the only
/// active one, `nil` flow = Unallocated, as for any entry.
struct LoanLedgerTarget: Equatable, Sendable {
    var walletId: Uuid?
    var flowId: Uuid?
    /// Free text, resolved by the core; blank = Uncategorized.
    var category: String
    /// Who the rows are for; `nil` = the author.
    var owner: String?
}

/// The Patrimonio tab's commands. Every write that changes a linked loan's
/// installment or end also brings its template in line in the same batch
/// (`UpdateRecurring` of amount and end date), so Spese fisse and the
/// Ricorrenze tab say the current installment.
///
/// Loans have no undo, like the recurring templates they drive: every write
/// goes through `apply(envelopes:in:)`, one batch on the vault on screen,
/// refused before the core on a vault only read.
extension AppStore {
    /// Makes a loan, with its template when `payment` asks for one. Says
    /// whether it went through; a refusal is already on the alert.
    ///
    /// A new template is minted first, so the loan names it in the same
    /// batch: monthly on the first installment's day, until the last one,
    /// for the installment and its fee (`LoanLink.template`). A template
    /// linked as it is is brought in line like after any later write.
    @discardableResult
    func createLoan(name: String, note: String?, terms: LoanTerms, payment: LoanPayment) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly() else { return false }
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let note = LoanLink.note(note)
        switch payment {
        case .none:
            return await apply([.createLoan(name: name, note: note, terms: terms, recurringId: nil)], in: vault.id)
        case .link(let recurringId):
            let loan = await core.envelope(
                vaultId: vault.id,
                .createLoan(name: name, note: note, terms: terms, recurringId: recurringId)
            )
            return await applyLoanWrite([loan], terms: terms, events: [], linkedTo: recurringId, in: vault.id)
        case .create(let target):
            guard let command = await loanTemplate(named: name, terms: terms, events: [], target: target) else {
                return false
            }
            let template = await core.envelope(vaultId: vault.id, command)
            let loan = await core.envelope(
                vaultId: vault.id,
                .createLoan(name: name, note: note, terms: terms, recurringId: template.id)
            )
            return await apply(envelopes: [template, loan], in: vault.id)
        }
    }

    /// Saves the loan's name, note and terms as edited: only the fields that
    /// changed, and nothing when none did.
    @discardableResult
    func updateLoan(_ loan: LoanView, name: String, note: String?, terms: LoanTerms) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly() else { return false }
        let patch = LoanLink.patch(loan, name: name, note: note, terms: terms)
        guard !patch.isEmpty else { return true }
        let update = await core.envelope(vaultId: vault.id, .updateLoan(loanId: loan.id, patch: patch))
        return await applyLoanWrite([update], terms: terms, events: loan.events, linkedTo: loan.recurringId, in: vault.id)
    }

    /// Adds a rate change or an early repayment; with `expense`, the
    /// repayment and its penalty are recorded as one expense in the same
    /// batch, before the event, dated on its day: `[Expense, AddLoanEvent,
    /// UpdateRecurring?]`. A rate change records nothing.
    @discardableResult
    func addLoanEvent(_ event: LoanEvent, to loan: LoanView, expense: LoanLedgerTarget?) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly() else { return false }
        var envelopes: [CommandEnvelope] = []
        if let expense, case .prepayment(let date, let amount, _, let fee) = event {
            let now = Date()
            let entry = Entry(
                amount: amount + fee,
                walletId: expense.walletId,
                flowId: expense.flowId,
                category: LoanLink.category(expense.category),
                note: String(localized: "\(loan.name): early repayment"),
                occurredAt: Self.stamp(day: CoreDate.localDay(date) ?? now, likeTimeOf: now),
                person: explicitPerson(expense.owner)
            )
            envelopes.append(await core.envelope(vaultId: vault.id, .expense(entry)))
        }
        // Minted before the preview: the event's id is the command's, and
        // the schedule orders events of one day by it.
        let add = await core.envelope(vaultId: vault.id, .addLoanEvent(loanId: loan.id, event: event))
        envelopes.append(add)
        let events = loan.events + [LoanEventView(id: add.id, event: event)]
        return await applyLoanWrite(envelopes, terms: loan.terms, events: events, linkedTo: loan.recurringId, in: vault.id)
    }

    /// Takes an event away; the linked template goes back to the installment
    /// the schedule has without it.
    @discardableResult
    func removeLoanEvent(_ eventId: Uuid, from loan: LoanView) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly() else { return false }
        let remove = await core.envelope(vaultId: vault.id, .removeLoanEvent(loanId: loan.id, eventId: eventId))
        let events = loan.events.filter { $0.id != eventId }
        return await applyLoanWrite([remove], terms: loan.terms, events: events, linkedTo: loan.recurringId, in: vault.id)
    }

    /// Links the loan to an expense template, or unlinks it with `nil`. The
    /// template linked is brought in line at once: its amount and end from
    /// then on are the schedule's.
    @discardableResult
    func linkLoan(_ loan: LoanView, to recurringId: Uuid?) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly() else { return false }
        guard recurringId != loan.recurringId else { return true }
        let link = await core.envelope(vaultId: vault.id, .setLoanRecurring(loanId: loan.id, recurringId: recurringId))
        return await applyLoanWrite([link], terms: loan.terms, events: loan.events, linkedTo: recurringId, in: vault.id)
    }

    /// Makes the loan's template from its schedule and links it, in one
    /// batch.
    @discardableResult
    func createLoanRecurring(for loan: LoanView, target: LoanLedgerTarget) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly() else { return false }
        guard let command = await loanTemplate(named: loan.name, terms: loan.terms, events: loan.events, target: target) else {
            return false
        }
        let template = await core.envelope(vaultId: vault.id, command)
        let link = await core.envelope(vaultId: vault.id, .setLoanRecurring(loanId: loan.id, recurringId: template.id))
        return await apply(envelopes: [template, link], in: vault.id)
    }

    /// Archives the loan, or restores it; a restored loan drives its
    /// template again, brought in line.
    @discardableResult
    func setLoanArchived(_ loan: LoanView, _ archived: Bool) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly() else { return false }
        guard archived != loan.archived else { return true }
        if archived { return await apply([.archiveLoan(loanId: loan.id)], in: vault.id) }
        let restore = await core.envelope(vaultId: vault.id, .restoreLoan(loanId: loan.id))
        return await applyLoanWrite([restore], terms: loan.terms, events: loan.events, linkedTo: loan.recurringId, in: vault.id)
    }

    /// One saved loan's whole schedule, `nil` when the core refuses.
    func loanSchedule(_ loanId: Uuid) async -> LoanSchedule? {
        guard let vault = currentVault else { return nil }
        return try? await core.read { try $0.loanSchedule(vaultId: vault.id, loanId: loanId) }
    }

    /// The schedule of a form not saved yet, `nil` when the terms are refused.
    func previewLoan(terms: LoanTerms, events: [LoanEventView]) async -> LoanSchedule? {
        let events = LoanLink.ordered(events)
        return try? await core.read { try $0.previewLoan(terms: terms, events: events) }
    }

    /// The active expense templates a loan may link: none linked by another
    /// loan, `loan`'s own included.
    func linkableRecurring(for loan: LoanView?) -> [RecurringView] {
        LoanLink.linkable(recurringTemplates, loans: loans, for: loan)
    }

    // MARK: - Recording the installments

    /// What recording the period of `recurringId` due on `periodDate` writes
    /// when a loan sets it: the schedule's installment with its fee. `nil`
    /// for any other period, which records the template's amount.
    func installmentAmount(recurringId: Uuid, periodDate: NaiveDate) -> Int64? {
        installmentsByPeriod[DuePeriod.key(recurringId, periodDate)]
    }

    /// The amount a due period shows, the one Registra records.
    func amount(of period: DuePeriod) -> Int64 {
        installmentAmount(recurringId: period.template.id, periodDate: period.date) ?? period.template.amount
    }

    // MARK: - Keeping the template in line

    /// `envelopes` with, when the loan is linked to a template still active,
    /// the `UpdateRecurring` that brings it in line with the schedule of
    /// `terms` and `events` (the loan as the batch leaves it), then applied
    /// as one batch. A schedule the core cannot work out is its refusal, on
    /// the alert, and nothing is written.
    private func applyLoanWrite(
        _ envelopes: [CommandEnvelope],
        terms: LoanTerms,
        events: [LoanEventView],
        linkedTo recurringId: Uuid?,
        in vaultId: Uuid
    ) async -> Bool {
        var envelopes = envelopes
        if let recurringId, let template = recurringTemplates.first(where: { $0.id == recurringId && !$0.archived }) {
            guard let schedule = await loanPlan(terms: terms, events: events) else { return false }
            let patch = LoanLink.alignment(of: template, to: schedule, today: CoreDate.day(Date()))
            if !patch.isEmpty {
                envelopes.append(await core.envelope(
                    vaultId: vaultId,
                    .updateRecurring(recurringId: template.id, patch: patch)
                ))
            }
        }
        return await apply(envelopes: envelopes, in: vaultId)
    }

    /// The `CreateRecurring` of a loan's new template, or `nil` with the
    /// reason on the alert.
    private func loanTemplate(
        named name: String,
        terms: LoanTerms,
        events: [LoanEventView],
        target: LoanLedgerTarget
    ) async -> Command? {
        guard let schedule = await loanPlan(terms: terms, events: events) else { return nil }
        guard let command = LoanLink.template(
            named: name,
            terms: terms,
            schedule: schedule,
            target: target,
            owner: explicitPerson(target.owner),
            today: CoreDate.day(Date())
        ) else {
            presentedError = AppError(
                code: "invalid_command",
                message: String(localized: "Every installment of the loan is already due: there is nothing left to record"),
                headline: String(localized: "No template was made")
            )
            return nil
        }
        return command
    }

    /// The schedule of `terms` and `events`, or `nil` with the core's
    /// refusal on the alert.
    private func loanPlan(terms: LoanTerms, events: [LoanEventView]) async -> LoanSchedule? {
        let events = LoanLink.ordered(events)
        do {
            return try await core.read { try $0.previewLoan(terms: terms, events: events) }
        } catch {
            report(error)
            return nil
        }
    }
}

/// A loan's template in step with its schedule, worked out without the
/// store: what a write appends, and what the tests check on hand-made
/// schedules.
enum LoanLink {
    /// The installment a linked template records from now on: the first
    /// installment due after `today` with its fee, the last one once every
    /// installment is due. `nil` for an empty schedule.
    static func currentInstallment(_ schedule: LoanSchedule, today: NaiveDate) -> Int64? {
        guard let row = schedule.rows.first(where: { $0.dueDate > today }) ?? schedule.rows.last else { return nil }
        return row.payment + row.fee
    }

    /// What `template` needs to say `schedule`: the current installment
    /// (`currentInstallment`) and the last due date as its end. Its start,
    /// cadence and everything else stay: a template linked as it was keeps
    /// the periods it already recorded. An end before the template's start
    /// is left out, since the core would refuse the schedule.
    static func alignment(of template: RecurringView, to schedule: LoanSchedule, today: NaiveDate) -> RecurringPatch {
        var patch = RecurringPatch()
        if let amount = currentInstallment(schedule, today: today), amount > 0, amount != template.amount {
            patch.amount = amount
        }
        if let end = schedule.rows.last?.dueDate, end != template.schedule.endDate, end >= template.schedule.startDate {
            var moved = template.schedule
            moved.endDate = end
            patch.schedule = moved
        }
        return patch
    }

    /// The first period a new template records: the first installment, or
    /// for a loan already running the first one due today or later, so the
    /// installments the bank took before the template existed do not all
    /// come due at once. `nil` when every one is behind.
    static func templateStart(_ schedule: LoanSchedule, firstDue: NaiveDate, today: NaiveDate) -> NaiveDate? {
        guard firstDue < today else { return firstDue }
        return schedule.rows.first { $0.dueDate >= today }?.dueDate
    }

    /// The `CreateRecurring` of a loan's template: an expense of the current
    /// installment with its fee, monthly on the first installment's day from
    /// `templateStart` to the last due date, noted with the loan's name, for
    /// `owner` (`nil` = the author). `nil` when nothing is left to record.
    static func template(
        named name: String,
        terms: LoanTerms,
        schedule: LoanSchedule,
        target: LoanLedgerTarget,
        owner: String?,
        today: NaiveDate
    ) -> Command? {
        guard let last = schedule.rows.last?.dueDate,
              let start = templateStart(schedule, firstDue: terms.firstDue, today: today),
              let amount = currentInstallment(schedule, today: today), amount > 0,
              let cadence = LoanSchedules.monthly(firstDue: terms.firstDue, start: start, end: last)
        else { return nil }
        return .createRecurring(
            transactionKind: .expense,
            amount: amount,
            walletId: target.walletId,
            flowId: target.flowId,
            category: category(target.category),
            note: name.trimmingCharacters(in: .whitespacesAndNewlines),
            schedule: cadence,
            owner: owner
        )
    }

    /// The fields of `loan` that differ from what the form says. A blank
    /// name is sent, for the core to refuse; a blank note clears it.
    static func patch(_ loan: LoanView, name: String, note: String?, terms: LoanTerms) -> LoanPatch {
        var patch = LoanPatch()
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if name != loan.name { patch.name = name }
        let note = (note ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if note != (loan.note ?? "") { patch.note = note }
        let saved = loan.terms
        if terms.principal != saved.principal { patch.principal = terms.principal }
        if terms.annualRatePpm != saved.annualRatePpm { patch.annualRatePpm = terms.annualRatePpm }
        if terms.months != saved.months { patch.months = terms.months }
        if terms.firstDue != saved.firstDue { patch.firstDue = terms.firstDue }
        if terms.installment != saved.installment { patch.installment = terms.installment }
        if terms.upfrontFees != saved.upfrontFees { patch.upfrontFees = terms.upfrontFees }
        if terms.installmentFee != saved.installmentFee { patch.installmentFee = terms.installmentFee }
        return patch
    }

    /// The active expense templates of `templates` that no loan but `loan`
    /// links, archived loans included: the core keeps a template to one loan.
    static func linkable(_ templates: [RecurringView], loans: [LoanView], for loan: LoanView?) -> [RecurringView] {
        let taken = Set(loans.filter { $0.id != loan?.id }.compactMap(\.recurringId))
        return templates.filter { !$0.archived && $0.kind == .expense && !taken.contains($0.id) }
    }

    /// Events by date, then by id, as `LoanView.events` lists them.
    static func ordered(_ events: [LoanEventView]) -> [LoanEventView] {
        events.sorted { (day(of: $0.event), $0.id) < (day(of: $1.event), $1.id) }
    }

    private static func day(of event: LoanEvent) -> NaiveDate {
        switch event {
        case .rateChange(let date, _, _), .prepayment(let date, _, _, _): date
        }
    }

    /// A trimmed note, `nil` when blank.
    static func note(_ text: String?) -> String? {
        let trimmed = (text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// A trimmed category, `nil` (Uncategorized) when blank.
    static func category(_ text: String) -> String? {
        note(text)
    }
}

extension LoanPatch {
    /// `UpdateLoan` refuses a patch that changes nothing.
    var isEmpty: Bool { self == LoanPatch() }
}

extension LinkedInstallment {
    /// `installments` by `DuePeriod.id`: one amount per template and due
    /// date, the first when the core lists one twice.
    static func index(_ installments: [LinkedInstallment]) -> [String: Int64] {
        Dictionary(
            installments.map { (DuePeriod.key($0.recurringId, $0.dueDate), $0.amount) },
            uniquingKeysWith: { first, _ in first }
        )
    }
}

extension DuePeriod {
    /// The id of the period of `templateId` due on `date`.
    static func key(_ templateId: Uuid, _ date: NaiveDate) -> String {
        "\(templateId)/\(date)"
    }
}
