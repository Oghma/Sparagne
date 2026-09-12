import SwiftUI
import SparagneCore

/// The spreadsheet (`docs/v2/UI.md` §2.1): one row per transaction, oldest
/// first, with an always-empty last line for the next one.
///
/// Editing is per row, not per cell: clicking a cell opens the whole row for
/// editing with the focus on that cell, ⇥ walks the fields, ↩ commits the diff
/// as one `UpdateTransaction` and esc throws the draft away. That is what the
/// footer hints promise, and it means a row is never half-written.
///
/// Leaving the row saves it too, the way a spreadsheet does: ⇥ off IMPORTO, a
/// click on another row or on the search field all commit what was typed. esc
/// is the only way to lose it.
struct LedgerGrid: View {
    @Bindable var store: AppStore

    /// Which row is open for editing; `nil` is the new-row line at the bottom.
    @State private var editing: Uuid?
    @State private var draft = RowDraft()
    @State private var newRow = RowDraft()
    /// The row under the pointer, so the eye can follow it across a grid that
    /// is much wider than a line of text.
    @State private var hovered: Uuid?
    @FocusState private var focus: CellFocus?

    var body: some View {
        VStack(spacing: 0) {
            GridHeader(showsWallet: store.showWalletColumn)
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
        // The month and the vault change what the empty line means, so its
        // draft starts over with them. A sync landing or a void must not:
        // those move `store.rows` under a line that is being typed.
        .onChange(of: store.month) { _, _ in
            resetEditing()
            newRow = RowDraft.blank(in: store)
        }
        .onChange(of: store.currentVault?.id) { _, _ in
            resetEditing()
            newRow = RowDraft.blank(in: store)
        }
        .onChange(of: store.direction) { _, _ in resetEditing() }
        // Showing the column fills the empty line's cell with the sticky
        // default; hiding it puts that value back out of reach.
        .onChange(of: store.showWalletColumn) { _, _ in
            resetEditing()
            newRow = RowDraft.blank(in: store)
        }
        .onChange(of: focus) { _, new in commitIfLeft(new) }
        .onAppear { newRow = RowDraft.blank(in: store) }
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
            showsWallet: store.showWalletColumn,
            draft: isEditing ? $draft : nil,
            focus: $focus,
            onOpen: { field in open(row, at: field) },
            onCommit: { submit(row.id) },
            onCancel: resetEditing
        )
        .background(isEditing || hovered == row.id ? Ink.raised : Color.clear)
        .overlay(alignment: .leading) {
            if isEditing {
                Rectangle().fill(Ink.accent).frame(width: 2)
            }
        }
        .onHover { inside in
            if inside {
                hovered = row.id
            } else if hovered == row.id {
                hovered = nil
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
        NewRowView(
            store: store,
            draft: $newRow,
            focus: $focus,
            onCommit: commitNewRow,
            showsWallet: store.showWalletColumn
        )
            .background(focus?.row == nil && focus != nil ? Ink.raised : Color.clear)
            .overlay(alignment: .leading) {
                Rectangle().fill(Ink.accent).frame(width: 2)
            }
    }

    // MARK: - Actions

    /// Opens `row` with the caret in `field`, saving whatever line was open
    /// before. A row that refuses to save stays open and keeps the focus, so
    /// the click that would have left it does not lose it.
    private func open(_ row: TransactionRow, at field: RowField) {
        guard !row.voided else { return }
        if editing == row.id {
            // Another cell of the row already open: move the caret only, or
            // the draft would be rebuilt from the stored row and lose the edit.
            focus = CellFocus(row: row.id, field: field)
            return
        }
        if let editing, !commit(editing) { return }
        editing = row.id
        draft = RowDraft(row: row, store: store)
        focus = CellFocus(row: row.id, field: field)
    }

    /// esc: throw the draft away. `editing` is cleared before the focus, so
    /// the focus change that follows does not read as "left the row" and save
    /// what esc was meant to undo.
    private func resetEditing() {
        editing = nil
        focus = nil
    }

    /// ↩: save the row and close it.
    private func submit(_ rowId: Uuid) {
        if commit(rowId) { focus = nil }
    }

    /// The commit on blur a spreadsheet does: ⇥ off IMPORTO lands on the empty
    /// line, a click lands on another row or on the search field, and in every
    /// case the line being left is written.
    private func commitIfLeft(_ new: CellFocus?) {
        guard let editing, new?.row != editing else { return }
        commit(editing)
    }

    /// Writes the draft back and says whether it went through.
    ///
    /// Sends only what changed, so two people editing different cells of the
    /// same row do not conflict (`docs/v2/ARCH.md` §4). A cell the parser or
    /// the core refuses keeps the row open with the draft intact and puts the
    /// focus back where the mistake is.
    @discardableResult
    private func commit(_ rowId: Uuid) -> Bool {
        guard editing == rowId else { return true }
        guard let row = store.rows.first(where: { $0.id == rowId }) else {
            // Voided, or carried out of the month by a sync while it was open.
            editing = nil
            return true
        }
        do {
            let patch = try draft.patch(against: row, store: store)
            editing = nil
            if !patch.isEmpty { store.update(transactionId: rowId, patch: patch) }
            return true
        } catch let failure as RowDraftError {
            store.report(failure.underlying)
            focus = CellFocus(row: rowId, field: failure.field)
            return false
        } catch {
            store.report(error)
            return false
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
                amount: entry.amount,
                walletId: entry.walletId
            )
            newRow = RowDraft.blank(in: store)
            focus = CellFocus(row: nil, field: .date)
        } catch let failure as RowDraftError {
            store.report(failure.underlying)
            focus = CellFocus(row: nil, field: failure.field)
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

/// The cells a row is typed into. PERSONA is not one: it is the author of the
/// command, not a field (`docs/v2/UI.md` §3).
///
/// Nothing here drives the traversal. ⇥ and ⇧⇥ are the system's, walking the
/// focusable views in layout order, and the order of the cases is only the
/// order the cells are laid out in. A closed row holds no text field, so from
/// IMPORTO of the row being edited ⇥ reaches the DATA cell of the empty line
/// and never gets stuck on the rows in between; ⇧⇥ from DATA goes back up to
/// the header, whose search field is the previous focusable view. Both leave
/// the row, which is what `commitIfLeft` is for.
enum RowField: Hashable, CaseIterable {
    case date
    case flow
    case category
    case note
    /// Only in the hierarchy while the optional WALLET column is on.
    case wallet
    case amount
}

// MARK: - The draft behind an edited row

/// A cell that refused what was typed. The grid reports `underlying`, which
/// carries the core's own message, and puts the focus back on `field`.
struct RowDraftError: Error {
    let field: RowField
    let underlying: Error
}

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
    var wallet = ""
    var amount = ""

    nonisolated init() {}

    init(row: TransactionRow, store: AppStore) {
        day = row.occurredAt
        flow = row.envelopeDisplay == TransactionRow.placeholder ? "" : row.envelopeDisplay
        category = row.category
        note = row.note
        // A transfer's WALLET cell is an arrow between two wallets, not a
        // name: nothing the cell could resolve, so it opens empty and the
        // commit leaves the legs alone.
        wallet = row.isTransfer || row.walletDisplay == TransactionRow.placeholder ? "" : row.walletDisplay
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
        if store.showWalletColumn { draft.wallet = store.defaultWalletName ?? "" }
        return draft
    }

    /// The fields of a new row, resolved and parsed. `nil` when the amount
    /// cell is still empty: tabbing to the end of a blank line and pressing ↩
    /// is not an error, it is nothing.
    func entry(
        store: AppStore
    ) throws -> (day: Date, flowId: Uuid?, category: String?, note: String, amount: Int64, walletId: Uuid?)? {
        guard !amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        let resolvedFlow = try cell(.flow) { try store.resolveFlow(named: flow) }
        let resolvedWallet = try cell(.wallet) { try store.resolveWallet(named: wallet) }
        let parsedAmount = try cell(.amount) { try parseMoney(text: amount, currency: store.currency) }
        let trimmed = category.trimmingCharacters(in: .whitespacesAndNewlines)
        return (
            day: day,
            flowId: resolvedFlow ?? store.defaultFlowId,
            category: trimmed.isEmpty ? nil : trimmed,
            note: note,
            amount: parsedAmount,
            walletId: resolvedWallet
        )
    }

    /// The diff against the row as it is stored: only the cells that changed.
    func patch(against row: TransactionRow, store: AppStore) throws -> TransactionPatch {
        var patch = TransactionPatch()

        // An emptied amount cell leaves the amount alone: clearing it is not a
        // way to say zero, and the core would refuse zero anyway.
        if !amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let parsed = try cell(.amount) { try parseMoney(text: amount, currency: store.currency) }
            if parsed != row.absoluteAmount, parsed > 0 { patch.amount = parsed }
        }

        let trimmedCategory = category.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedCategory != row.category { patch.category = trimmedCategory }

        let trimmedNote = note.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedNote != row.note { patch.note = trimmedNote }

        let resolvedFlow = try cell(.flow) { try store.resolveFlow(named: flow) }
        if let flowId = resolvedFlow, flowId != row.flowId { patch.flowId = flowId }

        // Only when the cell was actually retyped: a row sitting on an
        // archived wallet still shows its name, which no longer resolves.
        let trimmedWallet = wallet.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmedWallet.isEmpty, trimmedWallet != row.walletDisplay {
            let resolvedWallet = try cell(.wallet) { try store.resolveWallet(named: trimmedWallet) }
            if let walletId = resolvedWallet, walletId != row.walletId { patch.walletId = walletId }
        }

        if !Calendar.current.isDate(day, inSameDayAs: row.occurredAt) {
            patch.occurredAt = AppStore.stamp(day: day, likeTimeOf: row.occurredAt)
        }
        return patch
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
