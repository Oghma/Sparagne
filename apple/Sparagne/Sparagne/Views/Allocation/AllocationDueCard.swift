import SwiftUI
import SparagneCore

/// "Da distribuire": the period waiting for a decision, with Salta and Distribuisci.
struct AllocationDueCard: View {
    let store: AppStore
    let today: NaiveDate
    @Binding var totalText: String
    /// The total typed, `nil` while it is not an amount of zero or more.
    let total: Int64?
    let preview: AllocationPreview?
    /// The plan's lines as the table shows them, which Distribuisci sends
    /// worked out (`AppStore.executeAllocation`).
    let lines: [AllocationLine]
    /// An edit of the plan is on its way: the period waits for it, so what
    /// is decided is the plan the table shows once it is saved.
    let saving: Bool

    /// A command in flight. The buttons wait for it: a double click would
    /// decide the same period twice, and the core would refuse the second.
    @State private var working = false

    var body: some View {
        if let plan = store.allocationPlan, let pending = store.pendingAllocation, pending.planId == plan.id {
            dueCard(plan: plan, pending: pending)
        } else {
            quietCard
        }
    }

    // MARK: - A period due

    private func dueCard(plan: AllocationPlanView, pending: PendingAllocation) -> some View {
        let names = NameBook(snapshot: store.snapshot)
        return VStack(alignment: .leading, spacing: 0) {
            header(pending)
            HStack(alignment: .top, spacing: 16) {
                totalField
                incomes(plan: plan, names: names)
            }
            summary(names: names)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Ink.card, in: RoundedRectangle(cornerRadius: Metrics.cardRadius))
        .overlay(
            RoundedRectangle(cornerRadius: Metrics.cardRadius).strokeBorder(Ink.accent.opacity(0.4), lineWidth: 1)
        )
        .accessibilityElement(children: .contain)
        .accessibilityLabel(String(localized: "To share out"))
    }

    /// `Da distribuire · dom 27 set  9 giorni fa`, and the two buttons.
    private func header(_ pending: PendingAllocation) -> some View {
        HStack(spacing: 8) {
            HStack(spacing: 6) {
                Text(String(localized: "To share out \u{00B7} \(RecurringDayText.weekday(pending.periodDate))"))
                    .font(Face.ui(11.5, .semibold))
                    .foregroundStyle(Ink.text2)
                    .accessibilityAddTraits(.isHeader)
                Text(CountText.daysAgo(NaiveDay.days(from: pending.periodDate, to: today) ?? 0))
                if pending.missed > 0 {
                    Text(verbatim: "\u{00B7}").accessibilityHidden(true)
                    Text(AllocationText.missed(Int(pending.missed)))
                }
            }
            .font(Face.ui(11.5, .medium))
            .foregroundStyle(Ink.text3)
            .lineLimit(1)
            Spacer(minLength: 8)
            if store.canWrite {
                HStack(spacing: 6) {
                    Button(String(localized: "Skip")) {
                        $working.run { await store.skipAllocation() }
                    }
                    .buttonStyle(.chrome(.ghost, small: true))
                    Button(String(localized: "Share out \(LedgerMoney.bare(preview?.distributed ?? 0))")) {
                        guard let total else { return }
                        let shown = lines
                        $working.run { await store.executeAllocation(total: total, lines: shown) }
                    }
                    .buttonStyle(.chrome(.primary, small: true))
                    .disabled(total == nil || preview == nil)
                }
                .disabled(working || saving)
            }
        }
        .frame(minHeight: 22)
        .padding(.bottom, 8)
    }

