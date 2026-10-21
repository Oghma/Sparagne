import Foundation
import SparagneCore

/// Turning an entry into another kind: expense, income or refund.
///
/// A patch never changes the kind (`TransactionPatch`), so the row is voided
/// and written again with the same fields, the void first so the core checks
/// the new row against the balances it leaves. One batch, all or none, on
/// this device; the server takes each command on its own.
nonisolated enum KindChange {
    /// Why a row cannot change kind.
    enum Refusal: Error, Equatable, Sendable {
        /// A transfer has two ends where an entry has a wallet and an envelope.
        case transfer
        case voided
        /// A wallet's or an envelope's opening balance, filed under the system
        /// category Opening.
        case opening
        /// The row already is that kind.
        case sameKind
    }

    /// The kinds an entry can be.
    static let entryKinds: [TransactionKind] = [.expense, .income, .refund]

    /// What the amount cell's text says, by the quick-add grammar
    /// (`core/src/quick_add.rs`): the kind a leading `+` (income), `-` or `−`
    /// (expense) or `r` (refund) names, `nil` without one, and the number
    /// after it, trimmed.
    static func sign(of text: String) -> (kind: TransactionKind?, number: String) {
        (nil, text)
    }

    /// The kinds `view` may become, in `entryKinds` order; none for a
    /// transfer, a voided or an opening row.
    static func targets(of view: TransactionView) -> [TransactionKind] {
        []
    }

    /// The batch that turns `view` into `kind`: its void, then the same entry
    /// as `kind`, at `amount` when given, else at its own. The date keeps its
    /// offset, the category is the stored name (blank for Uncategorized), and
    /// the person is named unless it is `author`, who the core falls back to.
    static func commands(
        from view: TransactionView,
        to kind: TransactionKind,
        amount: Int64? = nil,
        author: String
    ) -> Result<[Command], Refusal> {
        .failure(.sameKind)
    }
}

/// The toast after a kind change took rows off the list on screen: they went to the other direction.
struct KindMoveNotice: Identifiable, Equatable, Sendable, UndoCountdown {
    let id = UUID()
    let vaultId: Uuid
    /// The first row moved, the one Mostra shows.
    let transactionId: Uuid
    let occurredAt: OffsetDateTime
    /// Where the rows went.
    let direction: LedgerDirection
    let count: Int
    let startedAt: Date
    let duration: Duration
}
