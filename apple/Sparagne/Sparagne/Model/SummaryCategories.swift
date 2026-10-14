import Foundation
import SparagneCore

/// One line of the RIEPILOGO's "categories of the year": a category's net
/// expense and its share of what the categories spent between them, or the
/// line that folds every category past the heaviest few.
struct CategoryShare: Identifiable, Hashable, Sendable {
    /// The category, `nil` on the "other" line.
    let categoryId: Uuid?
    /// The core's name; a system category is localized by the view, as the
    /// Mastro's panel does.
    let name: String
    let isSystem: Bool
    /// The line that folds the categories past `SummaryCategories.limit`.
    let isOther: Bool
    /// Net expense, minor units.
    let amount: Int64
    /// Rows behind the figure.
    let count: Int
    /// 0...1 of what the categories spent between them.
    let share: Double

    var id: String { categoryId ?? "other" }
}

/// The year's expenses by category, from the core's `category_totals` over
/// January up to the month on screen.
enum SummaryCategories {
    /// How many categories get a line of their own before "other".
    static let limit = 8

    /// The heaviest `limit` categories with a net expense, then one "other"
    /// line for the rest when there is any. The share is on the total the
    /// categories spent between them, as the Mastro's panel counts it: a
    /// refund filed where nothing was spent lowers no category's figure.
    ///
    /// `totals` come from the core heaviest first, and keep that order: the
    /// core breaks the ties, so the list and the Mastro's panel agree on them.
    static func rows(_ totals: [CategoryTotals]) -> [CategoryShare] {
        let spent = totals.reduce(Int64(0)) { $0 + max($1.netExpense, 0) }
        let spending = totals.filter { $0.netExpense > 0 }
        guard spent > 0, !spending.isEmpty else { return [] }
        let base = Double(spent)

        var lines = spending.prefix(limit).map { category in
            CategoryShare(
                categoryId: category.categoryId,
                name: category.name,
                isSystem: category.isSystem,
                isOther: false,
                amount: category.netExpense,
                count: Int(category.count),
                share: Double(category.netExpense) / base
            )
        }
        let rest = spending.dropFirst(limit)
        if !rest.isEmpty {
            let amount = rest.reduce(Int64(0)) { $0 + $1.netExpense }
            lines.append(
                CategoryShare(
                    categoryId: nil,
                    // The view names it, in the window's language.
                    name: "",
                    isSystem: false,
                    isOther: true,
                    amount: amount,
                    count: rest.reduce(0) { $0 + Int($1.count) },
                    share: Double(amount) / base
                )
            )
        }
        return lines
    }
}
