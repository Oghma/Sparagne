import SwiftUI
import SparagneCore

/// Column geometry of "Storico". Dettaglio takes what is left.
private enum HistoryColumn {
    static let period: CGFloat = 110
    static let outcome: CGFloat = 110
    static let detailMinimum: CGFloat = 120
    static let distributed: CGFloat = 120
    static let action: CGFloat = 92
    static let padding: CGFloat = 8
}

/// "Storico": the plan's decided periods, the most recent first
/// (`AppStore.allocationRuns`). Annulla is on the latest one only, the one
/// the core lets go back to waiting: its transfers are deleted and its
/// incomes return to the total. It asks first.
struct AllocationHistory: View {
    let store: AppStore

    /// The period whose Annulla was pressed, while the alert asks.
    @State private var confirming: AllocationRunView?
    @State private var working = false

    var body: some View {
        Panel(padding: 0) {
            VStack(alignment: .leading, spacing: 0) {
                RecurringCardHeader(title: String(localized: "History")) {
                    Text(String(localized: "undoing the latest allocation deletes its transfers and puts it back to share out"))
                        .font(Face.ui(11.5, .medium))
                        .foregroundStyle(Ink.text3)
                        .lineLimit(2)
                        .multilineTextAlignment(.trailing)
                }
                .padding(.horizontal, 12)
                .padding(.top, 10)
                if store.allocationRuns.isEmpty {
                    Text(String(localized: "No period decided yet"))
                        .font(Face.ui(12))
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
            confirming.map { String(localized: "Undo the allocation of \(RecurringDayText.weekday($0.periodDate))?") } ?? "",
            isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } }),
            presenting: confirming
        ) { run in
            Button(String(localized: "Undo the Allocation"), role: .destructive) {
                working = true
                Task {
                    await store.reopenAllocation(periodDate: run.periodDate)
                    working = false
                }
            }
            Button(String(localized: "Keep"), role: .cancel) {}
        } message: { run in
            Text(run.outcome == .executed
                ? String(localized: "Its transfers are deleted and the period is back to share out.")
                : String(localized: "The period is back to share out."))
        }
    }

    private var header: some View {
        HStack(spacing: 0) {
            cell(width: HistoryColumn.period) { Text(String(localized: "Period")) }
            cell(width: HistoryColumn.outcome) { Text(String(localized: "Outcome")) }
            cell(minWidth: HistoryColumn.detailMinimum) { Text(String(localized: "Detail")) }
            cell(width: HistoryColumn.distributed, alignment: .trailing) { Text(String(localized: "Distributed")) }
            cell(width: HistoryColumn.action) { Text(verbatim: "") }
        }
        .font(Face.ui(11, .medium))
        .foregroundStyle(Ink.text3)
        .padding(.horizontal, 4)
        .frame(height: Metrics.headerHeight)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }

    private func row(_ run: AllocationRunView, latest: Bool, last: Bool) -> some View {
        let skipped = run.outcome == .skipped
        return HStack(spacing: 0) {
            cell(width: HistoryColumn.period) {
                Text(RecurringDayText.weekday(run.periodDate)).foregroundStyle(Ink.text)
            }
            cell(width: HistoryColumn.outcome) {
                Text(AllocationText.outcome(run.outcome)).foregroundStyle(skipped ? Ink.text3 : Ink.text)
            }
            cell(minWidth: HistoryColumn.detailMinimum) {
                Text(AllocationText.detail(run)).foregroundStyle(skipped ? Ink.text3 : Ink.text2)
            }
            cell(width: HistoryColumn.distributed, alignment: .trailing) {
                Text(skipped ? TransactionRow.placeholder : LedgerMoney.bare(AllocationText.distributed(run)))
                    .foregroundStyle(skipped ? Ink.text3 : Ink.text)
            }
            cell(width: HistoryColumn.action, alignment: .trailing) {
                if latest, store.canWrite {
                    Button(String(localized: "allocation.undo", defaultValue: "Undo")) { confirming = run }
                        .buttonStyle(.chrome(.ghost, small: true))
                        .disabled(working)
                } else {
                    Text(verbatim: "")
                }
            }
        }
        .font(Face.ui(12))
        .padding(.horizontal, 4)
        .frame(height: 26)
        .overlay(alignment: .bottom) {
            if !last { Rectangle().fill(Ink.rowLine).frame(height: 1) }
        }
        .padding(.bottom, last ? 4 : 0)
        .accessibilityElement(children: .contain)
    }

    private func cell<Content: View>(
        width: CGFloat? = nil,
        minWidth: CGFloat? = nil,
        alignment: Alignment = .leading,
        @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, HistoryColumn.padding)
            .frame(
                minWidth: width ?? minWidth,
                idealWidth: width,
                maxWidth: width ?? .infinity,
                alignment: alignment
            )
    }
}
