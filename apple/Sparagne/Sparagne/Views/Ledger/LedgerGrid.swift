import SwiftUI
import SparagneCore

/// The spreadsheet (`docs/v2/UI.md` §2.1): one row per transaction, oldest
/// first, with an always-empty last line for the next one.
///
/// Editing is per row, not per cell: clicking a cell opens the whole row for
/// editing with the focus on that cell, ⇥ walks the fields, ↩ commits the diff
/// as one `UpdateTransaction` and esc throws the draft away. That is what the
/// footer hints promise, and it means a row is never half-written.
struct LedgerGrid: View {
    @Bindable var store: AppStore

    /// Which row is open for editing; `nil` is the new-row line at the bottom.
    @State private var editing: Uuid?
    @State private var draft = RowDraft()
    @State private var newRow = RowDraft()
    @FocusState private var focus: CellFocus?

    var body: some View {
        VStack(spacing: 0) {
            GridHeader()
            Hairline()
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(Array(store.rows.enumerated()), id: \.element.id) { index, row in
                        rowView(index: index, row: row)
                        Hairline()
                    }
                    newRowView
                }
            }
            .scrollBounceBehavior(.basedOnSize)
            Hairline()
            KeyHints()
        }
        .background(Ink.bg)
        .onChange(of: store.month) { _, _ in resetEditing() }
        .onChange(of: store.direction) { _, _ in resetEditing() }
        .onAppear { newRow = RowDraft.blank(in: store) }
        .onChange(of: store.rows.count) { _, _ in newRow = RowDraft.blank(in: store) }
        .onReceive(NotificationCenter.default.publisher(for: .duplicateLastRow)) { _ in
            duplicateLast()
        }
    }

    // MARK: - Existing rows

    @ViewBuilder
    private func rowView(index: Int, row: TransactionRow) -> some View {
        let isEditing = editing == row.id
        LedgerRowView(
            ordinal: index + 1,
            row: row,
            store: store,
            draft: isEditing ? $draft : nil,
            focus: $focus,
            onOpen: { field in open(row, at: field) },
            onCommit: { commit(row) },
            onCancel: resetEditing
        )
        .background(isEditing ? Ink.raised : Color.clear)
        .overlay(alignment: .leading) {
            if isEditing {
                Rectangle().fill(Ink.accent).frame(width: 2)
            }
        }
        .contextMenu {
            Button(String(localized: "Duplicate")) { duplicate(row) }
            Divider()
            Button(String(localized: "Void"), role: .destructive) { store.void(transactionId: row.id) }
        }
    }

    // MARK: - The empty line

    private var newRowView: some View {
        NewRowView(store: store, draft: $newRow, focus: $focus, onCommit: commitNewRow)
            .background(focus?.row == nil && focus != nil ? Ink.raised : Color.clear)
            .overlay(alignment: .leading) {
                Rectangle().fill(Ink.accent).frame(width: 2)
            }
    }

    // MARK: - Actions

    private func open(_ row: TransactionRow, at field: RowField) {
        guard !row.voided else { return }
        editing = row.id
        draft = RowDraft(row: row, store: store)
        focus = CellFocus(row: row.id, field: field)
    }

    private func resetEditing() {
        editing = nil
        focus = nil
    }

    /// Sends only what changed, so two people editing different cells of the
    /// same row do not conflict (`docs/v2/ARCH.md` §4).
    private func commit(_ row: TransactionRow) {
        guard editing == row.id else { return }
        do {
            let patch = try draft.patch(against: row, store: store)
            resetEditing()
            if !patch.isEmpty { store.update(transactionId: row.id, patch: patch) }
        } catch {
            store.report(error)
        }
    }

    private func commitNewRow() {
        do {
            guard let entry = try newRow.entry(store: store), entry.amount > 0 else { return }
            store.addRow(
                day: entry.day,
                flowId: entry.flowId,
                category: entry.category,
                note: entry.note,
                amount: entry.amount
            )
            newRow = RowDraft.blank(in: store)
            focus = CellFocus(row: nil, field: .date)
        } catch {
            store.report(error)
        }
    }

    /// ⌘D and the context menu: copy a row into the empty line, ready to be
    /// tweaked and saved.
    private func duplicate(_ row: TransactionRow) {
        newRow = RowDraft(row: row, store: store)
        newRow.day = Date()
        focus = CellFocus(row: nil, field: .amount)
    }

    /// ⌘D with nothing selected duplicates the last row of the month.
    private func duplicateLast() {
        guard let last = store.lastRow else { return }
        duplicate(last)
    }
}

