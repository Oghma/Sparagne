import Foundation

// Placeholder domain types mirroring the shapes the UniFFI-generated
// `SparagneCore` Swift package will eventually expose (see docs/v2/ARCH.md
// §2.2 and §7). These are UI-only stand-ins so the app can be built and
// previewed before that package exists. Replace with the generated types
// once `apple/SparagneCore` lands (see the commented-out `packages:` block
// in project.yml) and delete this file.

/// Mirrors the five transaction kinds in docs/v2/DISTILLATO_V1.md §1.2.
/// `TransferWallet`/`TransferFlow` legs move money between two wallets or
/// two flows (envelopes) respectively and never touch the income/expense
/// dashboards.
enum TransactionKind: String, CaseIterable, Identifiable, Sendable {
    case income
    case expense
    case refund
    case transferWallet
    case transferFlow

    var id: String { rawValue }
}

/// One row of the transactions table. A transaction is a header with one or
/// more signed legs (§1.2); the row form only needs the resolved
/// wallet/envelope names, since the kind is derivable from sign + targets.
struct TransactionRow: Identifiable, Hashable, Sendable {
    let id: UUID
    var occurredAt: Date
    var kind: TransactionKind
    /// Signed minor units (e.g. cents). Never a `Double`.
    var amountMinor: Int64
    var note: String
    var category: String?
    var wallet: String
    /// Destination wallet name, set only for `.transferWallet`.
    var walletDestination: String?
    var envelope: String
    /// Destination envelope name, set only for `.transferFlow`.
    var envelopeDestination: String?
    var isVoided: Bool

    /// `"Bank"` normally, `"Bank → Cash"` for a wallet transfer.
    var walletDisplay: String {
        guard let walletDestination else { return wallet }
        return "\(wallet) → \(walletDestination)"
    }

    /// `"Groceries"` normally, `"Unallocated → Vacation"` for a flow transfer.
    var envelopeDisplay: String {
        guard let envelopeDestination else { return envelope }
        return "\(envelope) → \(envelopeDestination)"
    }
}

/// Where money physically sits (cash, bank, card). Can go negative.
struct WalletRow: Identifiable, Hashable, Sendable {
    let id: UUID
    var name: String
    var balanceMinor: Int64
    var isArchived: Bool = false
}

/// What money is earmarked for (a "busta"). Stays >= 0 unless
/// `allowNegative`. `capMinor == nil` means `Unlimited`.
struct EnvelopeRow: Identifiable, Hashable, Sendable {
    let id: UUID
    var name: String
    var balanceMinor: Int64
    var capMinor: Int64?
    var allowNegative: Bool = false
    var isUnallocated: Bool = false

    /// `balance / cap` in `0...1`, or `nil` when there is no cap to show a bar for.
    var progress: Double? {
        guard let capMinor, capMinor > 0 else { return nil }
        return min(max(Double(balanceMinor) / Double(capMinor), 0), 1)
    }
}

/// A per-user container for wallets, envelopes, categories and transactions
/// (docs/v2/DISTILLATO_V1.md §1.1). The sidebar's vault picker is a stub
/// until accounts/vaults exist in the core.
struct VaultRow: Identifiable, Hashable, Sendable {
    let id: UUID
    var name: String
}

/// Fixture data driving the placeholder UI.
enum SampleData {
    static let currencyCode = "EUR"

    static let vaults: [VaultRow] = [
        VaultRow(id: UUID(), name: "Personal"),
        VaultRow(id: UUID(), name: "Home"),
    ]

    static let wallets: [WalletRow] = [
        WalletRow(id: UUID(), name: "Cash", balanceMinor: 4_250),
        WalletRow(id: UUID(), name: "Bank", balanceMinor: 182_430),
        WalletRow(id: UUID(), name: "Credit Card", balanceMinor: -12_500),
    ]

    static let envelopes: [EnvelopeRow] = [
        EnvelopeRow(id: UUID(), name: "Groceries", balanceMinor: 8_450, capMinor: 20_000),
        EnvelopeRow(id: UUID(), name: "Vacation", balanceMinor: 45_000, capMinor: nil),
        EnvelopeRow(id: UUID(), name: "Emergency", balanceMinor: 138_000, capMinor: 150_000),
        EnvelopeRow(id: UUID(), name: "Unallocated", balanceMinor: 30_680, capMinor: nil, isUnallocated: true),
    ]

    static let transactions: [TransactionRow] = {
        let calendar = Calendar.current
        let now = Date()
        func daysAgo(_ n: Int) -> Date { calendar.date(byAdding: .day, value: -n, to: now) ?? now }

        return [
            TransactionRow(id: UUID(), occurredAt: now, kind: .expense, amountMinor: -1_250, note: "Pizza", category: "Food", wallet: "Cash", walletDestination: nil, envelope: "Groceries", envelopeDestination: nil, isVoided: false),
            TransactionRow(id: UUID(), occurredAt: now, kind: .income, amountMinor: 250_000, note: "Salary", category: "Salary", wallet: "Bank", walletDestination: nil, envelope: "Unallocated", envelopeDestination: nil, isVoided: false),
            TransactionRow(id: UUID(), occurredAt: daysAgo(1), kind: .refund, amountMinor: 1_500, note: "Returned shoes", category: "Shopping", wallet: "Bank", walletDestination: nil, envelope: "Groceries", envelopeDestination: nil, isVoided: false),
            TransactionRow(id: UUID(), occurredAt: daysAgo(1), kind: .transferWallet, amountMinor: 10_000, note: "ATM withdrawal", category: nil, wallet: "Bank", walletDestination: "Cash", envelope: "Unallocated", envelopeDestination: nil, isVoided: false),
            TransactionRow(id: UUID(), occurredAt: daysAgo(3), kind: .transferFlow, amountMinor: 20_000, note: "Fund vacation", category: nil, wallet: "Bank", walletDestination: nil, envelope: "Unallocated", envelopeDestination: "Vacation", isVoided: false),
            TransactionRow(id: UUID(), occurredAt: daysAgo(5), kind: .expense, amountMinor: -4_200, note: "Train ticket", category: "Transport", wallet: "Bank", walletDestination: nil, envelope: "Groceries", envelopeDestination: nil, isVoided: true),
        ]
    }()
}
