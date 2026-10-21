import SwiftUI

/// The flat segmented control of the canvas (`.seg`): a 1-point outline on
/// the chrome's ground, the active segment lifted onto `hi` in full-strength
/// text. Not the accent: the accent marks what can be acted on, and every
/// segment can. `Picker(.segmented)` cannot be made to look like this, so it
/// is drawn by hand.
struct SegmentedStrip<Option: Hashable>: View {
    let options: [Option]
    @Binding var selection: Option
    let label: (Option) -> String
    /// At its own width whatever it is offered, for a strip in a table cell
    /// or a form row that would otherwise squeeze it.
    var fixed = false

    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        HStack(spacing: 1) {
            ForEach(options, id: \.self) { option in
                let active = option == selection
                Button {
                    selection = option
                } label: {
                    Text(label(option))
                        .font(Face.ui(11.5, .medium))
                        .foregroundStyle(active ? Ink.text : Ink.text2)
                        .lineLimit(1)
                        .padding(.horizontal, 8)
                        .frame(height: 18)
                        .background {
                            if active {
                                RoundedRectangle(cornerRadius: 4)
                                    .fill(Ink.hi)
                                    .shadow(color: .black.opacity(0.45), radius: 1, y: 1)
                            }
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                // One button per segment for VoiceOver, saying which one is
                // on: the fill that says it on screen is not read.
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(label(option))
                .accessibilityAddTraits(active ? [.isButton, .isSelected] : .isButton)
                .accessibilityAction { if isEnabled { selection = option } }
            }
        }
        .padding(1)
        .background(Ink.bg, in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Ink.line2, lineWidth: 1))
        // Dimmed less than a disabled button: a fixed choice (the kind of a
        // saved template) still has to be read.
        .opacity(isEnabled ? 1 : 0.75)
        .fixedSize(horizontal: fixed, vertical: fixed)
    }
}
