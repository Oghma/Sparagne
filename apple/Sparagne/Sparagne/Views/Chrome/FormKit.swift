import SwiftUI

// The pieces every form of the app is drawn with (`docs/v2/UI.md` §5): the
// Ricorrenze inspector, the modal sheets and the Settings window. One
// implementation, so a field in a sheet and a field in the inspector are the
// same box and change together; only the density differs (`FormMetrics`).
//
// Drawn by hand like the rest of the window: the system's grouped form,
// text field, switch and pop-up button are light-on-dark strangers on this
// ground.

/// How dense a form is. The inspector packs a column beside a table; a sheet
/// is a dialog of its own and gets room to breathe. Read from the
/// environment, so a form says it once (`FormSheet` does) instead of on
/// every row.
struct FormMetrics: Equatable, Sendable {
    /// The label column on the left of a `FormRow`.
    var labelWidth: CGFloat
    /// A `FormBox`'s height.
    var controlHeight: CGFloat
    /// The labels and what is typed.
    var fontSize: CGFloat
    /// Between the rows of a `FormGroup`.
    var rowSpacing: CGFloat
    /// An accent outline on the field being typed in. A sheet has nothing
    /// else to show where the cursor is; the inspector draws the amount, its
    /// one hero field, with the accent already, and keeps the rest quiet.
    var showsFocus: Bool

    /// The Ricorrenze inspector, and the default.
    static let inspector = FormMetrics(labelWidth: 80, controlHeight: 24, fontSize: 12, rowSpacing: 5, showsFocus: false)
    /// A sheet and the Settings window.
    static let sheet = FormMetrics(labelWidth: 132, controlHeight: 28, fontSize: 13, rowSpacing: 6, showsFocus: true)
}

extension EnvironmentValues {
    @Entry var formMetrics: FormMetrics = .inspector
}

// MARK: - Groups and rows

/// `DOVE`, `QUANDO`: a group's heading in small capitals, in `text3`.
struct FormGroupLabel: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(Face.ui(10.5, .semibold))
            .tracking(0.5)
            .textCase(.uppercase)
            .foregroundStyle(Ink.text3)
            .padding(.bottom, 1)
            .accessibilityAddTraits(.isHeader)
    }
}

/// A group of rows under an optional `FormGroupLabel`, as close together as
/// the form's density says.
struct FormGroup<Content: View>: View {
    let title: String?
    @ViewBuilder var content: Content

    @Environment(\.formMetrics) private var metrics

    init(_ title: String? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: metrics.rowSpacing) {
            if let title {
                FormGroupLabel(title)
            }
            content
        }
    }
}

/// A labelled row: the label in the form's label column, in `text2`, the
/// control after it. Centered on a one-line control; on the first line of
/// one that wraps.
///
/// The label is hidden from VoiceOver: the control carries the same words as
/// its own label, and reading both would say them twice. A row whose value
/// is plain text is a `FormValueRow`.
struct FormRow<Content: View>: View {
    let label: String
    var alignment: VerticalAlignment = .center
    @ViewBuilder var content: Content

    @Environment(\.formMetrics) private var metrics

    init(_ label: String, alignment: VerticalAlignment = .center, @ViewBuilder content: () -> Content) {
        self.label = label
        self.alignment = alignment
        self.content = content()
    }

    var body: some View {
        HStack(alignment: alignment, spacing: 8) {
            Text(label)
                .font(Face.ui(metrics.fontSize))
                .foregroundStyle(Ink.text2)
                .frame(width: metrics.labelWidth, alignment: .leading)
                .accessibilityHidden(true)
            content
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(minHeight: metrics.controlHeight + 2)
    }
}

/// A fact in a form: the label, and its value as text. Read as one element,
/// "label, value", where `FormRow` leaves the label to the control.
struct FormValueRow: View {
    let label: String
    let value: String

    @Environment(\.formMetrics) private var metrics

