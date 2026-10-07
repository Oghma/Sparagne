import SwiftUI

/// The Riepilogo's status line (`docs/v2/UI.md` §2.2): the year so far, in
/// the RIEPILOGO's own figures, and when the last change was saved.
struct SummaryStatusLine: View {
    let store: AppStore

    var body: some View {
        StatusLine(items: items)
    }

    private var items: [StatusItem] {
        var items: [StatusItem] = []
        if let year = store.year {
            // The months up to the one on screen: the later ones are drawn
            // blank in the table, and are not counted here either.
            let months = year.months.filter { !$0.isFuture }
            let savings = months.reduce(0) { $0 + $1.savings }
            items.append(StatusItem(label: String(localized: "Savings"), value: LedgerMoney.bare(savings)))
            if let last = months.last {
                items.append(StatusItem(label: String(localized: "Total"), value: LedgerMoney.bare(last.total)))
            }
        }
        if let savedAt = store.savedAt {
            items.append(StatusItem(label: String(localized: "saved at \(LedgerDate.clock(savedAt))")))
        }
        return items
    }
}
