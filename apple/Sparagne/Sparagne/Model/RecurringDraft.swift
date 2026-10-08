import Foundation
import SparagneCore

/// The fields of the Ricorrenze inspector (`docs/v2/UI.md` §2.5), for a new
/// template or one being edited. A plain struct with no view in it, so what
/// gets saved can be tested on its own.
///
/// Editing sends only what changed (`patch(against:currency:)`), the
/// convention `RecurringPatch` shares with `TransactionPatch`
/// (`docs/v2/ARCH.md` §4). The patch has no `kind`, and cannot set a wallet
/// or an envelope back to none (`nil` means "unchanged"), so the draft never
/// tries to: the kind is fixed once the template exists, and "any wallet" or
/// Unallocated are offered only to a template that has none yet
/// (`mayClearWallet`, `mayClearFlow`).
struct RecurringDraft: Equatable {
    /// `Frequency` as a flat choice, the segmented Giorno / Settimana / Mese
    /// / Anno: the day fields of the other cadences are kept while the user
    /// tries them, so switching back loses nothing.
    enum Cadence: String, CaseIterable, Identifiable, Hashable {
        case daily, weekly, monthly, yearly

        var id: String { rawValue }

        /// What the interval counts: "ogni 2 settimane".
        var unit: ScheduleUnit {
            switch self {
            case .daily: .day
            case .weekly: .week
            case .monthly: .month
            case .yearly: .year
            }
        }
    }

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
    var cadence: Cadence = .monthly
    var interval = 1
    /// ISO: Monday = 1 ... Sunday = 7.
    var weekday = 1
    var monthDay = 1
    var yearMonth = 1
    var yearDay = 1
    var startDate: NaiveDate
    /// "Fine: Mai / Il giorno". The date is kept while the switch says Mai,
    /// so turning it back on brings the same day back.
    var hasEndDate = false
    var endDate: NaiveDate
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
        startDate = today
        endDate = today
        if let parts = NaiveDay.components(today) {
            weekday = parts.isoWeekday
            monthDay = parts.day
            yearMonth = parts.month
            yearDay = parts.day
        }
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

        let schedule = template.schedule
        startDate = schedule.startDate
        hasEndDate = schedule.endDate != nil
        endDate = schedule.endDate ?? schedule.startDate
        interval = Int(schedule.interval)
        // The other cadences' days start from the start date, as a new
        // template's do from today.
        if let parts = NaiveDay.components(schedule.startDate) {
            weekday = parts.isoWeekday
            monthDay = parts.day
            yearMonth = parts.month
            yearDay = parts.day
        }
        switch schedule.frequency {
        case .daily:
            cadence = .daily
        case .weekly(let day):
            cadence = .weekly
            weekday = Int(day)
        case .monthly(let day):
            cadence = .monthly
            monthDay = Int(day)
        case .yearly(let month, let day):
            cadence = .yearly
            yearMonth = Int(month)
            yearDay = Int(day)
        }
    }

    /// A new template written from `template`: the same fields, its owner
    /// among them, running, but starting today. Starting where the original
    /// did would put every period since then due at once, and an end date
    /// already behind today would make the copy invalid, so it is dropped.
    init(duplicating template: RecurringView, today: NaiveDate) {
        self.init(template: template)
        enabled = true
        startDate = today
        if let parts = NaiveDay.components(today) {
            weekday = cadence == .weekly ? weekday : parts.isoWeekday
            monthDay = cadence == .monthly ? monthDay : parts.day
            yearMonth = cadence == .yearly ? yearMonth : parts.month
            yearDay = cadence == .yearly ? yearDay : parts.day
        }
        if hasEndDate, endDate < today { hasEndDate = false }
        endDate = hasEndDate ? endDate : today
    }

    // MARK: - What the fields say

    var frequency: Frequency {
        switch cadence {
        case .daily: .daily
        case .weekly: .weekly(weekday: Self.byte(weekday))
        case .monthly: .monthly(day: Self.byte(monthDay))
        case .yearly: .yearly(month: Self.byte(yearMonth), day: Self.byte(yearDay))
        }
    }

    /// The schedule as typed. Not checked here: an interval of zero or a
    /// day 32 go through as they are, for the core to refuse
    /// (`scheduleOccurrences`) and the inspector to say so.
    var schedule: Schedule {
        Schedule(
            frequency: frequency,
            interval: UInt32(clamping: max(interval, 0)),
            startDate: startDate,
            endDate: hasEndDate ? endDate : nil
        )
    }

    /// The amount in minor units, or `nil` while the field does not hold one.
    func amount(currency: Currency) -> Int64? {
        try? parseMoney(text: amountText, currency: currency)
    }

    private var trimmedCategory: String { category.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var trimmedNote: String { note.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var trimmedOwner: String { owner.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// Whether Salva or Crea may go: an amount above zero, and a schedule the
    /// core would take (the same rules as `Schedule::validate` in
    /// `core/src/recurring.rs`). The inspector also asks the core itself,
    /// through the preview of the next dates.
    func isValid(currency: Currency) -> Bool {
        guard let amount = amount(currency: currency), amount > 0 else { return false }
        guard interval >= 1 else { return false }
        switch cadence {
        case .daily: break
        case .weekly: guard (1...7).contains(weekday) else { return false }
        case .monthly: guard (1...31).contains(monthDay) else { return false }
        case .yearly: guard (1...12).contains(yearMonth), (1...31).contains(yearDay) else { return false }
        }
        if hasEndDate, endDate < startDate { return false }
        return kind == .expense || kind == .income
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
        if schedule != template.schedule { patch.schedule = schedule }
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
            schedule: schedule,
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

    /// The day fields are small; anything out of range is caught by
    /// `isValid` before it is sent, and clamping keeps the conversion from
    /// trapping on the way.
    private static func byte(_ value: Int) -> UInt8 { UInt8(clamping: value) }
}
