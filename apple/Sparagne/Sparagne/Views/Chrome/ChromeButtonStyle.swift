import SwiftUI

/// The square-cornered buttons of the window's chrome (`docs/v2/UI.md` §5):
/// bordered for an ordinary action, ghost for a quiet one beside it, primary
/// for the one the context is about, warning for a caution. Pills
/// (`PillButtonStyle`) are for states; these are for actions.
struct ChromeButtonStyle: ButtonStyle {
    enum Kind { case bordered, ghost, primary, warning }

    var kind: Kind = .bordered
    /// The popover's size, beside a paragraph rather than in a bar.
    var small = false

    func makeBody(configuration: Configuration) -> some View {
        ChromeButtonBody(configuration: configuration, kind: kind, small: small)
    }
}

/// A view of its own so it can read `isEnabled`, which a `ButtonStyle`
/// cannot.
private struct ChromeButtonBody: View {
    let configuration: ButtonStyleConfiguration
    let kind: ChromeButtonStyle.Kind
    let small: Bool
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        configuration.label
            .font(Face.ui(small ? 11 : 12, kind == .primary ? .semibold : .medium))
            .foregroundStyle(text)
            .padding(.horizontal, small ? 8 : 10)
            .frame(height: small ? 22 : Metrics.control)
            .background(fill, in: shape)
            .overlay(shape.strokeBorder(border, lineWidth: 1))
            .contentShape(shape)
            .opacity(isEnabled ? (configuration.isPressed ? 0.8 : 1) : 0.4)
    }

    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: small ? 5 : 7) }

    private var text: Color {
        switch kind {
        case .bordered: Ink.text
        case .ghost: Ink.text2
        // Dark text on the amber: white on it fails contrast.
        case .primary: Color(hex: 0x140A00)
        case .warning: Ink.warning
        }
    }

    private var fill: Color {
        switch kind {
        case .bordered: Ink.card
        case .ghost: .clear
        case .primary: Ink.accent
        case .warning: Ink.warning.opacity(0.12)
        }
    }

    private var border: Color {
        switch kind {
        case .bordered: Ink.line2
        case .ghost: .clear
        case .primary: Ink.accent
        case .warning: Ink.warning.opacity(0.4)
        }
    }
}

extension ButtonStyle where Self == ChromeButtonStyle {
    /// `.buttonStyle(.chrome(.ghost))`.
    static func chrome(_ kind: ChromeButtonStyle.Kind = .bordered, small: Bool = false) -> ChromeButtonStyle {
        ChromeButtonStyle(kind: kind, small: small)
    }
}
