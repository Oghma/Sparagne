import Foundation
import Observation
import SparagneCore

/// The Mastro's cursor and the one cell being typed into, as a spreadsheet
/// has them: a cell is selected (the cursor) or edited (the caret in it),
/// and an edited cell is written the moment it is left, whichever way.
///
/// Owned by the store, not by the grid, so switching tab does not lose what
/// is being typed, and the store can save it before the month, the lists or
/// the vault change under it (`finish`), while the rows it belongs to are
/// still loaded.
///
/// Only one cell is ever being edited, and what it says is `text` (or
/// `flowId` for ENVELOPE, which is picked from a list). Saving a stored
/// row's cell sends that field alone, worked out against the row as it is
/// stored at that moment, so a sync that changed another cell of the row
/// meanwhile is never written back over.
///
/// The new line keeps its cells in `newLine` until it is registered, which
/// happens when the cursor leaves it with an amount in it.
@MainActor
@Observable
final class LedgerEditor {
    @ObservationIgnored private weak var store: AppStore?
    /// The writes sent and not landed yet, one after the other in the order
    /// the cells were left, and a counter that says whether one was queued
    /// while the last was running (`settle`).
    @ObservationIgnored private var writes: Task<Void, Never>?
    @ObservationIgnored private var queued = 0
    @ObservationIgnored private var inFlight = 0

    /// The selected cell, `nil` when the grid has none.
    private(set) var cursor: CellFocus?
    /// The cursor's cell is being typed into.
    private(set) var isEditing = false
    /// What the cell being edited says, for every cell but ENVELOPE: the
    /// text field binds to it.
    var text = ""
    /// ENVELOPE's pick while that cell is being edited.
    var flowId: Uuid?
    /// The new line's cells, as they stand while the line is not registered:
    /// what it shows, and what it registers.
    var newLine = RowDraft()

    nonisolated init() {}

    /// Called once by the store that owns the editor.
    func attach(_ store: AppStore) {
        self.store = store
    }

    /// The moves and the cells that can be typed into, for the rows on
    /// screen now.
    var layout: LedgerCursor {
        LedgerCursor(
            rows: store?.rows ?? [],
            canWrite: store?.canWrite ?? false,
            showsWallet: store?.showWalletColumn ?? false
        )
    }

    /// The note the category suggestion is asked about: the text being typed
    /// while the new line's DESCRIZIONE is edited, its stored note otherwise.
    var newLineNote: String {
        isEditing && cursor == CellFocus(row: nil, field: .note) ? text : newLine.note
    }

    /// Writes sent that have not landed yet: quitting waits for them
    /// (`AppDelegate`).
    var hasPendingWrites: Bool { inFlight > 0 }

    // MARK: - The keys and the clicks

    /// A click on `cell`: the cell being edited is saved first, and the
    /// new line registered if the click leaves it. `false` when that save
    /// was refused: the cursor stays, editing the cell that is wrong.
    @discardableResult
    func select(_ cell: CellFocus) -> Bool {
        guard cell != cursor else { return true }
        guard leave(for: cell.row, wrapping: false) else { return false }
        place(at: cell)
        return true
    }

    /// A double click, ↩ or F2: `cell` (the cursor's, when `nil`) is typed
    /// into from what it says. `false` when it cannot be typed into.
    @discardableResult
    func edit(_ cell: CellFocus? = nil) -> Bool {
        guard let target = cell ?? cursor else { return false }
        // A double click on another cell selects it first, saving the one
        // being left; one that cannot be typed into is still selected.
        guard select(target), layout.isEditable(target) else { return false }
        if !isEditing { begin(with: content(of: target)) }
        return true
    }

    /// Characters typed. On a selected cell they replace what it says, and
    /// "" empties it (⌫); while the cell is being edited they are appended,
    /// which is how keys typed before the field took the caret are kept.
    /// `false` when the cell cannot be typed into.
    @discardableResult
    func type(_ characters: String) -> Bool {
        guard let cursor, layout.isEditable(cursor) else { return false }
        if isEditing {
            text += characters
        } else {
            begin(with: characters)
        }
        return true
    }

    /// ↑ ↓ ← → ⇥ ⇧⇥: the cell being edited is saved, the new line registered
    /// if it is left (⇥ off its IMPORTO included), and the cursor moves
    /// (`LedgerCursor.target`). `false` when a save was refused.
    @discardableResult
    func move(_ move: CursorMove) -> Bool {
        guard let from = cursor else { return true }
        let layout = self.layout
        let target = layout.target(of: move, from: from)
        // ⇥ off the new line's IMPORTO lands on the new line again, but the
        // line as typed is done with, as a row of a spreadsheet is.
        let wraps = from.row == nil && move == .tab && from.field == layout.fields.last
        guard leave(for: target.row, wrapping: wraps) else { return false }
        place(at: target)
        return true
    }

