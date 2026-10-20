import Foundation
import SparagneCore

/// A category the core suggested for a note, and the note it was for.
struct NoteSuggestion: Equatable, Sendable {
    let note: String
    let category: String
}

/// A cell that refused what was typed: the grid reports `underlying` and focuses `field`.
struct RowDraftError: Error {
    let field: RowField
    let underlying: Error
}

/// A new row's fields as the new line resolved them, ready for `AppStore.addRow`.
typealias RowEntry = (
    day: Date,
    flowId: Uuid?,
    category: String?,
    note: String,
    amount: Int64,
    walletId: Uuid?,
    kind: TransactionKind?,
    person: String?
)

/// The cells of the row being edited, strings until it commits.
struct RowDraft {
    var day = Date()
    /// Picked from the ENVELOPE cell's list, never typed (`EnvelopeSelectCell`).
    /// `nil` on the empty line is the sticky default; on a transfer it is
    /// nothing, since the cell does not open there.
    var flowId: Uuid?
    var category = ""
    var note = ""
    var wallet = ""
    /// A name typed, resolved against `AppStore.assignablePeople` when the
    /// row is saved. Blank leaves it to the default: the author on a new
    /// row, whoever it is for already on a stored one.
    var person = ""
    var amount = ""
    /// What the new line is written as. `nil` is the direction on screen's
    /// (`LedgerDirection.newRowKind`); a duplicate keeps its source's, so a
    /// refund copied into the line is saved as a refund, not an expense.
    var kind: TransactionKind?
    /// The category the vault's history files the note under, as the core
    /// suggested it for the note it was asked about. The empty line shows it
    /// in its CATEGORY cell while that is empty, and saves it if it still is.
    var suggestion: NoteSuggestion?

    nonisolated init() {}

    /// The suggestion, while it is still about the note in the cell: one
    /// keystroke in DESCRIZIONE and it no longer is.
    var suggestedCategory: String? {
        guard let suggestion, suggestion.note == note.trimmingCharacters(in: .whitespacesAndNewlines) else {
            return nil
        }
        return suggestion.category
    }

    init(row: TransactionRow, store: AppStore) {
        kind = row.kind
        day = row.occurredAt
        // A transfer's ENVELOPE cell is an arrow between two envelopes, not
        // one envelope: it stays read-only, and the commit leaves the legs
        // alone, as the WALLET cell does.
        flowId = row.isTransfer ? nil : row.flowId
        category = row.category
        note = row.note
        // A transfer's WALLET cell is an arrow between two wallets, not a
        // name: nothing the cell could resolve, so it opens empty and the
        // commit leaves the legs alone.
        wallet = row.isTransfer || row.walletDisplay == TransactionRow.placeholder ? "" : row.walletDisplay
        person = row.person
        amount = LedgerMoney.editable(row.absoluteAmount)
    }

    /// A new line: today if the month on screen is the current one, its first
    /// day otherwise, the sticky envelope already filled in, and the author
    /// as its person, which is who a row is for unless the cell says another.
    static func blank(in store: AppStore) -> RowDraft {
        var draft = RowDraft()
        let now = Date()
        draft.day = MonthKey(now) == store.month ? now : store.month.start()
        draft.flowId = store.defaultFlowId
        if store.showWalletColumn { draft.wallet = store.defaultWalletName ?? "" }
        draft.person = store.currentAuthor
        return draft
    }

    /// `row` copied into the empty line, dated today, ready to be tweaked and
    /// saved (⌘D and the context menu). `nil` for a transfer: the line has one
    /// wallet and one envelope, and a transfer's two ends fit neither.
    static func duplicate(of row: TransactionRow, store: AppStore) -> RowDraft? {
        guard !row.isTransfer else { return nil }
        var draft = RowDraft(row: row, store: store)
        draft.day = Date()
        return draft
    }

    /// The fields of a new row, resolved and parsed. `nil` when the amount
    /// cell is still empty: tabbing to the end of a blank line and pressing ↩
    /// is not an error, it is nothing.
    func entry(store: AppStore) throws -> RowEntry? {
        guard !amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        let resolvedWallet = try cell(.wallet) { try store.resolveWallet(named: wallet) }
        let resolvedPerson = try cell(.person) { try store.resolvePerson(named: person) }
        let parsedAmount = try cell(.amount) { try parseMoney(text: amount, currency: store.currency) }
        let trimmed = category.trimmingCharacters(in: .whitespacesAndNewlines)
        return (
            day: day,
            flowId: flowId ?? store.defaultFlowId,
            // An empty cell showing a suggestion saves what it shows.
            category: trimmed.isEmpty ? suggestedCategory : trimmed,
            note: note,
            amount: parsedAmount,
            walletId: resolvedWallet,
            kind: kind,
            person: resolvedPerson
        )
    }

