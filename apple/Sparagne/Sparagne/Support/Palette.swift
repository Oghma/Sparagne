import SwiftUI

/// The fixed dark palette of `docs/v2/UI.md` §5.
///
/// These are constants, not system colors: the window forces `.dark` and
/// paints its own ground, because the ledger is a designed surface (a dark
/// finance terminal) rather than a document that should follow the system
/// tint. Grounds step up in lightness from `bg` to `hi`; amber (`accent`) is
/// reserved for interaction, so a number never looks clickable by color alone.
enum Ink {
    /// The window chrome bars (toolbar, tab bar, status bar).
    static let bg = Color(hex: 0x0A0A0B)
    /// The main content ground, one step above the chrome.
    static let sheet = Color(hex: 0x0E0E10)
    /// A card on the sheet.
    static let card = Color(hex: 0x131316)
    /// One step above `card`, for a hovered or focused row.
    static let raised = Color(hex: 0x1B1B20)
    /// The selected segment of a segmented control.
    static let hi = Color(hex: 0x2A2A31)
    /// Hairlines and card borders.
    static let line = Color(hex: 0x1E1E23)
    /// Stronger borders: control outlines, key caps.
    static let line2 = Color(hex: 0x2B2B32)
    /// The hairline between table rows, fainter than `line`.
    static let rowLine = Color(hex: 0x18181C)
    static let text = Color(hex: 0xEDEDEF)
    /// Secondary text: values that support a heading.
    static let text2 = Color(hex: 0xA3A3AB)
    /// Column headings, placeholders and zero amounts.
    static let text3 = Color(hex: 0x80808A)
    /// Amber: interaction, selection, focus and active states only. Expenses
    /// and errors have their own colors, so amber never means "bad".
    static let accent = Color(hex: 0xFF9A2E)
    static let positive = Color(hex: 0x3ECF8E)
    /// Errors and negative balances. Deliberately not the accent.
    static let negative = Color(hex: 0xFF6B6B)
    /// The darker half of a two-tone bar (the second person, last year): a
    /// dark step of `chartExpense`.
    static let muted = Color(hex: 0x4A2A12)
    static let mutedPositive = Color(hex: 0x1D5340)
    /// Amber-yellow: a refused sync change, the mid band of a budget bar
    /// (`docs/v2/UI.md` §5's table). Used wherever the ledger needs a caution
    /// color instead of system orange.
    static let warning = Color(hex: 0xE8A33D)

    /// Chart series, validated for color-blind separation on `#111113`.
    static let chartIncome = Color(hex: 0x5A8CF0)
    static let chartExpense = Color(hex: 0xD6742A)
    static let chartSavings = Color(hex: 0x25A26C)

    /// Traffic light for a budget bar at a given fill fraction
    /// (`DISTILLATO_V1.md` §3.4: thresholds at 70% and 90%).
    static func progressTint(_ fraction: Double) -> Color {
        switch fraction {
        case ..<0.7: positive
        case ..<0.9: warning
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

/// Type scale. SF Pro with tabular figures, so columns of numbers line up
/// without per-column formatting tricks; the one monospaced face left is the
/// key cap.
enum Face {
    /// SF Pro at `size`, digits monospaced.
    static func ui(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight).monospacedDigit()
    }

    /// Table rows and most body text.
    static let row = ui(12)
    /// Column headings and section titles: small, sentence case.
    static let label = ui(11, .medium)
    /// The status bar and the keyboard hints.
    static let footnote = ui(11)
    /// Captions under a figure, chart axes.
    static let small = ui(10.5)
    /// A KPI card's number.
    static let display = ui(22, .semibold)
    /// The savings figure in the summary panel.
    static let headline = ui(24, .semibold)
    /// Key caps: the only monospaced face.
    static let key = Font.system(size: 10.5, design: .monospaced)
}

/// Fixed metrics, so the grid, its header and the footer totals all agree on
/// column widths without a layout pass.
enum Metrics {
    static let rowHeight: CGFloat = 24
    /// A table's column-heading row.
    static let headerHeight: CGFloat = 26
    static let topBar: CGFloat = 44
    static let tabBar: CGFloat = 27
    /// A text field or segmented control.
    static let control: CGFloat = 26
    /// A pill button.
    static let pill: CGFloat = 24
    /// Horizontal padding inside a table cell.
    static let cellPad: CGFloat = 8
    static let cardPad: CGFloat = 12
    static let cardRadius: CGFloat = 8
    static let gutter: CGFloat = 16
}

// MARK: - Shared chrome

/// A section heading: sentence case, `text3` (`docs/v2/UI.md` §5).
struct SectionLabel: View {
    let text: String
    var tint: Color = Ink.text3

    var body: some View {
        Text(text)
            .font(Face.label)
            .foregroundStyle(tint)
    }
}

/// A rounded card with a hairline border. Every box in the mockups is one.
struct Panel<Content: View>: View {
    var padding: CGFloat = Metrics.cardPad
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Ink.card, in: shape)
            .overlay(shape.strokeBorder(Ink.line, lineWidth: 1))
    }

    private var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: Metrics.cardRadius)
    }
}

/// A horizontal proportional bar, as under each category in the summary.
struct MeterBar: View {
    /// 0...1; values outside are clamped.
    let fraction: Double
    var tint: Color = Ink.chartExpense
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
