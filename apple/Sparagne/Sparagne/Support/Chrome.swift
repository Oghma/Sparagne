import SwiftUI

/// A small capsule button (`docs/v2/UI.md` §5): neutral for ordinary actions,
/// accent for the one that matters, warning for a caution. Pressing dims it
/// rather than changing its color, so every tone reacts the same way.
struct PillButtonStyle: ButtonStyle {
    enum Tone { case neutral, accent, warning }

    var tone: Tone = .neutral

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Face.ui(11.5, .medium))
            .foregroundStyle(textColor)
            .padding(.horizontal, 10)
            .frame(height: Metrics.pill)
            .background(Capsule().fill(fill))
            .overlay(Capsule().strokeBorder(border, lineWidth: 1))
            .contentShape(Capsule())
            .opacity(configuration.isPressed ? 0.8 : 1)
    }

    private var base: Color? {
        switch tone {
        case .neutral: nil
        case .accent: Ink.accent
        case .warning: Ink.warning
        }
    }

    private var textColor: Color { base ?? Ink.text2 }
    private var fill: Color { base?.opacity(0.14) ?? .clear }
    private var border: Color { base?.opacity(0.4) ?? Ink.line2 }
}

extension ButtonStyle where Self == PillButtonStyle {
    /// `.buttonStyle(.pill(.accent))`.
    static func pill(_ tone: PillButtonStyle.Tone = .neutral) -> PillButtonStyle {
        PillButtonStyle(tone: tone)
    }
}

/// A keyboard key drawn as a cap, for the hints in the status bar and the
/// command palette. The only place the monospaced face is used.
struct KeyCap: View {
    let text: String

    var body: some View {
        Text(text)
            .font(Face.key)
            .foregroundStyle(Ink.text3)
            .padding(.vertical, 3)
            .padding(.horizontal, 5)
            .background(Ink.bg, in: shape)
            .overlay(shape.strokeBorder(Ink.line2, lineWidth: 1))
    }

    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: 4) }
}

/// Faint 45° stripes, laid over a row that is pending (not yet synced or
/// confirmed). Barely visible on purpose: it marks state without competing
/// with the text.
struct HatchedFill: View {
    var body: some View {
        Canvas { context, size in
            let spacing: CGFloat = 6
            var path = Path()
            // Lines run from bottom-left to top-right; start far enough left
            // that the first stripe still covers the corner.
            var x = -size.height
            while x < size.width {
                path.move(to: CGPoint(x: x, y: size.height))
                path.addLine(to: CGPoint(x: x + size.height, y: 0))
                x += spacing
            }
            context.stroke(path, with: .color(.white.opacity(0.028)), lineWidth: 1)
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// A status light: green for synced, accent for pending, red for an error.
struct StatusDot: View {
    let color: Color

    var body: some View {
        Circle().fill(color).frame(width: 7, height: 7)
    }
}
