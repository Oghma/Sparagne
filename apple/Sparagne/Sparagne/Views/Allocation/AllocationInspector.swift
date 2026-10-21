import SwiftUI
import SparagneCore

/// The Riparto tab's inspector: Quando, the plan in figures, Avvisi and the Attivo switch.
struct AllocationInspector: View {
    let store: AppStore
    /// The tab's: a new plan is created on it when its first line is
    /// committed.
    @Binding var draft: AllocationScheduleDraft
    let lines: [AllocationLine]
    let preview: AllocationPreview?
    let due: Bool
    let today: NaiveDate
    /// Whether Salva and Annulla take ↩ and esc: not while a row of the
    /// table is open, whose own ↩ and esc they are.
    var shortcuts = true

    /// A command in flight: Salva and the switch wait for it.
    @State private var working = false

    private var plan: AllocationPlanView? { store.allocationPlan }
    private var editable: Bool { store.canWrite && !working }

    /// The heading follows every keystroke of the schedule, saved or not, as
    /// the Ricorrenze inspector's does.
    var body: some View {
        InspectorShell(
            title: String(localized: "Allocation plan"),
            subtitle: String(localized: "\(ScheduleFormatting.describe(draft.schedule)) \u{00B7} from \(RecurringDayText.full(draft.startDate))"),
            footerDisabled: working
        ) {
            ScheduleEditor(draft: $draft, cadences: cadences, showsEnd: false) {
                FormNote(
                    String(localized: "The first allocation counts the incomes from the start date; the later ones those that arrived after the last allocation, shared out or skipped."),
                    indented: false
                )
                if !draft.isValid {
                    FormNote(String(localized: "These settings do not make a schedule"), tone: .negative, indented: false)
                }
            }
            .disabled(!editable)
            figuresGroup
            warningsGroup
        } footer: {
            footer
        }
    }

    /// Settimana / Mese / Anno, as drawn; a daily schedule, which only a
    /// plan made elsewhere can have, keeps its own segment.
    private var cadences: [ScheduleDraft.Cadence] {
        (draft.cadence == .daily ? [.daily] : []) + [.weekly, .monthly, .yearly]
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
        .font(Face.row)
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

    @ViewBuilder
    private var footer: some View {
        if let plan {
            if store.canWrite {
                InspectorSwitch(label: Self.enabledLabel, isOn: plan.enabled, disabled: working) { on in
                    $working.run { await store.setAllocationEnabled(on) }
                }
            } else {
                Text(plan.enabled ? Self.enabledLabel : String(localized: "paused"))
                    .font(Face.row)
                    .foregroundStyle(Ink.text2)
            }
            Spacer(minLength: 8)
            if store.canWrite, draft.isDirty(against: plan) {
                Button(String(localized: "Cancel")) { draft = AllocationScheduleDraft(schedule: plan.schedule) }
                    .buttonStyle(.chrome(.ghost, small: true))
                    .keyboardShortcut(shortcuts ? .cancelAction : nil)
                if draft.isValid {
                    Button(String(localized: "Save")) {
                        let schedule = draft.schedule
                        $working.run { await store.setAllocationSchedule(schedule) }
                    }
                    .buttonStyle(.chrome(.primary, small: true))
                    .keyboardShortcut(shortcuts ? .defaultAction : nil)
                }
            } else if let hint = footerHint(plan) {
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

    /// The footer's right side: when the plan shares out, or that it is
    /// paused, where the switch on the left does not say it already in
    /// words (a vault only read shows "in pausa" there).
    private func footerHint(_ plan: AllocationPlanView) -> String? {
        if plan.enabled { return AllocationText.cadenceHint(plan.schedule) }
        return store.canWrite ? String(localized: "paused") : nil
    }

    /// "Attivo": the plan's, masculine in Italian, where a template's
    /// "Enabled" is "Attiva".
    static var enabledLabel: String {
        String(localized: "allocation.enabled", defaultValue: "Enabled")
    }
}
