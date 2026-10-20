import SwiftUI

/// A card's title line (`.ch`): the title, an accent count, and at the right a hint and the card's controls.
struct CardHeading<Trailing: View>: View {
    let title: String
    var count: Int?
    var hint: String?
    /// A Setup card's column headings sit closer under it.
    var bottomPadding: CGFloat = 8
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
            if let hint {
                Text(hint)
                    .font(Face.ui(11.5, .medium))
                    .foregroundStyle(Ink.text3)
                    .multilineTextAlignment(.trailing)
                    .lineLimit(2)
            }
            trailing
        }
        .frame(minHeight: 22)
        .padding(.bottom, bottomPadding)
    }
}

extension CardHeading where Trailing == EmptyView {
    init(title: String, count: Int? = nil, hint: String? = nil, bottomPadding: CGFloat = 8) {
        self.init(title: title, count: count, hint: hint, bottomPadding: bottomPadding) { EmptyView() }
    }
}

extension View {
    /// A card with a heading and no table edge to edge: on a `Panel`, 12 at
    /// the sides, 10 above and below.
    func headedCard() -> some View {
        Panel(padding: 0) {
            padding(.horizontal, Metrics.cardPad).padding(.vertical, 10)
        }
    }
}
