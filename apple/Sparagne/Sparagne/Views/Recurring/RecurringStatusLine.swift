import SwiftUI

/// The Ricorrenze tab's status line (`docs/v2/UI.md` §2.5): how many
/// templates run and how many are archived, what the running ones cost and
/// bring in a month (`RecurringMonthly`, hence "≈": a daily or weekly
/// template is spread over the average month), and when the last change was
/// saved. The templates are loaded by the tab (`RecurringTab`), so the
/// figures are of what it shows.
struct RecurringStatusLine: View {
    let store: AppStore

    var body: some View {
        StatusLine(items: items)
    }

    private var items: [StatusItem] {
        let templates = store.recurringTemplates
        let running = templates.filter { $0.enabled && !$0.archived }.count
        let archived = templates.filter(\.archived).count
        let monthly = RecurringMonthly.totals(templates)
        var items = [
            StatusItem(label: String(localized: "Active"), value: running.formatted(.number)),
            // Not "Archived", whose translation agrees with wallets and
            // envelopes; these are recurring entries.
            StatusItem(label: String(localized: "In archive"), value: archived.formatted(.number)),
            StatusItem(
                label: String(localized: "Fixed expenses a month"),
                value: "\u{2248} " + LedgerMoney.bare(monthly.expenses)
            ),
            StatusItem(
                label: String(localized: "Fixed income"),
                value: "\u{2248} " + LedgerMoney.bare(monthly.income)
            ),
        ]
        if let savedAt = store.savedAt {
            items.append(StatusItem(label: String(localized: "saved at \(LedgerDate.clock(savedAt))")))
        }
        return items
    }
}
