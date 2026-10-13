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
    static func rows(_ totals: [CategoryTotals]) -> [CategoryShare] {
        // A stub for now.
        _ = totals
        return []
    }
}
