import Foundation
import SparagneCore

/// Edit ▸ Undo and Redo for the ledger (`LedgerHistory`): the step each write
/// records, and the inverse it is built from.
///
/// An inverse is made of what the core stores, never of what the grid shows:
/// the stored category name, or `""` for Uncategorized (its localized label
/// typed back would create a category of that name), `""` for no note, the
/// original `occurred_at` with its own offset.
extension AppStore {
    /// The window's undo manager, handed over by `LedgerWindow`.
    func attach(undoManager: UndoManager?) {
        history.attach(undoManager)
    }

    /// Records patches just written, as one step: undo writes `inverse` and
    /// redo `forward`, each as one batch, so a bulk change comes back whole or
    /// not at all.
    func recordPatches(_ forward: [RowPatch], inverse: [RowPatch], vaultId: Uuid, name: String) {
        guard !forward.isEmpty, inverse.count == forward.count else { return }
        history.record(rows: Set(forward.map(\.id))) { _ in
            LedgerStep(
                name: name,
                undo: { [weak self] in _ = await self?.apply(Self.updates(inverse), in: vaultId) },
                redo: { [weak self] in _ = await self?.apply(Self.updates(forward), in: vaultId) }
            )
        }
    }

    /// Records a row just added. Undo voids it at once, not through the toast:
    /// taking back what was just typed is the whole point, there is nothing to
    /// confirm. Redo adds it again as a new row, under a new id, since a voided
    /// row cannot come back; the step follows it there.
    func recordAddedRow(_ command: Command, id: Uuid, vaultId: Uuid) {
        history.record(rows: [id]) { target in
            LedgerStep(
                name: String(localized: "Add Row"),
                undo: { [weak self] in
                    guard let id = target.rows.first else { return }
                    _ = await self?.apply([.voidTransaction(transactionId: id)], in: vaultId)
                },
                redo: { [weak self] in
                    guard let created = await self?.applyMinted(command, in: vaultId) else { return }
                    target.rows = [created]
                }
            )
        }
    }

    /// The patch that puts back what is stored now for every field `patch`
    /// changes. `nil` when the row is not loaded: the edit is then not
    /// undoable, rather than undone with a guess.
    func storedInverse(of patch: TransactionPatch, for id: Uuid) -> TransactionPatch? {
        guard let view = transactions.first(where: { $0.id == id }) else { return nil }
        return Self.inverse(of: patch, from: view)
    }

    static func inverse(of patch: TransactionPatch, from view: TransactionView) -> TransactionPatch {
        var inverse = TransactionPatch()
        if patch.amount != nil { inverse.amount = view.amount }
        if patch.occurredAt != nil { inverse.occurredAt = view.occurredAt }
        if patch.category != nil { inverse.category = storedCategory(view) }
        if patch.note != nil { inverse.note = view.note ?? "" }
        if patch.walletId != nil { inverse.walletId = view.walletId }
        if patch.flowId != nil { inverse.flowId = view.flowId }
        if patch.fromId != nil { inverse.fromId = view.fromId }
        if patch.toId != nil { inverse.toId = view.toId }
        return inverse
    }

    /// What `UpdateTransaction` needs to file a row back where it was: the
    /// stored name, or blank for Uncategorized.
    static func storedCategory(_ view: TransactionView) -> String {
        view.categoryIsSystem && view.category.lowercased() == "uncategorized" ? "" : view.category
    }

    static func updates(_ patches: [RowPatch]) -> [Command] {
        patches.map { .updateTransaction(transactionId: $0.id, patch: $0.patch) }
    }
}
