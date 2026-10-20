import SwiftUI
import SparagneCore

/// The figures of each sheet's status line (`StatusLine`).
enum SheetStatus {
    static func items(for tab: LedgerTab, store: AppStore) -> [StatusItem] {
        switch tab {
        case .summary: summary(store)
        case .ledger: ledger(store.sheetStats)
        case .recurring: recurring(store.recurringTemplates)
        case .allocation: allocation(store)
        case .setup: setup(store)
        }
    }

    /// `Media 47,69 · Conteggio 13 · Somma 619,94`, opened by "Selezione:"
    /// when the figures are the selection's: the rows on screen, as a
    /// spreadsheet's status bar reads the cells under the cursor. The month's
    /// own totals are the side panel's job (`SheetStats`).
    private static func ledger(_ stats: SheetStats) -> [StatusItem] {
        let average = String(localized: "Average")
        return [
            StatusItem(
                label: stats.scope == .selection ? "\(String(localized: "Selection:")) \(average)" : average,
                value: stats.mean.map(LedgerMoney.bare) ?? TransactionRow.placeholder
            ),
            StatusItem(label: String(localized: "Count"), value: stats.count.formatted(.number)),
            StatusItem(label: String(localized: "Sum"), value: LedgerMoney.bare(stats.sum)),
        ]
    }

    /// The year so far, in the RIEPILOGO's own figures: the months up to the
    /// one on screen, as the table draws the later ones blank.
    private static func summary(_ store: AppStore) -> [StatusItem] {
        guard let year = store.year else { return [] }
        let label = String(year.year)
        return [
            StatusItem(label: String(localized: "Savings \(label)"), value: LedgerMoney.bare(year.yearSavings)),
            StatusItem(label: String(localized: "Total"), value: LedgerMoney.bare(year.closingTotal)),
        ]
    }

    /// How many templates run and how many are archived, and what the running
    /// ones cost and bring in a month (`RecurringMonthly`, hence "≈": a daily
    /// or weekly template is spread over the average month). The tab loads
    /// the templates (`RecurringTab`), so the figures are of what it shows.
    private static func recurring(_ templates: [RecurringView]) -> [StatusItem] {
        let monthly = RecurringMonthly.totals(templates)
        return [
            StatusItem(
                label: String(localized: "Active"),
                value: templates.filter { $0.enabled && !$0.archived }.count.formatted(.number)
            ),
            // Not "Archived", whose translation agrees with wallets and
            // envelopes; these are recurring entries.
            StatusItem(
                label: String(localized: "In archive"),
                value: templates.filter(\.archived).count.formatted(.number)
            ),
            StatusItem(
                label: String(localized: "Fixed expenses a month"),
                value: "\u{2248} " + LedgerMoney.bare(monthly.expenses)
            ),
            StatusItem(label: String(localized: "Fixed income"), value: "\u{2248} " + LedgerMoney.bare(monthly.income)),
        ]
    }

    /// How many lines the plan has, what its fixed lines and its percentages
    /// ask for, and when the last period was shared out. The tab loads the
    /// history (`AllocationTab`), so the last allocation is of what it shows.
    private static func allocation(_ store: AppStore) -> [StatusItem] {
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
            items.append(StatusItem(
                label: String(localized: "Last allocation"),
                value: RecurringDayText.short(last.periodDate, today: CoreDate.day(Date()))
            ))
        }
        return items
    }

    /// How many wallets, envelopes and categories the vault has in use,
    /// archived ones left out. Unallocated is the core's own envelope, not
    /// one the user made.
    private static func setup(_ store: AppStore) -> [StatusItem] {
        [
            StatusItem(label: String(localized: "Wallets"), value: store.wallets.count.formatted(.number)),
            StatusItem(
                label: String(localized: "Envelopes"),
                value: store.flows.filter { !$0.isUnallocated }.count.formatted(.number)
            ),
            StatusItem(label: String(localized: "Categories"), value: store.categories.count.formatted(.number)),
        ]
    }
}
