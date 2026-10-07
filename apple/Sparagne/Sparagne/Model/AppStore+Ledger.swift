import Foundation
import SparagneCore

/// What the Mastro reads off the store beyond its rows: the lines of the grid
/// with the due periods among them, the status line's figures, and the names
/// a recurring template's ids stand for (`docs/v2/UI.md` §2.1). Computed only:
/// nothing here is stored or written.
extension AppStore {
    /// What the grid's lines are made of: the rows on screen and the periods
    /// due this month, to be slotted in at their date (`LedgerLines`). While a
    /// next page is still to come, the periods past the last row loaded wait
    /// for it. The grid keeps the lines until this changes
    /// (`LedgerLines.Cache`).
    var ledgerLinesInput: LedgerLines.Input {
        LedgerLines.Input(
            rows: rows,
            due: duePeriods,
            month: month,
            direction: direction,
            search: searchText,
            hasMoreRows: nextCursor != nil
        )
    }

    /// The status line's average, count and sum (`SheetStats`).
    var sheetStats: SheetStats {
        SheetStats.make(rows: rows, selection: selection)
    }

    /// A template's envelope as the BUSTA column writes it. `nil` is
    /// Unallocated (`RecurringView.flowId`); an archived envelope still has
    /// its name.
    func envelopeName(_ flowId: Uuid?) -> String {
        guard let flowId else { return NameBook.unallocatedLabel }
        return NameBook(snapshot: snapshot).flow(flowId) ?? TransactionRow.placeholder
    }

    /// A template's wallet as the WALLET column writes it. `nil` means the
    /// only active wallet at execution time, so that one is named when there
    /// is exactly one; otherwise the column cannot say yet.
    func walletName(_ walletId: Uuid?) -> String {
        if let walletId {
            return NameBook(snapshot: snapshot).wallet(walletId) ?? TransactionRow.placeholder
        }
        return wallets.count == 1 ? wallets[0].name : TransactionRow.placeholder
    }
}