    /// TOTALE DA RIPARTIRE: the base, which the user may correct.
    private var totalField: some View {
        VStack(alignment: .leading, spacing: 4) {
            FormGroupLabel(String(localized: "Total to share out"))
            HStack(spacing: 8) {
                TextField(String(localized: "Total to share out"), text: $totalText, prompt: Text(verbatim: "0,00"))
                    .textFieldStyle(.plain)
                    .labelsHidden()
                    .font(Face.ui(20, .semibold))
                    .foregroundStyle(total == nil ? Ink.negative : Ink.text)
                    .accessibilityLabel(String(localized: "Total to share out"))
                Text(verbatim: "€")
                    .font(Face.ui(13))
                    .foregroundStyle(Ink.text3)
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, 12)
            .frame(height: 40)
            .background(Ink.sheet, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Ink.line2, lineWidth: 1))
            .disabled(!store.canWrite || working)
            Text(String(localized: "can be corrected by hand"))
                .font(Face.ui(11.5))
                .foregroundStyle(Ink.text3)
        }
        .frame(width: 220, alignment: .leading)
    }

    /// The incomes that make the base up, oldest first.
    private func incomes(plan: AllocationPlanView, names: NameBook) -> some View {
        let incomes = store.allocationBase.incomes
        let decided = store.allocationRuns.first?.periodDate
        return VStack(alignment: .leading, spacing: 0) {
            FormGroupLabel(
                AllocationText.incomes(incomes.count, after: decided, since: plan.schedule.startDate)
            )
            .padding(.bottom, 2)
            ForEach(Array(incomes.enumerated()), id: \.element.id) { index, income in
                if index > 0 { DashedRule() }
                incomeRow(TransactionRow(view: income, names: names))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
    }

    private func incomeRow(_ row: TransactionRow) -> some View {
        HStack(spacing: 8) {
            Text(RecurringDayText.weekday(CoreDate.day(row.occurredAt)))
                .foregroundStyle(Ink.text2)
                .frame(width: 76, alignment: .leading)
            Text(row.note.isEmpty ? row.category : row.note)
                .foregroundStyle(Ink.text)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text("+" + LedgerMoney.bare(row.absoluteAmount))
                .foregroundStyle(Ink.positive)
                .frame(width: 96, alignment: .trailing)
        }
        .font(Face.row)
        .lineLimit(1)
        .frame(height: 22)
        .accessibilityElement(children: .combine)
    }

    /// Distribuiti · Resta in Non allocato · Non allocato dopo, and a chip
    /// for each line that gets less than it asks for.
    @ViewBuilder
    private func summary(names: NameBook) -> some View {
        if let preview {
            FlowLayout(horizontal: 18, vertical: 6) {
                figure(String(localized: "Distributed"), preview.distributed)
                figure(String(localized: "Left in Unallocated"), preview.remainder)
                figure(String(localized: "Unallocated after"), preview.unallocatedAfter)
                ForEach(Array(preview.lines.enumerated()), id: \.offset) { _, line in
                    if line.amount < line.wanted, line.status == .short || line.status == .capLimited {
                        RowTag(
                            text: AllocationText.less(line, name: names.flow(line.flowId) ?? TransactionRow.placeholder),
                            tint: Ink.accent
                        )
                    }
                }
            }
            .padding(.top, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(alignment: .top) { Hairline() }
            .padding(.top, 10)
        }
    }

    private func figure(_ label: String, _ amount: Int64) -> some View {
        AmountFigure(label: label, amount: amount, negative: amount < 0).font(Face.row)
    }

    // MARK: - Nothing due

    /// No period due: when the next one falls, or that the plan is paused, or
    /// that there is no plan yet.
    private var quietCard: some View {
        VStack(alignment: .leading, spacing: 0) {
            CardHeading(title: String(localized: "To share out"))
            Text(quietText)
                .font(Face.row)
                .foregroundStyle(Ink.text3)
                .padding(.bottom, 2)
        }
        .headedCard()
    }

    private var quietText: String {
        guard let plan = store.allocationPlan else {
            return store.canWrite
                ? String(localized: "No allocation plan yet: add an envelope to the plan below to start one.")
                : String(localized: "This vault has no allocation plan.")
        }
        guard plan.enabled else { return String(localized: "Nothing to share out: the plan is paused.") }
        guard let next = AllocationSchedule.next(plan.schedule, after: today) else {
            return String(localized: "Nothing to share out: the plan has no periods left.")
        }
        return String(localized: "Nothing to share out until \(RecurringDayText.weekday(next)).")
    }
}

/// The plan's next period after `day`, from the core's `scheduleOccurrences`;
/// `nil` past the schedule's end or for a schedule it refuses.
enum AllocationSchedule {
    static func next(_ schedule: Schedule, after day: NaiveDate) -> NaiveDate? {
        guard let from = NaiveDay.adding(1, to: day) else { return nil }
        return (try? CoreSchedule.occurrences(schedule, from, 1))?.first
    }
}

/// A dashed hairline between the rows of a short list, the canvas's
/// `1px dashed var(--line)`.
struct DashedRule: View {
    var body: some View {
        DashedLine()
            .stroke(Ink.line, style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
            .frame(height: 1)
    }
}

private nonisolated struct DashedLine: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.midY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
        return path
    }
}

/// A label in `text3` and its amount in semibold, red when `negative`, read
/// as one: the figures under the period due and under a move.
struct AmountFigure: View {
    let label: String
    let amount: Int64
    var negative = false
    var spacing: CGFloat = 4

    var body: some View {
        HStack(spacing: spacing) {
            Text(label).foregroundStyle(Ink.text3)
            Text(LedgerMoney.bare(amount))
                .fontWeight(.semibold)
                .foregroundStyle(negative ? Ink.negative : Ink.text)
        }
        .accessibilityElement(children: .combine)
    }
}
