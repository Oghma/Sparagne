import Foundation
import SparagneCore

/// Changing an entry's kind, and the transfer actions of the ledger.
extension AppStore {
    /// How long the toast after a kind change stays up.
    static let kindMoveNoticeDuration: Duration = .seconds(5)

    /// The kinds the row with `rowId` may become: none in a read-only vault
    /// or for a row `KindChange.targets` refuses.
    func kindTargets(of rowId: Uuid) -> [TransactionKind] {
        []
    }

    /// The kinds every row of `ids` that can change may become: what the
    /// selection bar and its menu offer.
    func kindTargets(rows ids: [Uuid]) -> [TransactionKind] {
        []
    }

    /// Turns the row into `kind`, at `amount` when given, as one batch and
    /// one undo step. Returns the new row's id; `nil` when nothing changed,
    /// with the alert up for a refusal. Puts up `kindMoveNotice` when the row
    /// leaves the direction on screen.
    @discardableResult
    func changeKind(of rowId: Uuid, to kind: TransactionKind, amount: Int64? = nil) async -> Uuid? {
        nil
    }

    /// Every row of `ids` that can become `kind`, in one batch and one undo
    /// step; the others are passed over. Returns each old id's new one.
    @discardableResult
    func changeKind(rows ids: [Uuid], to kind: TransactionKind) async -> [Uuid: Uuid] {
        [:]
    }

    /// Mostra on the toast: the ledger on the row's month and direction, the
    /// row selected (`reveal`).
    func showMovedRow(_ notice: KindMoveNotice) {}

    func dismissKindMoveNotice(_ id: UUID) {}

    /// A copy of the transfer with `rowId`, same ends, amount and note, dated
    /// today, written at once and undoable. Returns the copy's id.
    @discardableResult
    func duplicateTransfer(_ rowId: Uuid) async -> Uuid? {
        nil
    }
}