    /// The diff against the row as it is stored: only the cells that changed.
    ///
    /// The cells are read in this order, so a draft wrong in two places
    /// sends the focus to its amount first.
    func patch(against row: TransactionRow, store: AppStore) throws -> TransactionPatch {
        var patch = TransactionPatch()
        for field in [RowField.amount, .category, .note, .flow, .wallet, .person, .date] {
            try add(field, against: row, store: store, to: &patch)
        }
        return patch
    }

    /// The diff for `field` alone: what saving one cell of a stored row
    /// sends (`LedgerEditor`). The other cells are not even read, so a value
    /// a sync wrote into one of them meanwhile is never written back over.
    func patch(_ field: RowField, against row: TransactionRow, store: AppStore) throws -> TransactionPatch {
        var patch = TransactionPatch()
        try add(field, against: row, store: store, to: &patch)
        return patch
    }

    private func add(
        _ field: RowField,
        against row: TransactionRow,
        store: AppStore,
        to patch: inout TransactionPatch
    ) throws {
        switch field {
        case .amount:
            // An emptied amount cell leaves the amount alone: clearing it is
            // not a way to say zero, and the core would refuse zero anyway.
            guard !amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
            let parsed = try cell(.amount) { try parseMoney(text: amount, currency: store.currency) }
            if parsed != row.absoluteAmount, parsed > 0 { patch.amount = parsed }

        case .category:
            let trimmedCategory = category.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmedCategory != row.category { patch.category = trimmedCategory }

        case .note:
            let trimmedNote = note.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmedNote != row.note { patch.note = trimmedNote }

        case .flow:
            // An envelope the row is on stays as it is even once archived:
            // only a pick from the list moves it.
            if !row.isTransfer, let flowId, flowId != row.flowId { patch.flowId = flowId }

        case .wallet:
            // Only when the cell was actually retyped: a row sitting on an
            // archived wallet still shows its name, which no longer resolves.
            let trimmedWallet = wallet.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmedWallet.isEmpty, trimmedWallet != row.walletDisplay else { return }
            let resolvedWallet = try cell(.wallet) { try store.resolveWallet(named: trimmedWallet) }
            if let walletId = resolvedWallet, walletId != row.walletId { patch.walletId = walletId }

        case .person:
            // Only when retyped, as the wallet: a row for someone who has
            // left the vault still shows their name, which no longer
            // resolves. Back to whoever recorded it is said with a blank, the
            // patch's own word for it, rather than with a name.
            let trimmedPerson = person.trimmingCharacters(in: .whitespacesAndNewlines)
            guard row.isPersonEditable, !trimmedPerson.isEmpty, trimmedPerson != row.person else { return }
            let resolvedPerson = try cell(.person) { try store.resolvePerson(named: trimmedPerson) }
            if let name = resolvedPerson, name != row.person {
                patch.person = name == row.recordedBy ? "" : name
            }

        case .date:
            if !Calendar.current.isDate(day, inSameDayAs: row.occurredAt) {
                patch.occurredAt = AppStore.stamp(day: day, likeTimeOf: row.occurredAt)
            }
        }
    }

    // MARK: - One cell at a time

    /// What `field` says when it is typed into from what it holds: the
    /// bare day number for DATA, as `DayCell` starts from, and nothing for
    /// ENVELOPE, whose text only narrows its list (the pick is `flowId`).
    func text(of field: RowField, calendar: Calendar = .current) -> String {
        switch field {
        case .date: "\(calendar.component(.day, from: day))"
        case .flow: ""
        case .category: category
        case .note: note
        case .wallet: wallet
        case .person: person
        case .amount: amount
        }
    }

    /// `field` as the cell being edited left it: `text`, or for ENVELOPE the
    /// envelope picked from its list. A DATA text that means no date leaves
    /// the day alone, as the cell has always snapped back.
    mutating func set(_ field: RowField, text: String, flowId pick: Uuid?, in month: MonthKey) {
        switch field {
        case .date:
            if let parsed = LedgerDate.parseDay(text, in: month) { day = parsed }
        case .flow: flowId = pick
        case .category: category = text
        case .note: note = text
        case .wallet: wallet = text
        case .person: person = text
        case .amount: amount = text
        }
    }

    /// Whether `field` says something else here than in `other`: a day is
    /// the same day whatever its time, an envelope the same pick.
    func differs(in field: RowField, from other: RowDraft) -> Bool {
        switch field {
        case .date: !Calendar.current.isDate(day, inSameDayAs: other.day)
        case .flow: flowId != other.flowId
        default: text(of: field) != other.text(of: field)
        }
    }

    /// Runs a cell's parse, tagging whatever it throws with the cell it came
    /// from so the grid can send the focus back there.
    private func cell<T>(_ field: RowField, _ work: () throws -> T) throws -> T {
        do {
            return try work()
        } catch {
            throw RowDraftError(field: field, underlying: error)
        }
    }
}
