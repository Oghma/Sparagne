import Foundation
import SparagneCore

/// Resolves the ids on a transaction's legs to display names.
///
/// The core returns legs, not names (`TransactionView.legs`), so the table
/// joins them against the vault snapshot it already loaded.
struct NameBook: Sendable {
    private var wallets: [Uuid: String] = [:]
    private var flows: [Uuid: String] = [:]

    /// The Unallocated envelope has the internal name `unallocated`
    /// (`core/src/flow.rs`); the UI shows the localized term instead.
    static var unallocatedLabel: String { String(localized: "Unallocated") }

    init(snapshot: VaultSnapshot?) {
        guard let snapshot else { return }
        for wallet in snapshot.wallets { wallets[wallet.id] = wallet.name }
        for flow in snapshot.flows {
            flows[flow.id] = flow.isUnallocated ? Self.unallocatedLabel : flow.name
        }
    }

    func wallet(_ id: Uuid?) -> String? { id.flatMap { wallets[$0] } }
    func flow(_ id: Uuid?) -> String? { id.flatMap { flows[$0] } }
}

/// One row of the transactions table: a `TransactionView` with its legs
/// resolved and its amount signed for display.
struct TransactionRow: Identifiable, Hashable, Sendable {
    static let placeholder = "—"

    let id: Uuid
    let kind: TransactionKind
    let occurredAt: Date
    /// What the core stores: absolute minor units.
    let absoluteAmount: Int64
    /// Signed for display: expenses negative, income and refunds positive,
    /// transfers left positive and shown in the neutral transfer color.
    let signedAmount: Int64
    let categoryId: Uuid
    let category: String
    let note: String
    let voided: Bool

    /// Entries: the wallet leg. Transfers between wallets: the source.
    let walletId: Uuid?
    /// Entries: the envelope leg. Transfers between envelopes: the source.
    let flowId: Uuid?
    /// Transfers only: the destination, a wallet or a flow matching the kind.
    let destinationId: Uuid?

    let walletDisplay: String
    let envelopeDisplay: String

    var isTransfer: Bool { kind == .transferWallet || kind == .transferFlow }

    init(view: TransactionView, names: NameBook) {
        id = view.id
        kind = view.kind
        occurredAt = CoreDate.date(view.occurredAt) ?? Date()
        absoluteAmount = view.amount
        signedAmount = view.kind == .expense ? -view.amount : view.amount
        categoryId = view.categoryId
        category = view.category
        note = view.note ?? ""
        voided = view.voided

        var walletLegs: [(id: Uuid, amount: Int64)] = []
        var flowLegs: [(id: Uuid, amount: Int64)] = []
        for leg in view.legs {
            switch leg.target {
            case .wallet(let walletId): walletLegs.append((walletId, leg.amount))
            case .flow(let flowId): flowLegs.append((flowId, leg.amount))
            }
        }

        switch view.kind {
        case .transferWallet:
            let source = walletLegs.first { $0.amount < 0 } ?? walletLegs.first
            let target = walletLegs.first { $0.amount > 0 } ?? walletLegs.last
            walletId = source?.id
            destinationId = target?.id
            flowId = nil
            walletDisplay = Self.arrow(names.wallet(source?.id), names.wallet(target?.id))
            envelopeDisplay = Self.placeholder
        case .transferFlow:
            let source = flowLegs.first { $0.amount < 0 } ?? flowLegs.first
            let target = flowLegs.first { $0.amount > 0 } ?? flowLegs.last
            flowId = source?.id
            destinationId = target?.id
            walletId = nil
            walletDisplay = Self.placeholder
            envelopeDisplay = Self.arrow(names.flow(source?.id), names.flow(target?.id))
        case .income, .expense, .refund:
            walletId = walletLegs.first?.id
            flowId = flowLegs.first?.id
            destinationId = nil
            walletDisplay = names.wallet(walletLegs.first?.id) ?? Self.placeholder
            envelopeDisplay = names.flow(flowLegs.first?.id) ?? Self.placeholder
        }
    }

    private static func arrow(_ from: String?, _ to: String?) -> String {
        "\(from ?? placeholder) → \(to ?? placeholder)"
    }
}
