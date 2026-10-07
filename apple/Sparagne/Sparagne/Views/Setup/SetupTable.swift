import SwiftUI
import SparagneCore

/// The frame the three SETUP tables share (`docs/v2/UI.md` §2.3): a card
/// with a title row, a header, rows of 24 pt with a hairline under each, and
/// the pieces the rows are made of. Before this each table drew its own
/// copy of the same stack, and they drifted.
///
/// The cell is local rather than the ledger's `GridCell`: this sheet pads
/// cells by 8 and lets rows highlight with rounded corners, which the
/// ledger's grid does not.

/// The width of the trailing column that holds the archive icon.
enum SetupColumn {
    static let action: CGFloat = 36
}

/// One cell: a fixed width (or the rest of the row), one line, 8 pt of
/// padding. The whole cell is the click target, like the ledger's.
struct SetupCell<Content: View>: View {
    var width: CGFloat?
    var alignment: Alignment = .leading
    @ViewBuilder var content: Content

    var body: some View {
        content
            .lineLimit(1)
            .truncationMode(.tail)
            .frame(width: width, alignment: alignment)
            .frame(maxWidth: width == nil ? .infinity : nil, alignment: alignment)
            .padding(.horizontal, Metrics.cellPad)
            .contentShape(Rectangle())
    }
}

/// A rounded card with a title, an optional hint in `text3` and an optional
/// control at the right of the title.
struct SetupCard<Accessory: View, Content: View>: View {
    let title: String
    var hint: String?
    @ViewBuilder var accessory: Accessory
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .center, spacing: 8) {
                Text(title)
                    .font(Face.ui(11.5, .semibold))
                    .foregroundStyle(Ink.text2)
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 8)
                if let hint {
                    Text(hint)
                        .font(Face.ui(11.5, .medium))
                        .foregroundStyle(Ink.text3)
                        .multilineTextAlignment(.trailing)
                        .lineLimit(2)
                }
                accessory
            }
            .frame(minHeight: 22)
            .padding(.bottom, 4)
            content
        }
        .padding(.horizontal, Metrics.cardPad)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Ink.card, in: RoundedRectangle(cornerRadius: Metrics.cardRadius))
        .overlay(RoundedRectangle(cornerRadius: Metrics.cardRadius).strokeBorder(Ink.line, lineWidth: 1))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(title)
    }
}

extension SetupCard where Accessory == EmptyView {
    init(title: String, hint: String? = nil, @ViewBuilder content: () -> Content) {
        self.init(title: title, hint: hint, accessory: { EmptyView() }, content: content)
    }
}

/// The rows of a card's table, pulled 8 pt past the card's padding so the
/// rows' own padding lines their text up with the card's title and a
/// hovered row's rounded background sits 4 pt from the border.
struct SetupTableBody<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(spacing: 0) { content }
            .padding(.horizontal, -Metrics.cellPad)
    }
}

/// The column headings: 11 pt, medium, `text3`, over a `line2` rule.
struct SetupHeaderRow<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 0) { content }
            .font(Face.label)
            .foregroundStyle(Ink.text3)
            .frame(height: Metrics.headerHeight)
            .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
    }
}

/// One 24 pt row. `highlighted` (the pointer is over it, or it is open)
/// paints `raised` with a 4 pt radius and takes the hairline away, as the
/// canvas does; `editing` adds the amber bar at the left edge.
struct SetupRow<Content: View>: View {
    var highlighted = false
    var editing = false
    var separator = true
    /// A `line2` rule above the row, for the totals.
    var topRule = false
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 0) { content }
            .frame(height: Metrics.rowHeight)
            .contentShape(Rectangle())
            .background {
                if highlighted {
                    RoundedRectangle(cornerRadius: 4).fill(Ink.raised)
                }
            }
            .overlay(alignment: .bottom) {
                if separator && !highlighted {
                    Rectangle().fill(Ink.rowLine).frame(height: 1)
                }
            }
            .overlay(alignment: .top) {
                if topRule { Rectangle().fill(Ink.line2).frame(height: 1) }
            }
            .overlay(alignment: .leading) {
                if editing {
                    RoundedRectangle(cornerRadius: 1).fill(Ink.accent).frame(width: 2).padding(.vertical, 3)
                }
            }
    }
}

/// A small label in a `raised` pill: "system", "archived".
struct SetupTag: View {
    let text: String

    var body: some View {
        Text(text)
            .font(Face.ui(10, .semibold))
            .foregroundStyle(Ink.text3)
            .padding(.horizontal, 5)
            .frame(height: 16)
            .background(Ink.raised, in: RoundedRectangle(cornerRadius: 4))
            .fixedSize()
    }
}

/// An alias as a chip.
struct AliasChip: View {
    let text: String

    var body: some View {
        Text(text)
            .font(Face.ui(11))
            .foregroundStyle(Ink.text2)
            .padding(.horizontal, 5)
            .frame(height: 16)
            .background(Ink.raised, in: RoundedRectangle(cornerRadius: 4))
            .fixedSize()
    }
}

/// The icon button of a row's last column. Only the row under the pointer
/// shows one, so a quiet table stays quiet.
struct SetupIconButton: View {
    let symbol: String
    let help: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(Ink.text3)
                .frame(width: 20, height: 20)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
        .accessibilityLabel(label)
    }
}

/// How full a capped envelope is, as the 2 pt bar under its balance
/// (`docs/v2/UI.md` §2.3). The RIEPILOGO's gauges answer the same question,
/// so this asks `FundGauge` rather than redoing its arithmetic: the balance
/// against a net cap, the cumulative income against an income cap.
enum EnvelopeCapFill {
    /// 0...1, or `nil` for an envelope with no cap (no bar at all).
    static func fraction(mode: FlowMode, balance: Int64, incomeTotal: Int64?) -> Double? {
        let gauge: FundGauge
        switch mode {
        case .unlimited:
            return nil
        case .netCapped(let cap):
            gauge = FundGauge(id: "", name: "", cap: cap, filled: balance)
        case .incomeCapped(let cap):
            gauge = FundGauge(id: "", name: "", cap: cap, filled: incomeTotal ?? balance)
        }
        return gauge.fraction
    }
}

/// The 72 × 2 pt bar itself.
struct CapBar: View {
    let fraction: Double

    var body: some View {
        ZStack(alignment: .leading) {
            Capsule().fill(Ink.line2)
            Capsule().fill(Ink.chartIncome).frame(width: 72 * fraction)
        }
        .frame(width: 72, height: 2)
        .accessibilityElement()
        .accessibilityLabel(String(localized: "\(fraction.formatted(.percent.precision(.fractionLength(0)))) of the cap"))
    }
}
