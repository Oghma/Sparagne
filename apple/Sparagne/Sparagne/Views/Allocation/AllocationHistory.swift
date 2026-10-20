import SwiftUI
import SparagneCore

/// Column geometry of "Storico". Dettaglio takes what is left.
private enum HistoryColumn {
    static let period: CGFloat = 110
    static let outcome: CGFloat = 110
    static let detailMinimum: CGFloat = 120
    static let distributed: CGFloat = 120
    static let action: CGFloat = 92
}

/// "Storico": decided periods, newest first; only the latest, which the core can reopen, has Annulla.
struct AllocationHistory: View {
    let store: AppStore

    /// The period whose Annulla was pressed, while the alert asks.
    @State private var confirming: AllocationRunView?

    var body: some View {
        Panel(padding: 0) {
            VStack(alignment: .leading, spacing: 0) {
                CardHeading(
                    title: String(localized: "History"),
                    hint: String(localized: "undoing the latest allocation deletes its transfers and puts it back to share out")
                )
                .padding(.horizontal, 12)
                .padding(.top, 10)
                if store.allocationRuns.isEmpty {
                    Text(String(localized: "No period decided yet"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                        .padding(.horizontal, 12)
                        .padding(.bottom, 10)
                } else {
                    header
                    ForEach(Array(store.allocationRuns.enumerated()), id: \.element.periodDate) { index, run in
                        row(run, latest: index == 0, last: index == store.allocationRuns.count - 1)
                    }
                }
            }
        }
        .alert(
            confirming.map(Self.question) ?? "",
            isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } }),
            presenting: confirming
        ) { run in
            Button(Self.confirmation(run), role: .destructive) {
                Task { await store.reopenAllocation(periodDate: run.periodDate) }
            }
            Button(String(localized: "Keep"), role: .cancel) {}
        } message: { run in
            Text(run.outcome == .executed
                ? String(localized: "Its transfers are deleted and the period is back to share out.")
                : String(localized: "The period is back to share out."))
        }
        // The toast after Distribuisci offers the same period: it steps aside
        // while the alert asks, so only one of the two can be answered.
        .onChange(of: confirming != nil) { _, asking in store.allocationConfirmingReopen = asking }
        .onDisappear { store.allocationConfirmingReopen = false }
    }

    /// The alert's question: a shared-out period is undone, a skipped one
    /// reopened.
    private static func question(_ run: AllocationRunView) -> String {
        let day = RecurringDayText.weekday(run.periodDate)
        return run.outcome == .executed
            ? String(localized: "Undo the allocation of \(day)?")
            : String(localized: "Reopen the period of \(day)?")
    }

    private static func confirmation(_ run: AllocationRunView) -> String {
        run.outcome == .executed
            ? String(localized: "Undo the Allocation")
            : String(localized: "Reopen the Period")
    }

    private var header: some View {
        SetupHeaderRow(horizontalPadding: 4, accessibilityHeader: true) {
            Text(String(localized: "Period")).tableCell(width: HistoryColumn.period)
            Text(String(localized: "Outcome")).tableCell(width: HistoryColumn.outcome)
            Text(String(localized: "Detail")).tableCell(minWidth: HistoryColumn.detailMinimum)
            Text(String(localized: "Distributed")).tableCell(width: HistoryColumn.distributed, alignment: .trailing)
            Text(verbatim: "").tableCell(width: HistoryColumn.action)
        }
    }

    private func row(_ run: AllocationRunView, latest: Bool, last: Bool) -> some View {
        let skipped = run.outcome == .skipped
        return HStack(spacing: 0) {
            Text(RecurringDayText.weekday(run.periodDate)).foregroundStyle(Ink.text)
                .tableCell(width: HistoryColumn.period)
            Text(AllocationText.outcome(run.outcome)).foregroundStyle(skipped ? Ink.text3 : Ink.text)
                .tableCell(width: HistoryColumn.outcome)
            Text(AllocationText.detail(run)).foregroundStyle(skipped ? Ink.text3 : Ink.text2)
                .tableCell(minWidth: HistoryColumn.detailMinimum)
            Text(skipped ? TransactionRow.placeholder : LedgerMoney.bare(AllocationText.distributed(run)))
                .foregroundStyle(skipped ? Ink.text3 : Ink.text)
                .tableCell(width: HistoryColumn.distributed, alignment: .trailing)
            Group {
                if latest, store.canWrite {
                    Button(String(localized: "allocation.undo", defaultValue: "Undo")) { confirming = run }
                        .buttonStyle(.chrome(.ghost, small: true))
                        .disabled(store.allocationReopening)
                } else {
                    Text(verbatim: "")
                }
            }
            .tableCell(width: HistoryColumn.action, alignment: .trailing)
        }
        .font(Face.row)
        .padding(.horizontal, 4)
        .frame(height: 26)
        .overlay(alignment: .bottom) {
            if !last { Rectangle().fill(Ink.rowLine).frame(height: 1) }
        }
        .padding(.bottom, last ? 4 : 0)
        .accessibilityElement(children: .contain)
    }
}
