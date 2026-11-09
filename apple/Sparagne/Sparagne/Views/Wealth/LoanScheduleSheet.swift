import SwiftUI
import SparagneCore

/// Column geometry of the schedule's table: Residuo takes what is left.
private enum ScheduleColumn {
    static let number: CGFloat = 44
    static let due: CGFloat = 104
    static let figure: CGFloat = 96
    static let fee: CGFloat = 72
    static let residualMinimum: CGFloat = 110
    static let padding: CGFloat = 6
}

/// «Piano d'ammortamento»: every installment of a loan with its split
/// between interest and capital and the debt it leaves, each event under
/// the installment it follows, today's place marked, and the totals.
struct LoanScheduleSheet: View {
    let store: AppStore
    let loan: LoanView

    @Environment(\.dismiss) private var dismiss
    @State private var schedule: LoanSchedule?
    @State private var failed = false

    private let today = CoreDate.day(Date())

    var body: some View {
        FormSheet(
            String(localized: "Amortization schedule"),
            subtitle: "\(loan.name) \u{00B7} \(WealthText.summary(loan))",
            width: 680
        ) {
            if let schedule {
                table(schedule)
                totals(schedule)
                ForEach(schedule.warnings, id: \.self) { warning in
                    FormNote(WealthText.warning(warning), tone: .warning, indented: false)
                }
            } else if failed {
                FormNote(String(localized: "The schedule could not be worked out."), tone: .negative, indented: false)
            } else {
                ProgressView().controlSize(.small)
            }
        } footer: {
            FormPrimaryButton(title: String(localized: "Close")) { dismiss() }
        }
        .task(id: loan) {
            schedule = await store.loanSchedule(loan.id)
            failed = schedule == nil
        }
    }

    // MARK: - The table

