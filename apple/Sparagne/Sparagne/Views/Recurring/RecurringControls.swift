import SwiftUI
import SparagneCore

// The small controls of the Ricorrenze tab (`docs/v2/UI.md` §2.5), drawn by
// hand like the rest of the window: the system switch, segmented picker and
// date picker are light-on-dark strangers on this ground.

/// The canvas's 24 × 14 switch: amber with a dark knob when on, a grey
/// track when off. VoiceOver reads it as a toggle, with its state.
struct RecurringSwitch: View {
    let label: String
    let isOn: Bool
    let set: (Bool) -> Void

    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        Button {
            set(!isOn)
        } label: {
            ZStack(alignment: isOn ? .trailing : .leading) {
                Capsule().fill(isOn ? Ink.accent : Ink.line2)
                Circle()
                    .fill(isOn ? Color(hex: 0x140A00) : Ink.text3)
                    .frame(width: 10, height: 10)
                    .padding(2)
            }
            .frame(width: 24, height: 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(isEnabled ? 1 : 0.5)
        .help(label)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityAddTraits(.isToggle)
        .accessibilityValue(isOn ? String(localized: "Enabled") : String(localized: "Disabled"))
        .accessibilityAction { set(!isOn) }
    }
}

/// The canvas's `.seg`: segments on the darkest ground, the chosen one
/// raised on `hi`. Not `SegmentedStrip`, whose chosen segment is amber: here
/// the choice is a form value, not the sheet's filter.
struct RecurringSegments<Option: Hashable>: View {
    let options: [Option]
    let selection: Option
    let label: (Option) -> String
    let select: (Option) -> Void
    /// What the group is, for VoiceOver.
    let name: String

    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        HStack(spacing: 1) {
            ForEach(options, id: \.self) { option in
                let active = option == selection
                Button {
                    select(option)
                } label: {
                    Text(label(option))
                        .font(Face.ui(11.5, .medium))
                        .foregroundStyle(active ? Ink.text : Ink.text2)
                        .padding(.horizontal, 8)
                        .frame(height: 18)
                        .background(
                            RoundedRectangle(cornerRadius: 4)
                                .fill(active ? Ink.hi : Color.clear)
                                .shadow(color: active ? .black.opacity(0.45) : .clear, radius: 1, y: 1)
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(label(option))
                .accessibilityAddTraits(active ? [.isButton, .isSelected] : .isButton)
            }
        }
        .padding(1)
        .background(Ink.bg, in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Ink.line2, lineWidth: 1))
        .opacity(isEnabled ? 1 : 0.6)
        .fixedSize()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(name)
    }
}

/// The inspector's input box (`.inp`): 24 high, a `line2` border on the
/// card ground.
struct InspectorBox<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 8) { content }
            .font(Face.ui(12))
            .foregroundStyle(Ink.text)
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, minHeight: 24, maxHeight: 24, alignment: .leading)
            .background(Ink.card, in: RoundedRectangle(cornerRadius: 6))
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Ink.line2, lineWidth: 1))
    }
}

/// A one-line text field inside an `InspectorBox`.
struct InspectorTextField: View {
    let label: String
    @Binding var text: String

    var body: some View {
        InspectorBox {
            TextField(label, text: $text, prompt: Text(verbatim: ""))
                .textFieldStyle(.plain)
                .labelsHidden()
                .accessibilityLabel(label)
        }
    }
}

/// The 44 × 22 number box of "ogni [n] mesi, il giorno [n]". It keeps its own
/// text while it is typed in, so emptying it to type another number does not
/// snap back to a zero; the number goes out as soon as it reads as one.
struct MiniNumberField: View {
    let label: String
    @Binding var value: Int
    @State private var text = ""

    var body: some View {
        TextField(label, text: $text, prompt: Text(verbatim: ""))
            .textFieldStyle(.plain)
            .labelsHidden()
            .font(Face.ui(12))
            .multilineTextAlignment(.center)
            .frame(width: 44, height: 22)
            .background(Ink.card, in: RoundedRectangle(cornerRadius: 5))
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Ink.line2, lineWidth: 1))
            .accessibilityLabel(label)
            .onAppear { text = String(value) }
            .onChange(of: value) { _, new in
                if Int(text) != new { text = String(new) }
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
            InspectorBox {
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

/// A menu drawn as an `InspectorBox`, the chevron at its end.
struct InspectorMenu<Items: View>: View {
    let label: String
    let value: String
    @ViewBuilder var items: Items

    var body: some View {
        Menu {
            items
        } label: {
            InspectorBox {
                Text(value).lineLimit(1)
                Spacer(minLength: 4)
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(Ink.text3)
            }
            .contentShape(Rectangle())
        }
        // As in `VaultSelector`: the plain style keeps the label as drawn.
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .accessibilityLabel(label)
        .accessibilityValue(value)
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
            .font(Face.ui(11, .medium))
            .foregroundStyle(Ink.accent)
            .padding(.horizontal, 8)
            .frame(height: 22)
            .background(Ink.card, in: RoundedRectangle(cornerRadius: 5))
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Ink.accent.opacity(0.4), lineWidth: 1))
            .contentShape(RoundedRectangle(cornerRadius: 5))
            .opacity(isEnabled ? (configuration.isPressed ? 0.8 : 1) : 0.4)
    }
}

/// A card's heading line (`.ch`): the title at 11.5 semibold in `text2`, and
/// whatever goes on its right.
struct RecurringCardHeader<Trailing: View>: View {
    let title: String
    var count: Int?
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 8) {
            HStack(spacing: 4) {
                Text(title)
                if let count {
                    Text(count, format: .number).foregroundStyle(Ink.accent)
                }
            }
            .font(Face.ui(11.5, .semibold))
            .foregroundStyle(Ink.text2)
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 8)
            trailing
        }
        .frame(minHeight: 22)
        .padding(.bottom, 8)
    }
}

/// Amounts as the tab writes them: the ledger's fixed `780,00`, income with
/// its plus, and in the agenda expenses with their minus, as money going
/// out.
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
