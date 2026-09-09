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

/// One row of the transactions table: a `TransactionView` with its ids
/// resolved to names and its amount signed for display.
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
    /// Localized for the two system categories, verbatim otherwise.
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
        category = Self.categoryLabel(view)
        note = view.note ?? ""
        voided = view.voided

        // The core already sorted the legs out by kind (`core/src/query.rs`).
        switch view.kind {
        case .transferWallet:
            walletId = view.fromId
            destinationId = view.toId
            flowId = nil
            walletDisplay = Self.arrow(names.wallet(view.fromId), names.wallet(view.toId))
            envelopeDisplay = Self.placeholder
        case .transferFlow:
            flowId = view.fromId
            destinationId = view.toId
            walletId = nil
            walletDisplay = Self.placeholder
            envelopeDisplay = Self.arrow(names.flow(view.fromId), names.flow(view.toId))
        case .income, .expense, .refund:
            walletId = view.walletId
            flowId = view.flowId
            destinationId = nil
            walletDisplay = names.wallet(view.walletId) ?? Self.placeholder
            envelopeDisplay = names.flow(view.flowId) ?? Self.placeholder
        }
    }

    /// System categories are named in English by the core (`Opening`,
    /// `Uncategorized`); the UI shows the localized term instead.
    private static func categoryLabel(_ view: TransactionView) -> String {
        guard view.categoryIsSystem else { return view.category }
        switch view.category.lowercased() {
        case "opening": return String(localized: "Opening")
        case "uncategorized": return String(localized: "Uncategorized")
        default: return view.category
        }
    }

    private static func arrow(_ from: String?, _ to: String?) -> String {
        "\(from ?? placeholder) → \(to ?? placeholder)"
    }
}
