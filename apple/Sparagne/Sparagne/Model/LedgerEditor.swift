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
        // A stub for now.
        newLine.note
    }

    // MARK: - The keys and the clicks

    /// A click on `cell`: the cell being edited is saved first, and the
    /// new line registered if the click leaves it. `false` when that save
    /// was refused: the cursor stays, editing the cell that is wrong.
    @discardableResult
    func select(_ cell: CellFocus) -> Bool {
        // A stub for now.
        false
    }

    /// A double click, ↩ or F2: `cell` (the cursor's, when `nil`) is typed
    /// into from what it says. `false` when it cannot be typed into.
    @discardableResult
    func edit(_ cell: CellFocus? = nil) -> Bool {
        // A stub for now.
        false
    }

    /// Characters typed. On a selected cell they replace what it says, and
    /// "" empties it (⌫); while the cell is being edited they are appended,
    /// which is how keys typed before the field took the caret are kept.
    /// `false` when the cell cannot be typed into.
    @discardableResult
    func type(_ characters: String) -> Bool {
        // A stub for now.
        false
    }

    /// ↑ ↓ ← → ⇥ ⇧⇥: the cell being edited is saved, the new line registered
    /// if it is left (⇥ off its IMPORTO included), and the cursor moves
    /// (`LedgerCursor.target`). `false` when a save was refused.
    @discardableResult
    func move(_ move: CursorMove) -> Bool {
        // A stub for now.
        false
    }

    /// ↩ while editing: the cell is saved and stays selected. On the new
    /// line with an amount, the line is registered and the cursor goes to
    /// DATA of the blank line that takes its place.
    func confirm() {
        // A stub for now.
    }

    /// esc. While editing: the cell goes back to what it said, still
    /// selected. With a cell selected: no cursor. With the new line's cell
    /// selected: the line is emptied, as esc always did there.
    func cancel() {
        // A stub for now.
    }

    /// The cursor leaves the grid: the cell being edited is saved and the
    /// new line registered if it has an amount; the cursor goes. `false`
    /// when a save was refused, and then nothing moves.
    @discardableResult
    func finish() -> Bool {
        // A stub for now.
        true
    }

    /// The month, the lists, the vault or the role changed: whatever was
    /// open belongs to rows no longer on screen. No cursor, a blank new line.
    func reset() {
        // A stub for now.
    }

    /// ⌘D and the context menu: `row` copied into the new line, the caret in
    /// its IMPORTO (`RowDraft.duplicate`).
    func duplicate(_ row: TransactionRow) {
        // A stub for now.
    }

    /// Waits for the writes the editor has sent: `AppStore.settle` and
    /// quitting do.
    func settle() async {
        // A stub for now.
    }
}
