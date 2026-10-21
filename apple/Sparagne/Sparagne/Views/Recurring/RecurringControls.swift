import SwiftUI
import SparagneCore

// The small controls of the Ricorrenze and Riparto inspectors, drawn by hand
// like the rest of the window: the system segmented picker and date picker
// are light-on-dark strangers on this ground.

/// A form's choice as a `SegmentedStrip` at its own width, read to VoiceOver
/// as one group called `name`.
struct RecurringSegments<Option: Hashable>: View {
    let options: [Option]
    @Binding var selection: Option
    let label: (Option) -> String
    let name: String

    var body: some View {
        SegmentedStrip(options: options, selection: $selection, label: label, fixed: true)
            .accessibilityElement(children: .contain)
            .accessibilityLabel(name)
    }
}

/// Quando, as both inspectors edit a `ScheduleDraft`: the cadence, "ogni [1]
/// mese, il giorno [1]", Inizio and Fine, then the inspector's own `notes`.
struct ScheduleEditor<Notes: View>: View {
    @Binding var draft: ScheduleDraft
    var cadences = ScheduleDraft.Cadence.allCases
    /// Riparto has no Fine: the plan's end goes through as it is.
    var showsEnd = true
    @ViewBuilder var notes: Notes

    var body: some View {
        FormGroup(String(localized: "When")) {
            RecurringSegments(
                options: cadences,
                selection: $draft.cadence,
                label: Self.cadenceLabel,
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
            if showsEnd {
                FormRow(String(localized: "End")) {
                    HStack(spacing: 8) {
                        RecurringSegments(
                            options: [false, true],
                            selection: $draft.hasEndDate,
                            label: { $0 ? String(localized: "On a day\u{2026}") : String(localized: "Never") },
                            name: String(localized: "End")
                        )
                        if draft.hasEndDate {
                            DayField(label: String(localized: "End"), day: $draft.shownEndDate)
                        }
                    }
                }
            }
            notes
        }
    }

    private static func cadenceLabel(_ cadence: ScheduleDraft.Cadence) -> String {
        switch cadence {
        case .daily: String(localized: "Day")
        case .weekly: String(localized: "Week")
        case .monthly: String(localized: "Month")
        case .yearly: String(localized: "Year")
        }
    }

    /// "ogni [1] mese,". The spacing is tight on purpose: in English, with
    /// "month," and "on day", the row only just fits the inspector's 340 pt on
    /// one line, and it wraps (`ViewThatFits`) at any looser gap.
    private var every: some View {
        HStack(spacing: 4) {
            Text(String(localized: "every"))
            MiniNumberField(label: String(localized: "Interval"), value: $draft.interval)
            Text(unitWord + (draft.cadence == .daily ? "" : ","))
        }
        .font(Face.row)
        .foregroundStyle(Ink.text)
        .fixedSize()
    }

    /// The unit after the interval, singular for one. Two plain words rather
    /// than a plural key: the number is in the box before it, not in the
    /// sentence.
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
        .font(Face.row)
        .foregroundStyle(Ink.text)
        .fixedSize()
    }
}

extension ScheduleEditor where Notes == EmptyView {
    init(draft: Binding<ScheduleDraft>) {
        self.init(draft: draft) { EmptyView() }
    }
}

/// The 44 × 22 number box of "ogni [n] mesi, il giorno [n]".
struct MiniNumberField: View {
    let label: String
    @Binding var value: Int
    /// Its own, so emptying the box to type another number does not snap
    /// back to a zero; the number goes out as soon as it reads as one.
    @State private var text = ""

    var body: some View {
        TextField(label, text: $text, prompt: Text(verbatim: ""))
            .textFieldStyle(.plain)
            .labelsHidden()
            .font(Face.row)
            .multilineTextAlignment(.center)
            .frame(width: 44, height: 22)
            .background(Ink.card, in: RoundedRectangle(cornerRadius: 5))
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Ink.line2, lineWidth: 1))
            .accessibilityLabel(label)
            .onAppear { text = String(value) }
            .onChange(of: value) { _, new in
                // An empty box already means zero: rewriting it as "0" would
                // get in the way of the number being typed.
                if (Int(text) ?? 0) != new { text = String(new) }
            }
            .onChange(of: text) { _, new in
                let digits = new.filter(\.isNumber)
                if digits != new { text = digits }
                // An empty box is no number at all, which the draft takes as
                // zero: invalid, so nothing can be saved until it is filled.
                value = Int(digits.prefix(4)) ?? 0
            }
    }
}

/// A day, as a box that opens a calendar. The system's compact date picker
/// draws a light field on this dark ground.
struct DayField: View {
    let label: String
    @Binding var day: NaiveDate
    @State private var picking = false

    var body: some View {
        Button {
            picking = true
        } label: {
            FormBox {
                Text(RecurringDayText.full(day))
                Spacer(minLength: 4)
                Image(systemName: "calendar")
                    .font(.system(size: 11))
                    .foregroundStyle(Ink.text3)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityValue(RecurringDayText.full(day))
        .popover(isPresented: $picking, arrowEdge: .bottom) {
            DatePicker(
                label,
                selection: Binding(
                    get: { CoreDate.localDay(day) ?? Date() },
                    set: { new in
                        day = CoreDate.day(new)
                        picking = false
                    }
                ),
                displayedComponents: .date
            )
            .datePickerStyle(.graphical)
            .labelsHidden()
            .padding(10)
        }
    }
}

/// The canvas's "Registra": a small bordered button with the accent as its
/// line and its text, quieter than the filled "Registra tutte" above it.
struct AccentOutlineButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        AccentOutlineBody(configuration: configuration)
    }
}

private struct AccentOutlineBody: View {
    let configuration: ButtonStyleConfiguration
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        configuration.label
            .font(Face.label)
            .foregroundStyle(Ink.accent)
            .padding(.horizontal, 8)
            .frame(height: 22)
            .background(Ink.card, in: RoundedRectangle(cornerRadius: 5))
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Ink.accent.opacity(0.4), lineWidth: 1))
            .contentShape(RoundedRectangle(cornerRadius: 5))
            .opacity(isEnabled ? (configuration.isPressed ? 0.8 : 1) : 0.4)
    }
}

/// Amounts as the tab writes them: income with its plus, and in the agenda expenses with their minus.
enum RecurringAmount {
    /// The table and "Da confermare": an expense bare, an income `+`.
    static func text(_ template: RecurringView) -> String {
        template.kind == .income ? "+" + LedgerMoney.bare(template.amount) : LedgerMoney.bare(template.amount)
    }

    /// An agenda tile: `−60,00` or `+2.400,00`.
    static func signed(_ template: RecurringView) -> String {
        (template.kind == .income ? "+" : "\u{2212}") + LedgerMoney.bare(template.amount)
    }

    static func color(_ template: RecurringView) -> Color {
        template.kind == .income ? Ink.positive : Ink.text
    }
}
