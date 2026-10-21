import Foundation
import SparagneCore

/// Changing an entry's kind, and the transfer actions of the ledger.
extension AppStore {
    // A kind change voids the row and writes it again (`KindChange`), under an id the core
    // mints: one derived from the old row would read as a statement import's. The steps
    // about the old id could only be refused now, so they are forgotten, and the change's
    // own step follows the row to each id its undo and redo give it.

    /// How long the toast after a kind change stays up.
    static let kindMoveNoticeDuration: Duration = .seconds(5)

    /// The kinds the row with `rowId` may become: none in a read-only vault
    /// or for a row `KindChange.targets` refuses.
    func kindTargets(of rowId: Uuid) -> [TransactionKind] {
        guard canWrite, let view = loadedView(rowId) else { return [] }
        return KindChange.targets(of: view)
    }

    /// The kinds the rows of `ids` that can change may become: what the
    /// selection bar and its menu offer. A kind is offered when one row at
    /// least can become it, in `entryKinds` order; the change passes over
    /// the rows that already are it.
    func kindTargets(rows ids: [Uuid]) -> [TransactionKind] {
        guard canWrite else { return [] }
        let targets = Set(ids.compactMap { loadedView($0) }.flatMap { KindChange.targets(of: $0) })
        return KindChange.entryKinds.filter { targets.contains($0) }
    }

    /// Turns the row into `kind`, at `amount` when given, as one batch and
    /// one undo step. Returns the new row's id; `nil` when nothing changed,
    /// with the alert up for a refusal. Puts up `kindMoveNotice` when the row
    /// leaves the direction on screen.
    @discardableResult
    func changeKind(of rowId: Uuid, to kind: TransactionKind, amount: Int64? = nil) async -> Uuid? {
        await changeKind(of: rowId, to: kind, amount: amount, loaded: nil)
    }

    /// `changeKind(of:to:amount:)`, with `loaded` standing in for the row
    /// when the list on screen no longer holds it by the time the write
    /// runs: the ledger reads the row as its IMPORTO cell is left, which may
    /// be on the way to another month.
    @discardableResult
    func changeKind(of rowId: Uuid, to kind: TransactionKind, amount: Int64?, loaded: TransactionView?) async -> Uuid? {
        guard let vault = currentVault, !refusedAsReadOnly() else { return nil }
        // A void waiting on the toast goes out first, as before any other.
        await flushPendingUndo()
        guard let view = loadedView(rowId) ?? loaded else { return nil }
        if let refusal = KindChange.refusal(of: view, to: kind) {
            if let error = Self.kindChangeError(refusal) { presentedError = error }
            return nil
        }
        return await changeKinds([(view, amount)], to: kind, in: vault.id)[rowId]
    }

    /// Every row of `ids` that can become `kind`, in one batch and one undo
    /// step; the others are passed over. Returns each old id's new one.
    /// What was selected stays selected, under the new ids.
    @discardableResult
    func changeKind(rows ids: [Uuid], to kind: TransactionKind) async -> [Uuid: Uuid] {
        guard let vault = currentVault, !refusedAsReadOnly() else { return [:] }
        await flushPendingUndo()
        let views = ids.compactMap { loadedView($0) }.filter { KindChange.refusal(of: $0, to: kind) == nil }
        guard !views.isEmpty else { return [:] }
        return await changeKinds(views.map { ($0, nil) }, to: kind, in: vault.id)
    }

    /// Whether the row with `rowId` is refused `kind`, the reason then on
    /// the alert: what the ledger's IMPORTO cell asks before it lets go of a
    /// sign, so the cell stays open on a refusal. A row already of that kind
    /// is no refusal, only nothing to do.
    func refusesKindChange(of rowId: Uuid, to kind: TransactionKind) -> Bool {
        guard let view = loadedView(rowId), let refusal = KindChange.refusal(of: view, to: kind),
              let error = Self.kindChangeError(refusal)
        else { return false }
        presentedError = error
        return true
    }