// MARK: - Focus

/// A cell: which row (nil = the new line) and which field.
struct CellFocus: Hashable {
    let row: Uuid?
    let field: RowField
}

enum RowField: Hashable, CaseIterable {
    case date
    case flow
    case category
    case note
    case amount

    /// ⇥ order. PERSONA is not here: it is the author of the command, not a
    /// field (`docs/v2/UI.md` §3).
    var next: RowField? {
        let all = RowField.allCases
        guard let index = all.firstIndex(of: self), index + 1 < all.count else { return nil }
        return all[index + 1]
    }
}

// MARK: - The draft behind an edited row

/// The text in the cells of the row being edited. Everything is a string
/// until it commits, so a half-typed amount never has to be a number.
///
/// Main-actor bound because resolving a name or parsing an amount reads the
/// store, which owns the core.
@MainActor
struct RowDraft {
    var day = Date()
    var flow = ""
    var category = ""
    var note = ""
    var amount = ""

    nonisolated init() {}

    init(row: TransactionRow, store: AppStore) {
        day = row.occurredAt
        flow = row.envelopeDisplay == TransactionRow.placeholder ? "" : row.envelopeDisplay
        category = row.category
        note = row.note
        amount = LedgerMoney.editable(row.absoluteAmount)
    }

    /// A new line: today if the month on screen is the current one, its first
    /// day otherwise, and the sticky envelope already filled in.
    static func blank(in store: AppStore) -> RowDraft {
        var draft = RowDraft()
        let now = Date()
        draft.day = MonthKey(now) == store.month ? now : store.month.start()
        if let id = store.defaultFlowId, let flow = store.flows.first(where: { $0.id == id }) {
            draft.flow = store.flowName(flow)
        }
        return draft
    }

    /// The fields of a new row, resolved and parsed. `nil` when the amount
    /// cell is still empty: tabbing to the end of a blank line and pressing ↩
    /// is not an error, it is nothing.
    func entry(store: AppStore) throws -> (day: Date, flowId: Uuid?, category: String?, note: String, amount: Int64)? {
        guard !amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        let trimmed = category.trimmingCharacters(in: .whitespacesAndNewlines)
        return (
            day: day,
            flowId: try store.resolveFlow(named: flow) ?? store.defaultFlowId,
            category: trimmed.isEmpty ? nil : trimmed,
            note: note,
            amount: try parseMoney(text: amount, currency: store.currency)
        )
    }

    /// The diff against the row as it is stored: only the cells that changed.
    func patch(against row: TransactionRow, store: AppStore) throws -> TransactionPatch {
        var patch = TransactionPatch()

        // An emptied amount cell leaves the amount alone: clearing it is not a
        // way to say zero, and the core would refuse zero anyway.
        if !amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let parsed = try parseMoney(text: amount, currency: store.currency)
            if parsed != row.absoluteAmount, parsed > 0 { patch.amount = parsed }
        }

        let trimmedCategory = category.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedCategory != row.category { patch.category = trimmedCategory }

        let trimmedNote = note.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedNote != row.note { patch.note = trimmedNote }

        if let flowId = try store.resolveFlow(named: flow), flowId != row.flowId {
            patch.flowId = flowId
        }

        if !Calendar.current.isDate(day, inSameDayAs: row.occurredAt) {
            patch.occurredAt = AppStore.stamp(day: day, likeTimeOf: row.occurredAt)
        }
        return patch
    }
}
