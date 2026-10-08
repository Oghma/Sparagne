import SwiftUI
import SparagneCore

/// The Mastro's status line, as a spreadsheet's status
/// bar reads the cells under the cursor: the average, the count and the sum of
/// the rows on screen, or of the selection when two or more rows are picked,
/// then when the last change was saved. The month's own totals are the side
/// panel's job, so this one follows the eye instead (`SheetStats`).
struct LedgerStatusLine: View {
    let store: AppStore

    var body: some View {
        StatusLine(items: Self.items(store.sheetStats, savedAt: store.savedAt))
    }

    /// `Media 47,69 · Conteggio 13 · Somma 619,94 · salvato alle 12:04`,
    /// opened by "Selezione:" when the figures are the selection's.
    static func items(_ stats: SheetStats, savedAt: Date?) -> [StatusItem] {
        let average = String(localized: "Average")
        var items = [
            StatusItem(
                label: stats.scope == .selection ? "\(String(localized: "Selection:")) \(average)" : average,
                value: stats.mean.map(LedgerMoney.bare) ?? TransactionRow.placeholder
            ),
            StatusItem(label: String(localized: "Count"), value: stats.count.formatted(.number)),
            StatusItem(label: String(localized: "Sum"), value: LedgerMoney.bare(stats.sum)),
        ]
        if let savedAt {
            items.append(StatusItem(label: String(localized: "saved at \(LedgerDate.clock(savedAt))")))
        }
        return items
    }
}
