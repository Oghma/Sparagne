import SwiftUI

/// The Riparto tab's status line: how many lines the plan has, what its
/// fixed lines and its percentages ask for, when the last period was shared
/// out, and when the last change was saved. The history is loaded by the tab
/// (`AllocationTab`), so the last allocation is of what it shows.
struct AllocationStatusLine: View {
    let store: AppStore

    var body: some View {
        StatusLine(items: items)
    }

    private var items: [StatusItem] {
        let figures = AllocationFigures(lines: store.allocationPlan?.lines ?? [])
        var items = [
            StatusItem(label: String(localized: "Lines"), value: figures.lines.formatted(.number)),
            StatusItem(
                label: String(localized: "allocation.status.fixed", defaultValue: "Fixed"),
                value: LedgerMoney.bare(figures.fixed)
            ),
            StatusItem(label: String(localized: "Percentages"), value: AllocationPercent.label(figures.percent)),
        ]
        if let last = store.allocationRuns.first(where: { $0.outcome == .executed }) {
            let today = CoreDate.day(Date())
            items.append(StatusItem(
                label: String(localized: "Last allocation"),
                value: RecurringDayText.short(last.periodDate, today: today)
            ))
        }
        if let savedAt = store.savedAt {
            items.append(StatusItem(label: String(localized: "saved at \(LedgerDate.clock(savedAt))")))
        }
        return items
    }
}
