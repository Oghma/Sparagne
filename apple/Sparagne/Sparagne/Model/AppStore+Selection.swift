import Foundation
import SparagneCore

/// Row selection and the bulk actions over it: ⌘-click, ⇧-click and ⌘A pick
/// rows of the ledger, the selection bar voids them, files them under one
/// category or moves them to one envelope.
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
        let category = name.trimmingCharacters(in: .whitespacesAndNewlines)
        await patchTargets(
            bulkTargets.map { RowPatch(id: $0.id, patch: TransactionPatch(category: category)) },
            step: String(localized: "Category Change")
        )
    }

    /// "Set Envelope…": every target moved to `flowId` in one batch, all or
    /// nothing, as "Set Category…" is. A row already on it is left out, so
    /// nothing is sent, and nothing is undone, for it. An envelope that may
    /// not go negative refuses the batch when the rows would take it below
    /// zero, and every row stays where it was.
    ///
    /// One undo step puts every row back on its own envelope. The rows stay
    /// selected, so the next action can follow on the same ones.
    func setSelectionEnvelope(_ flowId: Uuid) async {
        await patchTargets(
            bulkTargets.filter { $0.flowId != flowId }.map { RowPatch(id: $0.id, patch: TransactionPatch(flowId: flowId)) },
            step: String(localized: "Envelope Change")
        )
    }

    /// `forward` as one batch and one undo step named `step`.
    private func patchTargets(_ forward: [RowPatch], step: String) async {
        guard let vault = currentVault, !forward.isEmpty else { return }
        // Read before the write: undo puts back what is stored now.
        let inverse = forward.compactMap { change in
            storedInverse(of: change.patch, for: change.id).map { RowPatch(id: change.id, patch: $0) }
        }
        guard await apply(Self.updates(forward), in: vault.id) else { return }
        recordPatches(forward, inverse: inverse, vaultId: vault.id, name: step)
    }

    /// The envelope every target is on, when they all share one: the list of
    /// "Set Envelope…" ticks it. `nil` when they are spread over several.
    var bulkEnvelope: Uuid? {
        let envelopes = Set(bulkTargets.map(\.flowId))
        guard envelopes.count == 1 else { return nil }
        return envelopes.first ?? nil
    }
}
