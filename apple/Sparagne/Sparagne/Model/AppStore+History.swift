import Foundation
import SparagneCore

/// Edit ▸ Undo and Redo for the ledger: the step each write records, and its inverse.
extension AppStore {
    // An inverse is what the core stores, never what the grid shows: the stored category,
    // `""` for Uncategorized (its label typed back would create a category), `""` for no
    // note and for a row that was its recorder's own.

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
    /// row cannot come back; the step follows it there. The toast that shows
    /// where a signed row went goes with the row.
    func recordAddedRow(_ command: Command, id: Uuid, vaultId: Uuid) {
        history.record(rows: [id]) { target in
            LedgerStep(
                name: String(localized: "Add Row"),
                undo: { [weak self] in
                    guard let self, let id = target.rows.first else { return }
                    guard await apply([.voidTransaction(transactionId: id)], in: vaultId) else { return }
                    if kindMoveNotice?.transactionId == id { kindMoveNotice = nil }
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
        if patch.person != nil { inverse.person = storedPerson(view) }
        return inverse
    }

    /// `view` as the core will store it once `patch` lands: the mirror of
    /// `inverse(of:from:)`, blank as the patch's word for Uncategorized and
    /// for whoever recorded the row. What a kind change reads a row through
    /// when cells saved into it are still on their way (`LedgerEditor`). The
    /// category id stays the old one: a name is the core's to resolve.
    nonisolated static func view(_ view: TransactionView, applying patch: TransactionPatch) -> TransactionView {
        var view = view
        if let amount = patch.amount { view.amount = amount }
        if let occurredAt = patch.occurredAt { view.occurredAt = occurredAt }
        if let category = patch.category {
            view.category = category.isEmpty ? "Uncategorized" : category
            view.categoryIsSystem = category.isEmpty
        }
        if let note = patch.note { view.note = note.isEmpty ? nil : note }
        if let walletId = patch.walletId { view.walletId = walletId }
        if let flowId = patch.flowId { view.flowId = flowId }
        if let fromId = patch.fromId { view.fromId = fromId }
        if let toId = patch.toId { view.toId = toId }
        if let person = patch.person { view.person = person.isEmpty ? view.createdBy : person }
        return view
    }

    /// What `UpdateTransaction` needs to give a row back to whoever it was
    /// for: blank when that was the one who recorded it, which is what the
    /// patch calls "back to `created_by`", and the name otherwise.
    static func storedPerson(_ view: TransactionView) -> String {
        view.person == view.createdBy ? "" : view.person
    }

    /// What `UpdateTransaction` needs to file a row back where it was: the
    /// stored name, or blank for Uncategorized. Also what a kind change files
    /// the new row under (`KindChange`).
    nonisolated static func storedCategory(_ view: TransactionView) -> String {
        TransactionRow.isUncategorized(view.category, isSystem: view.categoryIsSystem) ? "" : view.category
    }

    static func updates(_ patches: [RowPatch]) -> [Command] {
        patches.map { .updateTransaction(transactionId: $0.id, patch: $0.patch) }
    }
}
