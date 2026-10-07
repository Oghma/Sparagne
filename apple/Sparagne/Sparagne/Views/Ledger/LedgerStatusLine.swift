import SwiftUI
import SparagneCore

/// The Mastro's status line (`docs/v2/UI.md` §2.1): how many rows are on
/// screen, what they add up to, and when the last change was saved.
struct LedgerStatusLine: View {
    let store: AppStore

    var body: some View {
        StatusLine(items: items)
    }

    private var items: [StatusItem] {
        var items = [
            StatusItem(label: String(localized: "Count"), value: store.rows.count.formatted(.number)),
            StatusItem(label: String(localized: "Sum"), value: LedgerMoney.bare(Self.visibleTotal(store.rows))),
        ]
        if let savedAt = store.savedAt {
            items.append(StatusItem(label: String(localized: "saved at \(LedgerDate.clock(savedAt))")))
        }
        return items
    }

    /// The sum of the rows on screen, which is what the user is looking at:
    /// not the month's total, which the panel already shows. A refund sits in
    /// the USCITE list and has to come off it, or the sum reads higher than
    /// what was actually spent; a transfer moves money without spending it.
    static func visibleTotal(_ rows: [TransactionRow]) -> Int64 {
        rows.reduce(0) { total, row in
            switch row.kind {
            case .refund: total - row.absoluteAmount
            case .income, .expense: total + row.absoluteAmount
            case .transferWallet, .transferFlow: total
            }
        }
    }
}
