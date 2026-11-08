import SwiftUI

/// The year's expenses by category: the heaviest eight with a bar and their
/// share, then "other". The lines are the Mastro's panel's for a month
/// (`CategoryBreakdown`), so the two read alike.
struct CategoryYearCard: View {
    let categories: [CategoryShare]

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(text: String(localized: "Expenses by category \u{00B7} year"), tint: Ink.text2)
                    .accessibilityAddTraits(.isHeader)
                if categories.isEmpty {
                    Text(String(localized: "Nothing spent this year"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                } else {
                    VStack(spacing: 7) {
                        ForEach(categories) { CategoryShareLine(category: $0, heaviest: heaviest) }
                    }
                }
            }
            // As tall as the heaviest expenses beside it.
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    /// Against the heaviest line, "other" included: the lines past the
    /// eighth can add up to more than the first, and a bar never runs past
    /// its track.
    private var heaviest: Int64 {
        categories.map(\.amount).max() ?? 0
    }
}

/// One category's line: the name, the amount and its share, over a bar
/// against the heaviest line's amount.
struct CategoryShareLine: View {
    let category: CategoryShare
    let heaviest: Int64

    var body: some View {
        let name = SummaryText.categoryName(category.name, isSystem: category.isSystem, isOther: category.isOther)
        let share = SummaryText.percent(category.share)
        // "Other" folds what is left: told apart from a category by the
        // secondary ink, as a sum is.
        let tint = category.isOther ? Ink.text2 : Ink.text
        VStack(spacing: 3) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(name)
                    .foregroundStyle(tint)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                Text(LedgerMoney.bare(category.amount))
                    .foregroundStyle(tint)
                Text(share)
                    .font(Face.footnote)
                    .foregroundStyle(Ink.text3)
                    .frame(width: 32, alignment: .trailing)
            }
            .font(Face.row)
            MeterBar(fraction: fraction, track: Ink.track, height: 4, rounded: true)
        }
        // The bar is decoration for the same figure, not a second one: one
        // stop, "Casa: €4,800.00", 22%.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(AccessibilityText.figure(name, amount: LedgerMoney.amount(category.amount)))
        .accessibilityValue(share)
    }

    private var fraction: Double {
        guard heaviest > 0 else { return 0 }
        return Double(category.amount) / Double(heaviest)
    }
}
