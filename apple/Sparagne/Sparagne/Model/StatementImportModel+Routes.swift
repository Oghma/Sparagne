import Foundation
import SparagneCore

/// Which way a row of the review moves money, where a move between two
/// accounts of the file goes, and what the kind menu ticks.
extension StatementImportModel {
    /// Whether a row takes money out: by what it is taken for, as an export
    /// writing every amount positive says through its type, else by its
    /// sign.
    nonisolated static func movesOut(_ row: StatementRow) -> Bool {
        switch row.action {
        case .expense, .transferOut: true
        case .income, .refund, .transferIn: false
        case .bySign, .skip: row.outflow
        }
    }

    /// Expenses and transfers out minus, everything else plus, as the
    /// ledger's totals sign them. A transfer goes the way what the row is
    /// taken for says (`movesOut`): a transfer out, the money-out half of a
    /// move between two accounts, or a `BySign` row its match made a
    /// transfer, the way the file signs it.
    nonisolated static func signedAmount(_ row: StatementRow) -> Int64 {
        if row.kind == .expense { return -row.amount }
        if row.kind == .transferWallet { return movesOut(row) ? -row.amount : row.amount }
        return row.amount
    }

    /// One half of a move between two accounts of the file, paired with the
    /// other: the money-out half is the transfer, the other its pair.
    nonisolated static func isPairedHalf(_ row: StatementRow) -> Bool {
        row.pairedLine != nil && row.transferLeg
    }

    /// Every paired move among `rows` by line, both halves: the two wallets
    /// from the money-out half's to the other's (`from Revolut to BBVA`), or
    /// its two accounts when both go to one wallet, where the move changes
    /// nothing. One pass over the preview, for the review to look each row
    /// up in.
    static func pairedRoutes(_ rows: [StatementRow], names: NameBook) -> [UInt32: String] {
        let byLine = Dictionary(rows.map { ($0.line, $0) }, uniquingKeysWith: { first, _ in first })
        var routes: [UInt32: String] = [:]
        for row in rows where Self.isPairedHalf(row) {
            guard let other = row.pairedLine.flatMap({ byLine[$0] }) else { continue }
            let out = Self.movesOut(row) ? row : other
            let into = Self.movesOut(row) ? other : row
            func end(_ half: StatementRow) -> String {
                let wallet = out.walletId == into.walletId ? nil : names.wallet(half.walletId)
                return wallet ?? half.account ?? TransactionRow.placeholder
            }
            routes[row.line] = String(localized: "from \(end(out)) to \(end(into))")
        }
        return routes
    }

    /// The kind menu's ticked item: a `BySign` as the kind it resolved to,
    /// an income or an expense. One its match made a transfer ticks nothing:
    /// the cell says transfer, and either entry is a real change.
    func kindChoice(for row: StatementRow) -> StatementActionChoice {
        let action = self.action(for: row)
        guard action == .bySign else { return StatementActionChoice(action) }
        return row.kind == .income || row.kind == .expense ? (row.outflow ? .expense : .income) : .bySign
    }
}

extension StatementImportModel.Counts {
    /// Whether the import writes anything: a row, new or replacing an
    /// entry, a move, or the record that a transaction already there stands
    /// for a row, which keeps the next export from taking it again.
    var writesAnything: Bool {
        written + moves + possibleDuplicates + matchedTransfers > 0
    }
}
