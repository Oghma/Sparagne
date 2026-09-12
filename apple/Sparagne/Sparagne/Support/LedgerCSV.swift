import Foundation
import SparagneCore

/// The ledger's CSV export (`docs/v2/UI.md` §6, ⌘E): a pure rendering over
/// the rows already on screen, so the window only has to hand it what
/// `AppStore.rows` already filtered (month, direction, person, search, and
/// voided/transfers only when the View menu is showing them).
///
/// RFC 4180: comma-separated, CRLF line endings, a field quoted when it
/// carries a comma, a quote or a newline, with inner quotes doubled.
enum LedgerCSV {
    static let header = "date,kind,envelope,category,description,person,amount,voided"
    /// With the optional WALLET column on, the file carries it in the same
    /// place the grid does: after the description (`docs/v2/UI.md` §3).
    static let headerWithWallet = "date,kind,envelope,category,description,wallet,person,amount,voided"

    static func header(wallet: Bool) -> String { wallet ? headerWithWallet : header }

    /// The full file text, header included. An empty `rows` still yields the
    /// header line, so a filtered month with nothing in it still exports a
    /// file a spreadsheet can open.
    ///
    /// `wallet` is the visibility of the column in the window: the export is
    /// what is on screen, columns included.
    static func render(_ rows: [TransactionRow], wallet: Bool = false) -> String {
        var text = header(wallet: wallet) + "\r\n"
        for row in rows {
            text += line(for: row, wallet: wallet) + "\r\n"
        }
        return text
    }

    /// `<vault>-<yyyy>-<mm>-<direction>.csv`, e.g. `main-2026-08-expenses.csv`.
    /// Uses `LedgerDirection`'s raw value rather than its localized label, so
    /// the file name stays stable across locales.
    static func fileName(vault: String, month: MonthKey, direction: LedgerDirection) -> String {
        let slug = vault.lowercased().replacingOccurrences(of: " ", with: "-")
        return String(format: "%@-%04d-%02d-%@.csv", slug, month.year, month.month, direction.rawValue)
    }

    private static func line(for row: TransactionRow, wallet: Bool) -> String {
        // A wallet transfer's `envelopeDisplay` is only the placeholder
        // (`TransactionRow`), so the export falls back to the wallet arrow
        // for that one kind; every other row already carries what it needs
        // in `envelopeDisplay`.
        let envelope = row.isTransfer && row.kind == .transferWallet ? row.walletDisplay : row.envelopeDisplay
        var fields = [
            CoreDate.day(row.occurredAt),
            kindText(row.kind),
            envelope,
            row.category,
            row.note,
        ]
        if wallet { fields.append(row.walletDisplay) }
        fields.append(contentsOf: [
            row.person,
            amountText(row),
            row.voided ? "true" : "false",
        ])
        return fields.map(quoted).joined(separator: ",")
    }

    private static func kindText(_ kind: TransactionKind) -> String {
        switch kind {
        case .income: "income"
        case .expense: "expense"
        case .refund: "refund"
        case .transferWallet: "transfer_wallet"
        case .transferFlow: "transfer_flow"
        }
    }

    /// Signed major units with a `.` decimal point, from integer minor units
    /// only, never through `Double` (the same pattern as
    /// `LedgerMoney.editable`). Expenses export negative; income, refunds and
    /// transfers positive, the same sign convention the ledger's totals use
    /// (`LedgerStatusBar.visibleTotal`).
    private static func amountText(_ row: TransactionRow) -> String {
        let magnitude = row.absoluteAmount.magnitude
        let text = String(format: "%llu.%02llu", magnitude / 100, magnitude % 100)
        return row.kind == .expense ? "-\(text)" : text
    }

    private static func quoted(_ field: String) -> String {
        guard field.contains(",") || field.contains("\"") || field.contains("\n") || field.contains("\r") else {
            return field
        }
        return "\"" + field.replacingOccurrences(of: "\"", with: "\"\"") + "\""
    }
}
