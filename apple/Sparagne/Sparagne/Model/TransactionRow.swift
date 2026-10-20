import Foundation
import SparagneCore

/// Resolves the ids on a transaction's legs to display names.
///
/// The core returns legs, not names (`TransactionView.legs`), so the table
/// joins them against the vault snapshot it already loaded.
nonisolated struct NameBook: Sendable {
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
nonisolated struct TransactionRow: Identifiable, Hashable, Sendable {
    static let placeholder = "—"

    let id: Uuid
    let kind: TransactionKind
    private(set) var occurredAt: Date
    /// What the core stores: absolute minor units.
    private(set) var absoluteAmount: Int64
    let categoryId: Uuid
    /// Localized for the two system categories, verbatim otherwise.
    private(set) var category: String
    private(set) var note: String
    /// Who the row is for, the PERSONA column of the ledger: the author, unless
    /// the row was recorded on someone else's behalf.
    private(set) var person: String
    /// `created_by`: the member of the vault who entered the row. Shown only
    /// where it differs from `person`, as "recorded by".
    let recordedBy: String
    let voided: Bool

    /// Entries: the wallet leg. Transfers between wallets: the source.
    private(set) var walletId: Uuid?
    /// Entries: the envelope leg. Transfers between envelopes: the source.
    private(set) var flowId: Uuid?

    private(set) var walletDisplay: String
    private(set) var envelopeDisplay: String
    /// What the ENVELOPE column writes: an entry's envelope, a transfer
    /// between envelopes' source. The column has no room for two names, so
    /// the whole route goes after the description (`transferRoute`).
    private(set) var envelopeColumn: String

    var isTransfer: Bool { kind == .transferWallet || kind == .transferFlow }

    /// A transfer's two ends, "Unallocated → Spesa" or "Conto → Contanti":
    /// the ledger tags the description with it, where both names fit, so a
    /// transfer says where the money went. `nil` for an entry.
    var transferRoute: String? {
        switch kind {
        case .transferFlow: envelopeDisplay
        case .transferWallet: walletDisplay
        case .income, .expense, .refund: nil
        }
    }

    /// Recorded by one member for another: the PERSONA cell then says who
    /// typed it, on hover and to VoiceOver.
    var isOnBehalf: Bool { person != recordedBy }

    /// Whether the PERSONA cell opens for typing. A transfer always stays its
    /// author's (`TransactionPatch.person` is for entries only).
    var isPersonEditable: Bool { !isTransfer }

    init(view: TransactionView, names: NameBook) {
        id = view.id
        kind = view.kind
        occurredAt = CoreDate.date(view.occurredAt) ?? Date()
        absoluteAmount = view.amount
        categoryId = view.categoryId
        category = Self.categoryLabel(view.category, isSystem: view.categoryIsSystem)
        note = view.note ?? ""
        person = view.person
        recordedBy = view.createdBy
        voided = view.voided

        // The core already sorted the legs out by kind (`core/src/query.rs`).
        switch view.kind {
        case .transferWallet:
            walletId = view.fromId
            flowId = nil
            walletDisplay = Self.arrow(names.wallet(view.fromId), names.wallet(view.toId))
            envelopeDisplay = Self.placeholder
            envelopeColumn = Self.placeholder
        case .transferFlow:
            flowId = view.fromId
            walletId = nil
            walletDisplay = Self.placeholder
            envelopeDisplay = Self.arrow(names.flow(view.fromId), names.flow(view.toId))
            envelopeColumn = names.flow(view.fromId) ?? Self.placeholder
        case .income, .expense, .refund:
            walletId = view.walletId
            flowId = view.flowId
            walletDisplay = names.wallet(view.walletId) ?? Self.placeholder
            envelopeDisplay = names.flow(view.flowId) ?? Self.placeholder
            envelopeColumn = envelopeDisplay
        }
    }

    /// The row as `patch` leaves it once the core has it: what the ledger
    /// shows of a cell saved before a reload gives the row back
    /// (`LedgerEditor.shown`), and what the next cell of the row is worked
    /// out against meanwhile.
    ///
    /// The category is the name as typed, Uncategorized for a blank one, and
    /// its id the old one until then: the name may be a new category, or an
    /// alias the core files under another. A transfer's two ends are not
    /// patched here, as the grid never patches them.
    func applying(_ patch: TransactionPatch, names: NameBook) -> TransactionRow {
        var row = self
        if let amount = patch.amount { row.absoluteAmount = amount }
        if let stamp = patch.occurredAt, let date = CoreDate.date(stamp) { row.occurredAt = date }
        if let category = patch.category {
            row.category = category.isEmpty ? String(localized: "Uncategorized") : category
        }
        if let note = patch.note { row.note = note }
        // Blank is the patch's word for whoever recorded the row.
        if let person = patch.person { row.person = person.isEmpty ? recordedBy : person }
        if !isTransfer, let walletId = patch.walletId {
            row.walletId = walletId
            row.walletDisplay = names.wallet(walletId) ?? Self.placeholder
        }
        if !isTransfer, let flowId = patch.flowId {
            row.flowId = flowId
            row.envelopeDisplay = names.flow(flowId) ?? Self.placeholder
            row.envelopeColumn = row.envelopeDisplay
        }
        return row
    }

    /// System categories are named in English by the core (`Opening`,
    /// `Uncategorized`); the UI shows the localized term instead.
    static func categoryLabel(_ name: String, isSystem: Bool) -> String {
        guard isSystem else { return name }
        switch name.lowercased() {
        case "opening": return String(localized: "Opening")
        case "uncategorized": return String(localized: "Uncategorized")
        default: return name
        }
    }

    /// Whether a category is the system Uncategorized, by its stored name:
    /// the core keeps it in English whatever the window's language.
    static func isUncategorized(_ name: String, isSystem: Bool) -> Bool {
        isSystem && name.lowercased() == "uncategorized"
    }

    private static func arrow(_ from: String?, _ to: String?) -> String {
        "\(from ?? placeholder) → \(to ?? placeholder)"
    }
}
