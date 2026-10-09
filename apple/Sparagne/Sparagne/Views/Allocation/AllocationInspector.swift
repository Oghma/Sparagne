import SwiftUI
import SparagneCore

/// The Riparto tab's inspector: when the plan shares out (Quando, Inizio),
/// what its lines ask for ("Il piano in cifre"), what deserves a second look
/// (Avvisi) and the Attivo switch.
///
/// The schedule is a draft owned by the tab, since a new plan is created
/// with it when its first line is committed; once there is a plan, Salva
/// sends it and Annulla puts the saved one back, as in the Ricorrenze
/// inspector. A vault the account only reads shows the fields without
/// letting them change.
struct AllocationInspector: View {
    let store: AppStore
    @Binding var draft: AllocationScheduleDraft
    let lines: [AllocationLine]
    let preview: AllocationPreview?
    let due: Bool
    let today: NaiveDate

    /// A command in flight: Salva and the switch wait for it.
    @State private var working = false

    private var plan: AllocationPlanView? { store.allocationPlan }
    private var editable: Bool { store.canWrite && !working }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    title
                    whenGroup
                        .disabled(!editable)
                    figuresGroup
                    warningsGroup
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            footer
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Ink.bg)
    }

    // MARK: - Heading

    /// The schedule as the fields say it, saved or not: the heading follows
    /// every keystroke, as the Ricorrenze inspector's does.
    private var title: some View {
        let schedule = draft.schedule
        let cadence = ScheduleFormatting.describe(schedule)
        return VStack(alignment: .leading, spacing: 2) {
            Text(String(localized: "Allocation plan"))
                .font(Face.ui(16, .semibold))
                .foregroundStyle(Ink.text)
                .accessibilityAddTraits(.isHeader)
            Text(String(localized: "\(cadence) \u{00B7} from \(RecurringDayText.full(schedule.startDate))"))
                .font(Face.ui(12))
                .foregroundStyle(Ink.text2)
                .lineLimit(2)
        }
    }

    // MARK: - Quando

    private var whenGroup: some View {
        FormGroup(String(localized: "When")) {
            RecurringSegments(
                options: cadences,
                selection: draft.cadence,
                label: Self.cadenceLabel,
                select: { draft.cadence = $0 },
                name: String(localized: "Frequency")
            )
            .padding(.bottom, 2)
            FormRow(String(localized: "Repeat"), alignment: .firstTextBaseline) {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 4) {
                        every
                        on
                    }
                    VStack(alignment: .leading, spacing: 5) {
                        every
                        on
                    }
                }
            }
            FormRow(String(localized: "Start")) {
                DayField(label: String(localized: "Start"), day: $draft.startDate)
            }
            FormNote(
                String(localized: "The first allocation counts the incomes from the start date; the later ones those that arrived after the last allocation, shared out or skipped."),
                indented: false
            )
            if !draft.isValid {
                FormNote(String(localized: "These settings do not make a schedule"), tone: .negative, indented: false)
            }
        }
    }

    /// Settimana / Mese / Anno, as drawn; a daily schedule, which only a
    /// plan made elsewhere can have, keeps its own segment.
    private var cadences: [RecurringDraft.Cadence] {
        (draft.cadence == .daily ? [.daily] : []) + [.weekly, .monthly, .yearly]
    }

    private static func cadenceLabel(_ cadence: RecurringDraft.Cadence) -> String {
        switch cadence {
        case .daily: String(localized: "Day")
        case .weekly: String(localized: "Week")
        case .monthly: String(localized: "Month")
        case .yearly: String(localized: "Year")
        }
    }

    /// "ogni [1] mese,".
    private var every: some View {
        HStack(spacing: 4) {
            Text(String(localized: "every"))
            MiniNumberField(label: String(localized: "Interval"), value: $draft.interval)
            Text(unitWord + (draft.cadence == .daily ? "" : ","))
        }
        .font(Face.ui(12))
        .foregroundStyle(Ink.text)
        .fixedSize()
    }

    private var unitWord: String {
        let one = draft.interval == 1
        switch draft.cadence {
        case .daily: return one ? String(localized: "day") : String(localized: "days")
        case .weekly: return one ? String(localized: "week") : String(localized: "weeks")
        case .monthly: return one ? String(localized: "month") : String(localized: "months")
        case .yearly: return one ? String(localized: "year") : String(localized: "years")
        }
    }

    /// The day the cadence falls on: a weekday, a day of the month, or a
    /// day and a month.
    private var on: some View {
        HStack(spacing: 4) {
            switch draft.cadence {
            case .daily:
                EmptyView()
            case .weekly:
                Text(String(localized: "on"))
                FormMenu(
                    label: String(localized: "Weekday"),
                    value: ScheduleFormatting.weekdayName(UInt8(clamping: draft.weekday))
                ) {
                    ForEach(1...7, id: \.self) { day in
                        Button(ScheduleFormatting.weekdayName(UInt8(day))) { draft.weekday = day }
                    }
                }
                .fixedSize()
            case .monthly:
                Text(String(localized: "on day"))
                MiniNumberField(label: String(localized: "Day of the month"), value: $draft.monthDay)
            case .yearly:
                Text(String(localized: "on the"))
                MiniNumberField(label: String(localized: "Day of the month"), value: $draft.yearDay)
                FormMenu(
                    label: String(localized: "Month"),
                    value: ScheduleFormatting.monthName(UInt8(clamping: draft.yearMonth))
                ) {
                    ForEach(1...12, id: \.self) { month in
                        Button(ScheduleFormatting.monthName(UInt8(month))) { draft.yearMonth = month }
                    }
                }
                .fixedSize()
            }
        }
        .font(Face.ui(12))
        .foregroundStyle(Ink.text)
        .fixedSize()
    }

    // MARK: - Il piano in cifre

    private var figuresGroup: some View {
        let figures = AllocationFigures(lines: lines)
        return FormGroup(String(localized: "The plan in figures")) {
            VStack(spacing: 0) {
                figure(String(localized: "Lines"), figures.lines.formatted(.number))
                figure(String(localized: "Fixed amounts"), LedgerMoney.bare(figures.fixed))
                figure(
                    String(localized: "Percentages"),
                    String(localized: "\(AllocationPercent.label(figures.percent)) of the total")
                )
                figure(String(localized: "Up to the cap"), AllocationText.envelopes(figures.fillToCap))
                figure(String(localized: "allocation.next", defaultValue: "Next"), nextText)
            }
        }
    }

    /// The period after today, while the plan runs.
    private var nextText: String {
        guard let plan, plan.enabled,
              let next = AllocationSchedule.next(plan.schedule, after: today)
        else { return TransactionRow.placeholder }
        return RecurringDayText.weekday(next)
    }

    private func figure(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).foregroundStyle(Ink.text2)
            Spacer(minLength: 8)
            Text(value).fontWeight(.semibold).foregroundStyle(Ink.text)
        }
        .font(Face.ui(12))
        .frame(height: 22)
        .overlay(alignment: .bottom) { DashedRule() }
        .accessibilityElement(children: .combine)
    }

    // MARK: - Avvisi

    private var warningsGroup: some View {
        let names = NameBook(snapshot: store.snapshot)
        let warnings = AllocationWarning.of(
            lines: lines,
            flows: store.snapshot?.flows ?? [],
            preview: preview,
            due: due
        )
        return FormGroup(String(localized: "Warnings")) {
            if warnings.isEmpty {
                note(tag: nil, AllocationText.allClear(percent: AllocationFigures(lines: lines).percent))
            } else {
                ForEach(warnings, id: \.self) { warning in
                    let words = AllocationText.warning(warning, names: names)
                    note(tag: words.tag, words.text)
                }
            }
        }
    }

    /// `[Vacanze] riceve 1.432,50 invece di 1.760,00: …`: the envelope as an
    /// accent tag, the sentence after it.
    private func note(tag: String?, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 4) {
            if let tag { RowTag(text: tag, tint: Ink.accent) }
            Text(text)
                .font(Face.ui(11.5))
                .foregroundStyle(Ink.text3)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: 8) {
            if let plan {
                if store.canWrite {
                    FormSwitch(label: Self.enabledLabel, isOn: plan.enabled) { on in
                        run { await store.setAllocationEnabled(on) }
                    }
                    .disabled(working)
                    // The switch says it already; the word is for the eye.
                    Text(Self.enabledLabel)
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text2)
                        .accessibilityHidden(true)
                } else {
                    Text(plan.enabled ? Self.enabledLabel : String(localized: "paused"))
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text2)
                }
                Spacer(minLength: 8)
                if store.canWrite, draft.isDirty(against: plan) {
                    Button(String(localized: "Cancel")) { draft = AllocationScheduleDraft(schedule: plan.schedule) }
                        .buttonStyle(.chrome(.ghost, small: true))
                        .keyboardShortcut(.cancelAction)
                    if draft.isValid {
                        Button(String(localized: "Save")) {
                            let schedule = draft.schedule
                            run { await store.setAllocationSchedule(schedule) }
                        }
                        .buttonStyle(.chrome(.primary, small: true))
                        .keyboardShortcut(.defaultAction)
                    }
                } else if let hint = plan.enabled ? AllocationText.cadenceHint(plan.schedule) : String(localized: "paused") {
                    Text(hint)
                        .font(Face.ui(11.5))
                        .foregroundStyle(Ink.text3)
                        .lineLimit(1)
                }
            } else {
                Text(store.canWrite
                    ? String(localized: "The plan starts with its first line.")
                    : String(localized: "No allocation plan"))
                    .font(Face.ui(11.5))
                    .foregroundStyle(Ink.text3)
                Spacer(minLength: 0)
            }
        }
        .disabled(working)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(minHeight: 44)
        .overlay(alignment: .top) { Hairline() }
        .background(Ink.bg)
        .accessibilityElement(children: .contain)
    }

    /// "Attivo": the plan's, masculine in Italian, where a template's
    /// "Enabled" is "Attiva".
    static var enabledLabel: String {
        String(localized: "allocation.enabled", defaultValue: "Enabled")
    }

    private func run(_ work: @escaping @MainActor () async -> Void) {
        working = true
        Task {
            await work()
            working = false
        }
    }
}