    /// Mostra on the toast: the ledger on the row's month and direction, the
    /// row selected (`reveal`).
    func showMovedRow(_ notice: KindMoveNotice) {
        dismissKindMoveNotice(notice.id)
        guard notice.vaultId == currentVault?.id else { return }
        reveal(transactionId: notice.transactionId, occurredAt: notice.occurredAt, direction: notice.direction)
    }

    /// The toast's window elapsed, or it was closed.
    func dismissKindMoveNotice(_ id: UUID) {
        if kindMoveNotice?.id == id { kindMoveNotice = nil }
    }

    /// A copy of the transfer with `rowId`, same ends, amount and note, dated
    /// today, written at once and undoable. Returns the copy's id.
    ///
    /// The copy keeps the transfer's time of day, as a row moved to another
    /// day does. ⌘Z voids it, as it voids any row just added.
    @discardableResult
    func duplicateTransfer(_ rowId: Uuid) async -> Uuid? {
        guard let vault = currentVault, !refusedAsReadOnly(), let view = loadedView(rowId), !view.voided,
              let from = view.fromId, let to = view.toId
        else { return nil }
        let occurredAt = Self.stamp(day: Date(), likeTimeOf: CoreDate.date(view.occurredAt) ?? Date())
        let note = view.note.flatMap { $0.isEmpty ? nil : $0 }
        let command: Command
        switch view.kind {
        case .transferWallet:
            command = .transferWallet(amount: view.amount, fromWalletId: from, toWalletId: to, note: note, occurredAt: occurredAt)
        case .transferFlow:
            command = .transferFlow(amount: view.amount, fromFlowId: from, toFlowId: to, note: note, occurredAt: occurredAt)
        case .income, .expense, .refund:
            return nil
        }
        guard let created = await applyMinted(command, in: vault.id) else { return nil }
        recordAddedRow(command, id: created, vaultId: vault.id)
        return created
    }

    // MARK: - The change

    /// The row with `id` as the core keeps it: the fields a kind change
    /// writes again, untouched by what the grid localizes.
    private func loadedView(_ id: Uuid) -> TransactionView? {
        transactions.first { $0.id == id }
    }

    /// `changes`, each row with its new amount if any, as one batch and one
    /// undo step. Returns each old id's new one; none when the batch was
    /// refused, which leaves every row as it was.
    private func changeKinds(
        _ changes: [(view: TransactionView, amount: Int64?)],
        to kind: TransactionKind,
        in vaultId: Uuid
    ) async -> [Uuid: Uuid] {
        let trail = KindChangeTrail(changes.map { change in
            var after = change.view
            after.kind = kind
            after.amount = change.amount ?? change.view.amount
            return KindChangeTrail.Row(current: change.view.id, before: change.view, after: after)
        })
        let selected = changes.map(\.view.id).filter { selection.contains($0) }
        guard let created = await rewrite(trail, back: false, in: vaultId) else { return [:] }
        history.forget(rows: Set(created.keys))
        recordKindChange(trail, vaultId: vaultId)
        // The reload dropped the old ids from the selection: the new ones
        // take their place, where they are on screen.
        for new in selected.compactMap({ created[$0] }) where rows.contains(where: { $0.id == new }) {
            if !selection.contains(new) { selection.toggle(new) }
        }
        // Expense and refund share USCITE: only a move to or from income
        // takes the rows off the list on screen.
        if !direction.kinds.contains(kind), let first = changes.first, let moved = created[first.view.id] {
            kindMoveNotice = KindMoveNotice(
                vaultId: vaultId,
                transactionId: moved,
                occurredAt: first.view.occurredAt,
                direction: kind == .income ? .income : .expenses,
                count: created.count,
                startedAt: Date(),
                duration: Self.kindMoveNoticeDuration
            )
        }
        return created
    }

