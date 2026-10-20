import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// A `TransactionRow` made by hand, for the pure models over rows (the
/// cursor, the selection's figures, ⌫'s target, the CSV) that need no vault.
/// No name resolves but `wallet`'s, so no other case may depend on one.
enum TestRow {
    /// A row of `kind`, today unless `occurredAt` says otherwise. A transfer
    /// has the two ends and no wallet or envelope, any other kind the
    /// opposite. `recordedBy` defaults to `person`; a `wallet` names the
    /// row's wallet.
    static func make(
        _ id: Uuid = UUID().uuidString,
        kind: TransactionKind = .expense,
        amount: Int64 = 1_250,
        category: String = "Spesa",
        note: String? = nil,
        person: String = "matteo",
        recordedBy: String? = nil,
        voided: Bool = false,
        occurredAt: Date = Date(),
        wallet: String? = nil
    ) -> TransactionRow {
        let isTransfer = kind == .transferWallet || kind == .transferFlow
        let walletId = UUID().uuidString
        let view = TransactionView(
            id: id,
            kind: kind,
            occurredAt: CoreDate.offset(occurredAt),
            amount: amount,
            categoryId: UUID().uuidString,
            category: category,
            categoryIsSystem: false,
            note: note,
            person: person,
            createdBy: recordedBy ?? person,
            voided: voided,
            walletId: isTransfer ? nil : walletId,
            flowId: isTransfer ? nil : UUID().uuidString,
            fromId: isTransfer ? UUID().uuidString : nil,
            toId: isTransfer ? UUID().uuidString : nil,
            legs: []
        )
        let snapshot = wallet.map {
            VaultSnapshot(
                id: UUID().uuidString,
                name: "Casa",
                currency: .eur,
                wallets: [WalletView(id: walletId, name: $0, balance: 0, archived: false)],
                flows: [],
                unallocatedFlowId: UUID().uuidString
            )
        }
        return TransactionRow(view: view, names: NameBook(snapshot: snapshot))
    }
}
