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

    /// "Void N Rows", ⌫ and ⌦: one pending void for every target, so one toast
    /// to take it back and, when it elapses, one batch.
    func voidSelection() async {
        let ids = bulkTargets.map(\.id)
        guard !ids.isEmpty else { return }
        selection.clear()
        await void(transactionIds: ids)
    }
}
