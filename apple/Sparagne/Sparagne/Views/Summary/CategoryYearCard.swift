import SwiftUI

/// The year's expenses by category: the heaviest eight with a bar and their
/// share, then "other". Drawn as the Mastro's panel draws a month's
/// (`CategoryBreakdown`), so the two read alike.
struct CategoryYearCard: View {
    let categories: [CategoryShare]

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                Text(String(localized: "Expenses by category \u{00B7} year"))
                    .font(Face.label)
                    .foregroundStyle(Ink.text2)
                    .accessibilityAddTraits(.isHeader)
                if categories.isEmpty {
                    Text(String(localized: "Nothing spent this year"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                } else {
                    VStack(spacing: 7) {
                        ForEach(categories) { line($0) }
                    }
                }
            }
            // As tall as the heaviest expenses beside it.
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    private func line(_ category: CategoryShare) -> some View {
        let name = SummaryText.categoryName(category.name, isSystem: category.isSystem, isOther: category.isOther)
        let share = SummaryText.percent(category.share)
        // "Other" folds what is left: told apart from a category by the
        // secondary ink, as a sum is.
        let tint = category.isOther ? Ink.text2 : Ink.text
        return VStack(spacing: 3) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(name)
                    .foregroundStyle(tint)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                Text(LedgerMoney.bare(category.amount))
                    .foregroundStyle(tint)
                Text(share)
                    .font(Face.ui(11))
                    .foregroundStyle(Ink.text3)
                    .frame(width: 32, alignment: .trailing)
            }
            .font(Face.row)
            ShareBar(fraction: fraction(category))
        }
        // The bar is decoration for the same figure, not a second one: one
        // stop, "Casa: €4,800.00", 22%.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(AccessibilityText.figure(name, amount: LedgerMoney.amount(category.amount)))
        .accessibilityValue(share)
    }

    /// Against the heaviest line, "other" included: the lines past the
    /// eighth can add up to more than the first, and a bar never runs past
    /// its track.
    private func fraction(_ category: CategoryShare) -> Double {
        let heaviest = categories.map(\.amount).max() ?? 0
        guard heaviest > 0 else { return 0 }
        return Double(category.amount) / Double(heaviest)
    }
}

/// A 4-point bar with rounded ends on a faint track, the Mastro's category
/// bar.
private struct ShareBar: View {
    let fraction: Double

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(Color(hex: 0x222228))
                Capsule()
                    .fill(Ink.chartExpense)
                    .frame(width: geometry.size.width * min(max(fraction, 0), 1))
            }
        }
        .frame(height: 4)
        .accessibilityHidden(true)
    }
}