    init(_ label: String, value: String) {
        self.label = label
        self.value = value
    }

    var body: some View {
        HStack(spacing: 8) {
            Text(label)
                .foregroundStyle(Ink.text2)
                .frame(width: metrics.labelWidth, alignment: .leading)
            Text(value)
                .foregroundStyle(Ink.text)
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
        }
        .font(Face.ui(metrics.fontSize))
        .frame(minHeight: metrics.controlHeight + 2)
        .accessibilityElement(children: .combine)
    }
}

/// A line under a field, in the control column: a rule the field has to
/// meet, a warning that never blocks, or why something failed with the
/// server's own words as the detail. `indented: false` spans the whole form,
/// for a paragraph about a group rather than a field.
struct FormNote: View {
    enum Tone { case plain, warning, negative }

    let text: String
    var tone: Tone = .plain
    var detail: String?
    var indented = true

    @Environment(\.formMetrics) private var metrics

    init(_ text: String, tone: Tone = .plain, detail: String? = nil, indented: Bool = true) {
        self.text = text
        self.tone = tone
        self.detail = detail
        self.indented = indented
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            if tone == .warning {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: metrics.fontSize - 2))
                    .foregroundStyle(Ink.warning)
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(text)
                    .font(Face.ui(metrics.fontSize - 1))
                    .foregroundStyle(tone == .negative ? Ink.negative : Ink.text2)
                if let detail {
                    Text(detail)
                        .font(Face.ui(metrics.fontSize - 2))
                        .foregroundStyle(Ink.text3)
                }
            }
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.leading, indented ? metrics.labelWidth + 8 : 0)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

// MARK: - Controls

/// The input box (`.inp` of the canvas): a `line2` border on the card
/// ground, as tall as the form's density says.
struct FormBox<Content: View>: View {
    /// Outlined in the accent: the field being typed in.
    var focused = false
    @ViewBuilder var content: Content

    @Environment(\.formMetrics) private var metrics

    var body: some View {
        HStack(spacing: 8) { content }
            .font(Face.ui(metrics.fontSize))
            .foregroundStyle(Ink.text)
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, minHeight: metrics.controlHeight, maxHeight: metrics.controlHeight, alignment: .leading)
            .background(Ink.card, in: RoundedRectangle(cornerRadius: 6))
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(focused ? Ink.accent : Ink.line2, lineWidth: 1))
    }
}

/// A one-line text field inside a `FormBox`. `prompt` is the grey text of an
/// empty field, for a field with no label column beside it or a format to
/// follow; `secure` hides what is typed.
struct FormTextField: View {
    let label: String
    @Binding var text: String
    var prompt: String?
    var secure = false

    @FocusState private var focused: Bool
    @Environment(\.formMetrics) private var metrics

    var body: some View {
        FormBox(focused: metrics.showsFocus && focused) {
            // An empty prompt rather than none: without one, the plain style
            // writes the label into the empty field, beside the label column
            // that already says it.
            let promptText = Text(verbatim: prompt ?? "")
            Group {
                if secure {
                    SecureField(label, text: $text, prompt: promptText)
                } else {
                    TextField(label, text: $text, prompt: promptText)
                }
            }
            .textFieldStyle(.plain)
            .labelsHidden()
            .focused($focused)
            .accessibilityLabel(label)
        }
    }
}

/// A menu drawn as a `FormBox`, the chevron at its end. The items are any
/// buttons; `FormPicker` is the one-of-many case.
struct FormMenu<Items: View>: View {
    let label: String
    let value: String
    @ViewBuilder var items: Items