    /// ↩ while editing: the cell is saved and stays selected. On the new
    /// line with an amount, the line is registered and the cursor goes to
    /// DATA of the blank line that takes its place.
    func confirm() {
        guard let at = cursor, isEditing, saveEditedCell() else { return }
        guard at.row == nil, register() == .registered else { return }
        place(at: CellFocus(row: nil, field: .date))
    }

    /// esc. While editing: the cell goes back to what it said, still
    /// selected. With a cell selected: no cursor. With the new line's cell
    /// selected: the line is emptied, as esc always did there.
    func cancel() {
        guard let at = cursor else { return }
        if isEditing {
            // Nothing was written yet: what the cell said is still in the
            // row, or in `newLine`.
            stopEditing()
            return
        }
        if at.row == nil, let store { newLine = RowDraft.blank(in: store) }
        place(at: nil)
    }

    /// The cursor leaves the grid: the cell being edited is saved and the
    /// new line registered if it has an amount; the cursor goes. `false`
    /// when a save was refused, and then nothing moves.
    @discardableResult
    func finish() -> Bool {
        guard cursor != nil else { return true }
        guard leave(for: nil, wrapping: true) else { return false }
        place(at: nil)
        return true
    }

    /// The month, the lists, the vault or the role changed: whatever was
    /// open belongs to rows no longer on screen. No cursor, a blank new line.
    func reset() {
        place(at: nil)
        newLine = store.map { RowDraft.blank(in: $0) } ?? RowDraft()
    }

    /// ⌘D and the context menu: `row` copied into the new line, the caret in
    /// its IMPORTO (`RowDraft.duplicate`).
    func duplicate(_ row: TransactionRow) {
        guard let store, store.canWrite, let copy = RowDraft.duplicate(of: row, store: store) else { return }
        // A stored cell being typed into is saved, as any move away from it
        // is. The new line is replaced by the copy, cell being typed into and
        // all, as ⌘D always replaced it.
        if cursor?.row != nil {
            guard saveEditedCell() else { return }
        }
        newLine = copy
        place(at: CellFocus(row: nil, field: .amount))
        begin(with: copy.amount)
    }

    /// Waits for the writes the editor has sent: `AppStore.settle` and
    /// quitting do.
    func settle() async {
        while let writes {
            let generation = queued
            await writes.value
            if generation == queued {
                self.writes = nil
                return
            }
        }
    }

    // MARK: - What the store tells the editor

    /// The rows on screen changed: a reload, a sync, a void on the toast. A
    /// cursor on a row no longer among them (voided, synced away, filtered
    /// out) has nothing left to point at and goes, with whatever was typed
    /// into it; one on a row that can no longer be typed into (voided while
    /// the deleted rows are shown) stops editing; one on the new line goes
    /// when the line does.
    func rowsChanged() {
        guard let store, let at = cursor else { return }
        let gone = if let id = at.row { !store.rows.contains { $0.id == id } } else { !store.canWrite }
        if gone {
            place(at: nil)
        } else if isEditing, !layout.isEditable(at) {
            stopEditing()
        }
    }

    /// The WALLET column was shown or hidden. A cursor on it moves to
    /// PERSONA, the cell beside it, its edit saved first: one refused is
    /// lost, as a change of month loses one, since the cell is gone. The new
    /// line's cell takes the sticky wallet as the column shows, and lets go
    /// of what it said as it hides, so a wallet nobody can see is never
    /// registered with the line.
    func walletColumnChanged() {
        guard let store else { return }
        if let at = cursor, at.field == .wallet, !store.showWalletColumn {
            if !saveEditedCell() { stopEditing() }
            place(at: CellFocus(row: at.row, field: .person))
        }
        newLine.wallet = store.showWalletColumn ? store.defaultWalletName ?? "" : ""
    }

    /// The cell being edited is written now and stays selected: what ↩ does
    /// on a stored row, and what the store does before a filter that may
    /// take the row off screen (the deleted rows, the transfers). On the new
    /// line it is written into `newLine`, which nothing refuses. `false`
    /// when it was refused, and then the cell is still being edited.
    @discardableResult
    func saveEditedCell() -> Bool {
        guard let store, isEditing, let at = cursor else { return true }
        guard let id = at.row else {
            newLine.set(at.field, text: text, flowId: flowId, in: store.month)
            stopEditing()
            return true
        }
        // Voided or carried off by a sync between a reload and
        // `rowsChanged`: there is nothing left to write to.
        guard let row = store.rows.first(where: { $0.id == id }) else {
            stopEditing()
            return true
        }
        // The cell alone, against the row as it is stored now: a sync that
        // changed another cell since the edit began is not undone.
        var draft = RowDraft(row: row, store: store)
        draft.set(at.field, text: text, flowId: flowId, in: store.month)
        do {
            let patch = try draft.patch(at.field, against: row, store: store)
            if !patch.isEmpty {
                // Read now, not when the write gets its turn: by then the
                // rows of another month may have replaced this one.
                let inverse = store.storedInverse(of: patch, for: id)
                write { store in
                    await store.update(transactionId: id, patch: patch, inverse: inverse)
                }
            }
        } catch {
            store.report((error as? RowDraftError)?.underlying ?? error)
            return false
        }
        stopEditing()
        return true
    }