    private func table(_ schedule: LoanSchedule) -> some View {
        let events = Dictionary(loan.events.map { ($0.id, $0.event) }, uniquingKeysWith: { first, _ in first })
        let previous = LoanFacts.previousRates(loan)
        let firstAhead = schedule.rows.firstIndex { $0.dueDate > today }
        return VStack(spacing: 0) {
            header
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(schedule.eventIdsBeforeFirst, id: \.self) { id in
                            if let event = events[id] { eventLine(event, previousRate: previous[id]) }
                        }
                        ForEach(Array(schedule.rows.enumerated()), id: \.element.number) { index, row in
                            if index == firstAhead, index > 0 { todayLine }
                            line(row, past: row.dueDate <= today)
                                .id(row.number)
                            ForEach(row.eventIds, id: \.self) { id in
                                if let event = events[id] { eventLine(event, previousRate: previous[id]) }
                            }
                        }
                    }
                }
                .frame(height: 380)
                .scrollBounceBehavior(.basedOnSize)
                // Opened on today's place in a long schedule, a few rows of
                // the past above it.
                .onAppear {
                    if let firstAhead, firstAhead > 3 {
                        proxy.scrollTo(schedule.rows[firstAhead - 3].number, anchor: .top)
                    }
                }
            }
        }
        .font(Face.row)
        .monospacedDigit()
    }

    private var header: some View {
        SetupHeaderRow(accessibilityHeader: true) {
            Text(String(localized: "No.")).tableCell(width: ScheduleColumn.number, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(String(localized: "Due")).tableCell(width: ScheduleColumn.due, padding: ScheduleColumn.padding)
            Text(String(localized: "Installment"))
                .tableCell(width: ScheduleColumn.figure, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(String(localized: "Interest"))
                .tableCell(width: ScheduleColumn.figure, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(String(localized: "Capital"))
                .tableCell(width: ScheduleColumn.figure, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(String(localized: "Fees")).tableCell(width: ScheduleColumn.fee, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(String(localized: "Remaining debt"))
                .tableCell(minWidth: ScheduleColumn.residualMinimum, alignment: .trailing, padding: ScheduleColumn.padding)
        }
    }

    /// One installment; the ones already due in `text2`.
    private func line(_ row: LoanScheduleRow, past: Bool) -> some View {
        let ink = past ? Ink.text2 : Ink.text
        return SetupRow {
            Text(verbatim: "\(row.number)").foregroundStyle(Ink.text3)
                .tableCell(width: ScheduleColumn.number, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(RecurringDayText.full(row.dueDate)).foregroundStyle(ink)
                .tableCell(width: ScheduleColumn.due, padding: ScheduleColumn.padding)
            Text(LedgerMoney.bare(row.payment)).foregroundStyle(ink)
                .tableCell(width: ScheduleColumn.figure, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(LedgerMoney.bare(row.interest)).foregroundStyle(ink)
                .tableCell(width: ScheduleColumn.figure, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(LedgerMoney.bare(row.capital)).foregroundStyle(ink)
                .tableCell(width: ScheduleColumn.figure, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(LedgerMoney.bare(row.fee)).foregroundStyle(row.fee == 0 ? Ink.text3 : ink)
                .tableCell(width: ScheduleColumn.fee, alignment: .trailing, padding: ScheduleColumn.padding)
            Text(LedgerMoney.bare(row.residual)).foregroundStyle(ink)
                .tableCell(minWidth: ScheduleColumn.residualMinimum, alignment: .trailing, padding: ScheduleColumn.padding)
        }
        .accessibilityElement(children: .combine)
    }

    /// An event, under the installment it follows: its kind as an accent
    /// tag, what it changed after it.
    private func eventLine(_ event: LoanEvent, previousRate: UInt32?) -> some View {
        HStack(spacing: 8) {
            RowTag(text: WealthText.kind(event).lowercased(), tint: Ink.accent, opacity: 0.14)
            Text(verbatim: "\(RecurringDayText.full(event.effectiveDate)) \u{00B7} \(WealthText.detail(event, previousRate: previousRate))")
                .foregroundStyle(Ink.text2)
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, ScheduleColumn.padding + ScheduleColumn.number)
        .frame(height: Metrics.rowHeight)
        .background(Ink.accent.opacity(0.05))
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.rowLine).frame(height: 1) }
        .accessibilityElement(children: .combine)
    }

    /// Where today falls: between the last installment due and the next.
    private var todayLine: some View {
        HStack(spacing: 6) {
            Rectangle().fill(Ink.accent.opacity(0.6)).frame(height: 1)
            Text(String(localized: "today"))
                .font(Face.small)
                .foregroundStyle(Ink.accent)
                .fixedSize()
            Rectangle().fill(Ink.accent.opacity(0.6)).frame(width: 24, height: 1)
        }
        .frame(height: 14)
        .accessibilityElement(children: .combine)
    }

    // MARK: - Totals

    private func totals(_ schedule: LoanSchedule) -> some View {
        HStack(spacing: 18) {
            AmountFigure(label: String(localized: "Total interest"), amount: schedule.totalInterest, spacing: 5)
            if schedule.totalFees + loan.terms.upfrontFees > 0 {
                AmountFigure(label: String(localized: "Fees"), amount: schedule.totalFees + loan.terms.upfrontFees, spacing: 5)
            }
            if schedule.totalPrepaid > 0 {
                AmountFigure(label: String(localized: "Early repayments"), amount: schedule.totalPrepaid, spacing: 5)
            }
            if schedule.totalPenalties > 0 {
                AmountFigure(label: String(localized: "Penalties"), amount: schedule.totalPenalties, spacing: 5)
            }
            AmountFigure(label: String(localized: "Total cost"), amount: schedule.totalCost, spacing: 5)
            Spacer(minLength: 0)
        }
        .font(Face.row)
        .monospacedDigit()
        .padding(.top, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .top) { Rectangle().fill(Ink.line2).frame(height: 1) }
    }
}

/// «Crea ricorrenza» for a saved loan: the monthly expense that records its
/// installments, made from its schedule and linked to it in one go.
struct LoanRecurringSheet: View {
    let store: AppStore
    let loan: LoanView

    @Environment(\.dismiss) private var dismiss
    @State private var target: LoanLedgerTarget
    @State private var working = false
    @State private var completion = CategoryCompletionModel()

    init(store: AppStore, loan: LoanView) {
        self.store = store
        self.loan = loan
        _target = State(initialValue: LoanTargetRows.defaults(store))
    }

    var body: some View {
        let overview = store.loanOverview.first { $0.loanId == loan.id }
        FormSheet(
            String(localized: "Create recurring entry"),
            subtitle: overview?.nextPayment.map {
                String(localized: "A monthly expense of \(WealthText.euro($0)) that follows the schedule of \(loan.name).")
            } ?? String(localized: "A monthly expense that follows the schedule of \(loan.name)."),
            width: 480
        ) {
            FormGroup {
                LoanTargetRows(store: store, target: $target, completion: completion)
            }
        } footer: {
            FormCancelButton(shortcut: !completion.isOpen) { dismiss() }
                .disabled(working)
            FormPrimaryButton(title: String(localized: "Create"), shortcut: !completion.isOpen) {
                let target = target
                $working.run {
                    if await store.createLoanRecurring(for: loan, target: target) { dismiss() }
                }
            }
            .disabled(working)
        }
        .hangingCategoryList(completion)
    }
}
