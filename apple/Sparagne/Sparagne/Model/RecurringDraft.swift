import Foundation
import SparagneCore

/// The fields of the Ricorrenze inspector, for a new
/// template or one being edited. A plain struct with no view in it, so what
/// gets saved can be tested on its own.
///
/// Editing sends only what changed (`patch(against:currency:)`), the
/// convention `RecurringPatch` shares with `TransactionPatch`.
/// The patch has no `kind`, and cannot set a wallet
/// or an envelope back to none (`nil` means "unchanged"), so the draft never
/// tries to: the kind is fixed once the template exists, and "any wallet" or
/// Unallocated are offered only to a template that has none yet
/// (`mayClearWallet`, `mayClearFlow`).
struct RecurringDraft: Equatable {
    /// Only `.income` or `.expense`: a template is never a refund or a transfer.
    var kind: TransactionKind = .expense
    /// What the amount field holds, parsed by the core (`parseMoney`).
    var amountText = ""
    /// `nil` = any active wallet when a period is recorded.
    var walletId: Uuid?
    /// `nil` = Unallocated.
    var flowId: Uuid?
    var category = ""
    var note = ""
    /// Quando: the cadence and its day, the interval, the start and the end.
    var when: ScheduleDraft
    var enabled = true
    /// Whose template it is, the person every period it records is for: one
    /// of `AppStore.assignablePeople`. Blank is the author, as the core
    /// reads it.
    var owner = ""

    /// A new template: monthly, from today, on today's day, owned by
    /// `owner`, the author who creates it unless the picker says otherwise.
    /// A template that starts today has today as its first period, which is
    /// what "every month from today" says.
    init(today: NaiveDate, owner: String = "") {
        self.owner = owner
        when = ScheduleDraft(today: today)
        // The End field offers today, wherever the start moves.
        when.shownEndDate = today
    }

    /// The fields of a saved template, as they are.
    init(template: RecurringView) {
        kind = template.kind
        // With the decimal comma the window writes amounts in; `parseMoney`
        // reads either separator.
        amountText = LedgerMoney.editable(template.amount).replacingOccurrences(of: ".", with: ",")
        walletId = template.walletId
        flowId = template.flowId
        category = template.category ?? ""
        note = template.note ?? ""
        enabled = template.enabled
        owner = template.owner
        when = ScheduleDraft(schedule: template.schedule)
        // The End field offers the saved end, or with none the saved start,
        // wherever the start moves.
        when.shownEndDate = template.schedule.endDate ?? template.schedule.startDate
    }

    /// A new template written from `template`: the same fields, its owner
    /// among them, running, but starting today. Starting where the original
    /// did would put every period since then due at once, and an end date
    /// already behind today would make the copy invalid, so it is dropped.
    init(duplicating template: RecurringView, today: NaiveDate) {
        self.init(template: template)
        enabled = true
        when.startDate = today
        if let parts = NaiveDay.components(today) {
            if when.cadence != .weekly { when.weekday = parts.isoWeekday }
            if when.cadence != .monthly { when.monthDay = parts.day }
            if when.cadence != .yearly {
                when.yearMonth = parts.month
                when.yearDay = parts.day
            }
        }
        if let end = when.endDate, end < today { when.hasEndDate = false }
        if !when.hasEndDate { when.shownEndDate = today }
    }

    // MARK: - What the fields say

    /// The schedule as typed, not checked (`ScheduleDraft.schedule`).
    var schedule: Schedule { when.schedule }

    /// The amount in minor units, or `nil` while the field does not hold one.
    func amount(currency: Currency) -> Int64? {
        try? parseMoney(text: amountText, currency: currency)
    }