    /// One step for the whole change. Undo turns the new rows back into
    /// what they were, redo turns them again, each under new ids the step
    /// follows; the steps about the ids voided on the way are forgotten.
    private func recordKindChange(_ trail: KindChangeTrail, vaultId: Uuid) {
        history.record(rows: trail.ids) { target in
            LedgerStep(
                name: String(localized: "Kind Change"),
                undo: { [weak self] in await self?.replay(trail, back: true, target: target, in: vaultId) },
                redo: { [weak self] in await self?.replay(trail, back: false, target: target, in: vaultId) }
            )
        }
    }

    private func replay(_ trail: KindChangeTrail, back: Bool, target: LedgerHistory.Target, in vaultId: Uuid) async {
        let voided = trail.ids
        guard await rewrite(trail, back: back, in: vaultId) != nil else { return }
        // The step moves first: it is about the old ids too until then, and
        // would be forgotten with them.
        target.rows = trail.ids
        history.forget(rows: voided)
        if let notice = kindMoveNotice, voided.contains(notice.transactionId) { kindMoveNotice = nil }
    }

    /// Voids every row of `trail` and writes it again as the other version,
    /// `before` when going `back`, in one batch under envelopes the core
    /// mints, so each new id is known before the batch runs. The rows keep
    /// every field but the kind and the amount. Moves each row to its new
    /// id, and returns them by old id; `nil` when refused.
    private func rewrite(_ trail: KindChangeTrail, back: Bool, in vaultId: Uuid) async -> [Uuid: Uuid]? {
        var envelopes: [CommandEnvelope] = []
        var created: [Uuid: Uuid] = [:]
        for row in trail.rows {
            var source = back ? row.after : row.before
            source.id = row.current
            let goal = back ? row.before : row.after
            let built = KindChange.commands(from: source, to: goal.kind, amount: goal.amount, author: currentAuthor)
            guard case .success(let commands) = built else { return nil }
            for command in commands {
                let minted = await core.envelope(vaultId: vaultId, command)
                envelopes.append(minted)
                // The entry is created under its envelope's id; the void
                // creates nothing.
                if case .voidTransaction = command { continue }
                created[row.current] = minted.id
            }
        }
        guard await apply(envelopes: envelopes, in: vaultId) else { return nil }
        trail.move(to: created)
        return created
    }

    /// What the alert says when a row cannot change kind. Nothing for one
    /// that already is that kind: there is nothing to change.
    static func kindChangeError(_ refusal: KindChange.Refusal) -> AppError? {
        let headline: String
        let message: String
        switch refusal {
        case .transfer:
            headline = String(localized: "A transfer cannot change kind")
            message = String(localized: "It moves money between two wallets or two envelopes: type its amount without a sign.")
        case .voided:
            headline = String(localized: "A deleted row cannot change kind")
            message = String(localized: "It was deleted, on this Mac or another, before the change reached it.")
        case .opening:
            headline = String(localized: "An opening balance cannot change kind")
            message = String(localized: "It is the balance a wallet or an envelope started from: type its amount without a sign.")
        case .sameKind:
            return nil
        }
        return AppError(code: "invalid_command", message: message, headline: headline)
    }
}

/// The rows of one kind change, as its undo and redo move them from id to id.
private final class KindChangeTrail {
    struct Row {
        /// The id the row has now: the one the last write gave it.
        var current: Uuid
        /// The row as it was, and as the change made it: the same fields
        /// under another kind, and another amount when one was typed.
        let before: TransactionView
        let after: TransactionView
    }

    private(set) var rows: [Row]

    var ids: Set<Uuid> { Set(rows.map(\.current)) }

    init(_ rows: [Row]) {
        self.rows = rows
    }

    func move(to created: [Uuid: Uuid]) {
        for index in rows.indices {
            if let new = created[rows[index].current] { rows[index].current = new }
        }
    }
}