    // MARK: - Leaving a cell

    /// What every way out of a cell does before the cursor moves: the cell
    /// being edited is saved, and the new line registered when the cursor
    /// leaves it for `row`, a stored row, or with `wrapping` for the world
    /// outside the grid or the new line again after ⇥ off its end. `false`
    /// when either was refused; the cursor is then on the cell to fix.
    private func leave(for row: Uuid?, wrapping: Bool) -> Bool {
        guard let at = cursor else { return true }
        guard saveEditedCell() else { return false }
        if at.row == nil, row != nil || wrapping {
            return register() != .refused
        }
        return true
    }

    private enum Registration {
        case registered
        /// No amount yet: the line stays as typed, a draft.
        case nothing
        case refused
    }

    /// Sends the new line as a row, if it has an amount, and blanks it.
    /// Everything the row is made of is read now, its kind included: a
    /// change of list registers the line on its way out, and by the time the
    /// write runs the list on screen is the other one.
    private func register() -> Registration {
        guard let store else { return .nothing }
        let parsed: Result<RowEntry?, Error> = Result { try newLine.entry(store: store) }
        switch parsed {
        case .success(let entry):
            guard let entry, entry.amount > 0 else { return .nothing }
            let kind = entry.kind ?? store.direction.newRowKind
            write { store in
                await store.addRow(
                    day: entry.day,
                    flowId: entry.flowId,
                    category: entry.category,
                    note: entry.note,
                    amount: entry.amount,
                    walletId: entry.walletId,
                    kind: kind,
                    person: entry.person
                )
            }
            newLine = Self.next(after: entry, in: store)
            return .registered
        case .failure(let error):
            store.report((error as? RowDraftError)?.underlying ?? error)
            // The cell that is wrong takes the caret, from what it says.
            if let failure = error as? RowDraftError, layout.fields.contains(failure.field) {
                let cell = CellFocus(row: nil, field: failure.field)
                place(at: cell)
                begin(with: content(of: cell))
            }
            return .refused
        }
    }

    /// The blank line after `entry` was registered: on the envelope and the
    /// wallet just used, which is what the sticky defaults will say once the
    /// row lands.
    private static func next(after entry: RowEntry, in store: AppStore) -> RowDraft {
        var next = RowDraft.blank(in: store)
        next.flowId = entry.flowId ?? next.flowId
        if store.showWalletColumn, let walletId = entry.walletId,
           let wallet = store.wallets.first(where: { $0.id == walletId }) {
            next.wallet = wallet.name
        }
        return next
    }

    // MARK: - The cursor

    /// The cursor on `cell`, selected and not being edited.
    private func place(at cell: CellFocus?) {
        cursor = cell
        stopEditing()
    }

    /// The cursor's cell starts being typed into, saying `start`. ENVELOPE
    /// starts on the envelope it is on, with `start` narrowing its list.
    private func begin(with start: String) {
        guard let cursor else { return }
        text = start
        flowId = draft(for: cursor.row)?.flowId
        isEditing = true
    }

    private func stopEditing() {
        isEditing = false
        text = ""
        flowId = nil
    }

    /// What `cell` says when it is typed into from what it holds
    /// (`RowDraft.text(of:)`).
    private func content(of cell: CellFocus) -> String {
        draft(for: cell.row)?.text(of: cell.field) ?? ""
    }

    /// The cells of a line as they stand: a stored row's as the row is
    /// stored now, the new line's as typed.
    private func draft(for row: Uuid?) -> RowDraft? {
        guard let id = row else { return newLine }
        guard let store, let stored = store.rows.first(where: { $0.id == id }) else { return nil }
        return RowDraft(row: stored, store: store)
    }

    // MARK: - The writes

    /// Queues a write behind those already sent, so they land in the order
    /// the cells were left. It is for the vault on screen now: if another
    /// vault is on screen by its turn (the one it was for deleted, by a sync
    /// or by its owner, with no chance to wait for it), it is dropped without
    /// a word rather than sent to the wrong vault.
    private func write(_ work: @escaping @MainActor (AppStore) async -> Void) {
        guard let store, let vaultId = store.currentVault?.id else { return }
        queued += 1
        inFlight += 1
        let previous = writes
        writes = Task { [weak self] in
            await previous?.value
            if store.currentVault?.id == vaultId { await work(store) }
            self?.inFlight -= 1
        }
    }
}