    private var trimmedCategory: String { category.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var trimmedNote: String { note.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var trimmedOwner: String { owner.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// Whether Salva or Crea may go: an amount above zero, and a schedule the
    /// core would take (`ScheduleDraft.isValid`). The inspector also asks the
    /// core itself, through the preview of the next dates.
    func isValid(currency: Currency) -> Bool {
        guard let amount = amount(currency: currency), amount > 0 else { return false }
        return when.isValid && (kind == .expense || kind == .income)
    }

    // MARK: - Editing a saved template

    /// Whether "any wallet" may be chosen: only while the template has no
    /// wallet, since a patch cannot take one away.
    static func mayClearWallet(_ template: RecurringView?) -> Bool { template?.walletId == nil }

    /// Whether Unallocated may be chosen, for the same reason.
    static func mayClearFlow(_ template: RecurringView?) -> Bool { template?.flowId == nil }

    /// Sets the wallet, refusing "none" on a template that has one.
    mutating func setWallet(_ id: Uuid?, template: RecurringView?) {
        if id == nil, !Self.mayClearWallet(template) { return }
        walletId = id
    }

    /// Sets the envelope, refusing Unallocated on a template that has one.
    mutating func setFlow(_ id: Uuid?, template: RecurringView?) {
        if id == nil, !Self.mayClearFlow(template) { return }
        flowId = id
    }

    /// The fields that differ from `template`, and nothing else. Never the
    /// kind, and never a wallet or envelope set back to none. An amount that
    /// does not parse, or is not above zero, is left out: Salva is off then
    /// anyway (`isValid`).
    func patch(against template: RecurringView, currency: Currency) -> RecurringPatch {
        var patch = RecurringPatch()
        if let amount = amount(currency: currency), amount > 0, amount != template.amount {
            patch.amount = amount
        }
        if let walletId, walletId != template.walletId { patch.walletId = walletId }
        if let flowId, flowId != template.flowId { patch.flowId = flowId }
        // Blank clears both on the core's side ("Uncategorized", no note),
        // which is what an emptied field means.
        if trimmedCategory != (template.category ?? "") { patch.category = trimmedCategory }
        if trimmedNote != (template.note ?? "") { patch.note = trimmedNote }
        if when.schedule != template.schedule { patch.schedule = when.schedule }
        if enabled != template.enabled { patch.enabled = enabled }
        // Only a change: the patch's blank would mean "back to the author",
        // and nobody picked that.
        if !trimmedOwner.isEmpty, trimmedOwner != template.owner { patch.owner = trimmedOwner }
        return patch
    }

    /// Whether the inspector has something to save or throw away: a field
    /// in the patch, or an amount that no longer reads as the saved one
    /// (half typed, or emptied), which the patch cannot carry.
    func isDirty(against template: RecurringView, currency: Currency) -> Bool {
        amount(currency: currency) != template.amount || !patch(against: template, currency: currency).isEmpty
    }

    // MARK: - Creating a template

    /// The arguments of `AppStore.createRecurring`, or `nil` while the draft
    /// is not valid. Blank category and note are left out, as the core
    /// stores none for them.
    func creation(currency: Currency) -> Creation? {
        guard isValid(currency: currency), let amount = amount(currency: currency) else { return nil }
        return Creation(
            kind: kind,
            amount: amount,
            walletId: walletId,
            flowId: flowId,
            category: trimmedCategory.isEmpty ? nil : trimmedCategory,
            note: trimmedNote.isEmpty ? nil : trimmedNote,
            schedule: when.schedule,
            owner: trimmedOwner.isEmpty ? nil : trimmedOwner
        )
    }

    struct Creation: Equatable {
        let kind: TransactionKind
        let amount: Int64
        let walletId: Uuid?
        let flowId: Uuid?
        let category: String?
        let note: String?
        let schedule: Schedule
        /// `nil` = the author; `AppStore.createRecurring` also sends the
        /// author as `nil`.
        let owner: String?
    }
}

/// `when`'s fields where the inspector and the tests read them before it
/// held them. They go away once Ricorrenze and Riparto share one Quando
/// editor bound to `when`.
extension RecurringDraft {
    typealias Cadence = ScheduleDraft.Cadence

    var cadence: Cadence {
        get { when.cadence }
        set { when.cadence = newValue }
    }

    var interval: Int {
        get { when.interval }
        set { when.interval = newValue }
    }

    var weekday: Int {
        get { when.weekday }
        set { when.weekday = newValue }
    }

    var monthDay: Int {
        get { when.monthDay }
        set { when.monthDay = newValue }
    }

    var yearMonth: Int {
        get { when.yearMonth }
        set { when.yearMonth = newValue }
    }

    var yearDay: Int {
        get { when.yearDay }
        set { when.yearDay = newValue }
    }

    var startDate: NaiveDate {
        get { when.startDate }
        set { when.startDate = newValue }
    }

    var hasEndDate: Bool {
        get { when.hasEndDate }
        set { when.hasEndDate = newValue }
    }

    /// The End field's day, on or off: `when.shownEndDate`, not the
    /// schedule's `when.endDate`.
    var endDate: NaiveDate {
        get { when.shownEndDate }
        set { when.shownEndDate = newValue }
    }
}
