import Foundation
import SparagneCore

/// The Patrimonio's three cards and its status line, as of today: what the
/// wallets hold, what the loans still owe, and the difference.
///
/// Today's figures whatever the month on screen, and the household's whatever
/// the PERSONA: a debt is not one person's, and a balance is not a month's.
struct WealthFigures: Equatable, Sendable {
    /// An active wallet and its balance, for the Liquidità card's caption.
    struct Wallet: Equatable, Sendable {
        let name: String
        let balance: Int64
    }

    /// The active wallets, archived ones left out, in the vault's order.
    let wallets: [Wallet]
    /// What the active wallets hold together.
    let liquidity: Int64
    /// What the active loans still owe after today's installments.
    let debt: Int64
    /// The loans still being repaid: not archived, with some debt left.
    let activeLoans: Int
    /// What the active loans' next installments add up to, fees included:
    /// what the loans take out of the wallets in a month.
    let monthlyInstallments: Int64

    /// Liquidità − debito: below zero while the loans owe more than the
    /// wallets hold.
    var netWorth: Int64 { liquidity - debt }

    /// The figures of `wallets` and `loans`; `overview` is the core's, which
    /// covers the loans not archived. A loan archived, or not in the
    /// overview, counts for nothing: archiving is how a loan leaves the
    /// figures.
    static func build(wallets: [WalletView], loans: [LoanView], overview: [LoanOverview]) -> WealthFigures {
        let active = wallets.filter { !$0.archived }
        let running = Set(loans.filter { !$0.archived }.map(\.id))
        let owing = overview.filter { running.contains($0.loanId) && $0.residual > 0 }
        return WealthFigures(
            wallets: active.map { Wallet(name: $0.name, balance: $0.balance) },
            liquidity: active.reduce(0) { $0 + $1.balance },
            debt: owing.reduce(0) { $0 + $1.residual },
            activeLoans: owing.count,
            monthlyInstallments: owing.reduce(0) { $0 + ($1.nextPayment ?? 0) }
        )
    }
}
