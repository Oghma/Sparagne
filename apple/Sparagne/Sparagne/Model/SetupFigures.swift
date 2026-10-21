import Foundation
import SparagneCore

/// What the Setup tables work out from the store's lists, once per drawing
/// rather than once per row.
enum SetupFigures {
    /// What the wallets hold together, which is also what the envelopes add
    /// up to.
    static func total(of wallets: [WalletView]) -> Int64 {
        wallets.reduce(0) { $0 + $1.balance }
    }

    /// Each category's aliases, in alphabetical order whatever the case: what
    /// its ALIAS cell shows, its row opens with and the search reads.
    static func aliases(_ aliases: [AliasView]) -> [Uuid: [String]] {
        Dictionary(grouping: aliases, by: \.categoryId).mapValues { entries in
            entries.map(\.alias).sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
        }
    }
}