    var body: some View {
        Menu {
            items
        } label: {
            FormBox {
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

/// One of `options`, as a `FormMenu` whose items carry the system's
/// checkmark on the chosen one.
struct FormPicker<Option: Hashable>: View {
    let label: String
    @Binding var selection: Option
    let options: [Option]
    let title: (Option) -> String

    var body: some View {
        FormMenu(label: label, value: title(selection)) {
            Picker(label, selection: $selection) {
                ForEach(options, id: \.self) { option in
                    Text(title(option)).tag(option)
                }
            }
            .pickerStyle(.inline)
            .labelsHidden()
        }
    }
}

/// The canvas's 24 × 14 switch: amber with a dark knob when on, a grey
/// track when off. VoiceOver reads it as a toggle, with its state.
struct FormSwitch: View {
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

/// A yes-or-no setting: the switch in the control column with its sentence
/// after it, like a checkbox. The sentence is too long for the label column
/// and reads better beside what it turns on.
struct FormToggleRow: View {
    let label: String
    @Binding var isOn: Bool

    @Environment(\.formMetrics) private var metrics

    init(_ label: String, isOn: Binding<Bool>) {
        self.label = label
        _isOn = isOn
    }

    var body: some View {
        FormRow("") {
            HStack(spacing: 8) {
                FormSwitch(label: label, isOn: isOn) { isOn = $0 }
                // The switch already says it to VoiceOver; the sentence is a
                // bigger target for the pointer.
                Text(label)
                    .font(Face.ui(metrics.fontSize))
                    .foregroundStyle(Ink.text)
                    .contentShape(Rectangle())
                    .onTapGesture { isOn.toggle() }
                    .accessibilityHidden(true)
            }
        }
    }
}

// MARK: - Sheets

/// A sheet of the window: its title at 15 semibold with an optional line in
/// `text2` under it, the content, and a footer band of buttons aligned right
/// (`FormCancelButton`, `FormPrimaryButton`, `FormDestructiveButton`). The
/// ground is the window's sheet color, so a dialog reads as part of the
/// terminal rather than the system's grouped form.
struct FormSheet<Content: View, Footer: View>: View {
    let title: String
    var subtitle: String?
    var width: CGFloat = 440
    @ViewBuilder var content: Content
    @ViewBuilder var footer: Footer

    init(
        _ title: String,
        subtitle: String? = nil,
        width: CGFloat = 440,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.title = title
        self.subtitle = subtitle
        self.width = width
        self.content = content()
        self.footer = footer()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(title)
                        .font(Face.ui(15, .semibold))
                        .foregroundStyle(Ink.text)
                        .accessibilityAddTraits(.isHeader)
                    if let subtitle {
                        Text(subtitle)
                            .font(Face.ui(12.5))
                            .foregroundStyle(Ink.text2)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                content
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)

            HStack(spacing: 8) {
                Spacer(minLength: 0)
                footer
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
            .background(Ink.bg)
            .overlay(alignment: .top) { Hairline() }
        }
        .frame(width: width)
        .background(Ink.sheet)
        .presentationBackground(Ink.sheet)
        .environment(\.formMetrics, .sheet)
    }
}

/// Cancel: a ghost button, and esc.
struct FormCancelButton: View {
    var title = String(localized: "Cancel")
    let action: () -> Void

    var body: some View {
        Button(title, role: .cancel, action: action)
            .buttonStyle(.chrome(.ghost))
            .keyboardShortcut(.cancelAction)
    }
}

/// The action the sheet is for: the accent, and ↩.
struct FormPrimaryButton: View {
    let title: String
    var role: ButtonRole?
    let action: () -> Void

    var body: some View {
        Button(title, role: role, action: action)
            .buttonStyle(.chrome(.primary))
            .keyboardShortcut(.defaultAction)
    }
}

/// An action that loses something: its words in `negative`, on a bordered
/// button so it still reads as one. No ↩, so a stray return cannot trigger
/// it.
struct FormDestructiveButton: View {
    let title: String
    var small = false
    let action: () -> Void

    var body: some View {
        Button(role: .destructive, action: action) {
            Text(title).foregroundStyle(Ink.negative)
        }
        .buttonStyle(.chrome(small ? .ghost : .bordered, small: small))
    }
}
