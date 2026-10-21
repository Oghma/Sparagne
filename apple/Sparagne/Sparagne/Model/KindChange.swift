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
    ///
    /// The typographic minus is how the ledger shows a refund, so an amount
    /// copied off the grid reads as it was shown. A sign alone leaves the
    /// number empty, which the cell refuses as it refuses any missing amount.
    static func sign(of text: String) -> (kind: TransactionKind?, number: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let kind: TransactionKind? = switch trimmed.first {
        case "+": .income
        case "-", "\u{2212}": .expense
        case "r", "R": .refund
        default: nil
        }
        guard kind != nil else { return (nil, trimmed) }
        return (kind, String(trimmed.dropFirst()).trimmingCharacters(in: .whitespacesAndNewlines))
    }

    /// The kinds `view` may become, in `entryKinds` order; none for a
    /// transfer, a voided or an opening row.
    static func targets(of view: TransactionView) -> [TransactionKind] {
        entryKinds.filter { refusal(of: view, to: $0) == nil }
    }

    /// Why `view` cannot become `kind`, `nil` when it can. A transfer is
    /// never a target either: an entry has no second end to give it.
    static func refusal(of view: TransactionView, to kind: TransactionKind) -> Refusal? {
        if isTransfer(view.kind) || isTransfer(kind) { return .transfer }
        if view.voided { return .voided }
        if TransactionRow.isOpening(view.category, isSystem: view.categoryIsSystem) { return .opening }
        if view.kind == kind { return .sameKind }
        return nil
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
        if let refusal = refusal(of: view, to: kind) { return .failure(refusal) }
        // The author compared as `AppStore.explicitPerson` compares it: the
        // local name and the synced username of one person differ in case.
        let person = view.person.lowercased() == author.lowercased() ? nil : view.person
        let entry = Entry(
            amount: amount ?? view.amount,
            walletId: view.walletId,
            flowId: view.flowId,
            category: AppStore.storedCategory(view),
            note: view.note.flatMap { $0.isEmpty ? nil : $0 },
            occurredAt: view.occurredAt,
            person: person
        )
        let create: Command
        switch kind {
        case .expense: create = .expense(entry)
        case .income: create = .income(entry)
        case .refund: create = .refund(entry)
        case .transferWallet, .transferFlow: return .failure(.transfer)
        }
        return .success([.voidTransaction(transactionId: view.id), create])
    }

    private static func isTransfer(_ kind: TransactionKind) -> Bool {
        kind == .transferWallet || kind == .transferFlow
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
