import SwiftUI

/// The fixed dark palette of `docs/v2/UI.md` §5.
///
/// These are constants, not system colors: the window forces `.dark` and
/// paints its own ground, because the ledger is a designed surface (a finance
/// terminal) rather than a document that should follow the system tint.
enum Ink {
    static let bg = Color(hex: 0x0D0D0D)
    static let panel = Color(hex: 0x141414)
    /// One step above `panel`, for a hovered or focused row.
    static let raised = Color(hex: 0x1C1C1C)
    static let line = Color(hex: 0x242424)
    static let text = Color(hex: 0xE6E6E6)
    /// Column headings, placeholders and zero amounts.
    static let dim = Color(hex: 0x7A7A7A)
    /// Active filters, the caret row, expense bars.
    static let accent = Color(hex: 0xFF5A3C)
    static let positive = Color(hex: 0x3ECF8E)
    static let negative = Color(hex: 0xFF5A3C)
    /// The darker half of a two-tone bar (the second person, last year).
    static let muted = Color(hex: 0x5C2318)
    static let mutedPositive = Color(hex: 0x1D5340)

    /// Traffic light for a budget bar at a given fill fraction
    /// (`DISTILLATO_V1.md` §3.4: thresholds at 70% and 90%).
    static func progressTint(_ fraction: Double) -> Color {
        switch fraction {
        case ..<0.7: positive
        case ..<0.9: Color(hex: 0xE8A33D)
        default: negative
        }
    }
}

extension Color {
    /// `0xFF5A3C` -> the color. Opaque; the palette has no alpha.
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: 1
        )
    }
}

/// Type scale. Everything is monospaced with tabular figures, so columns of
/// numbers line up without per-column formatting tricks.
enum Face {
    static func mono(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    /// Table rows and most body text.
    static let row = mono(12)
    /// Column headings and section titles: small, uppercase, tracked.
    static let label = mono(10, .medium)
    /// The status bar and the keyboard hints.
    static let footnote = mono(10)
    /// A KPI card's number.
    static let display = mono(30, .medium)
    /// The savings figure in the summary panel.
    static let headline = mono(20, .medium)
}

/// Fixed metrics, so the grid, its header and the footer totals all agree on
/// column widths without a layout pass.
enum Metrics {
    static let rowHeight: CGFloat = 27
    static let sidebarWidth: CGFloat = 284
    static let gutter: CGFloat = 16
}

// MARK: - Shared chrome

/// A section heading: uppercase, tracked, dim (`docs/v2/UI.md` §5).
struct SectionLabel: View {
    let text: String
    var tint: Color = Ink.dim

    var body: some View {
        Text(text.uppercased())
            .font(Face.label)
            .tracking(0.8)
            .foregroundStyle(tint)
    }
}

/// A bordered card on the panel ground. Every box in the mockups is one.
struct Panel<Content: View>: View {
    var padding: CGFloat = 14
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Ink.panel)
            .overlay(Rectangle().strokeBorder(Ink.line, lineWidth: 1))
    }
}

/// A horizontal proportional bar, as under each category in the summary.
struct MeterBar: View {
    /// 0...1; values outside are clamped.
    let fraction: Double
    var tint: Color = Ink.accent
    var height: CGFloat = 3

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Rectangle().fill(Ink.line)
                Rectangle()
                    .fill(tint)
                    .frame(width: geometry.size.width * min(max(fraction, 0), 1))
            }
        }
        .frame(height: height)
    }
}

/// A single hairline in the palette's line color. `Divider()` picks up the
/// system separator, which is too bright on this ground.
struct Hairline: View {
    var body: some View {
        Rectangle().fill(Ink.line).frame(height: 1)
    }
}
