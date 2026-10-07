import Foundation
import SparagneCore

/// Row selection and the bulk actions over it: ⌘-click, ⇧-click and ⌘A pick
/// rows of the ledger, the selection bar voids them or files them under one
/// category (`docs/v2/DISTILLATO_V1.md` §7, "bulk edit").
///
/// The gestures only say which rows; the grid decides which gesture a click
/// was (`LedgerGrid.open`), so everything here runs without a window.
extension AppStore {
    /// The selected rows, in the order on screen.
    var selectedRows: [TransactionRow] { rows.filter { selection.contains($0.id) } }

    /// What a bulk action works on: the selected rows that are still live
    /// entries. A voided row can be neither voided nor updated again, and a
    /// transfer has no category and two ends, so both stay selected but are
    /// passed over.
    var bulkTargets: [TransactionRow] {
        rows.filter { selection.contains($0.id) && !$0.voided && !$0.isTransfer }
    }

    /// ⌘-click. Nothing on a vault this account only reads: every bulk action
    /// writes, so a selection there would promise what cannot be done.
    func toggleSelection(_ id: Uuid) {
        guard canWrite else { return }
        selection.toggle(id)
    }

    /// ⇧-click: from the anchor to `id`, in the order on screen.
    func extendSelection(to id: Uuid) {
        guard canWrite else { return }
        selection.extend(to: id, in: rows.map(\.id))
    }

    /// ⌘A while no cell is being edited: every row on screen, the pages not
    /// scrolled to yet excepted, since nobody has seen them.
    func selectAllRows() {
        guard canWrite else { return }
        selection.selectAll(rows.map(\.id))
    }

    func clearSelection() {
        selection.clear()
    }

    /// "Delete N Rows", ⌫ and ⌦: one pending void for every target, so one toast
    /// to take it back and, when it elapses, one batch.
    func voidSelection() async {
        let ids = bulkTargets.map(\.id)
        guard !ids.isEmpty else { return }
        selection.clear()
        await void(transactionIds: ids)
    }

    /// "Set Category…": every target filed under `name` in one batch, so
    /// either all of them move or, when one is refused (a row voided on
    /// another device since it was loaded), none does. A blank name is
    /// Uncategorized, as in a cell; an unknown one is created by the core.
    ///
    /// One undo step puts every row back where it was. The rows stay selected,
    /// so the next action can follow on the same ones.
    func setSelectionCategory(_ name: String) async {
        guard let vault = currentVault else { return }
        let category = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let forward = bulkTargets.map { RowPatch(id: $0.id, patch: TransactionPatch(category: category)) }
        guard !forward.isEmpty else { return }
        // Read before the write: undo puts back what is stored now.
        let inverse = forward.compactMap { change in
            storedInverse(of: change.patch, for: change.id).map { RowPatch(id: change.id, patch: $0) }
        }
        guard await apply(Self.updates(forward), in: vault.id) else { return }
        recordPatches(forward, inverse: inverse, vaultId: vault.id, name: String(localized: "Category Change"))
    }
}
