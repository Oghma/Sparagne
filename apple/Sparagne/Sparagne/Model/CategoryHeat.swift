import Foundation
import SparagneCore

/// One category's net expense in one bucket of the core's
/// `category_bucket_totals`. Bucket 0 is everything before January, 1...12
/// the months of the year.
struct CategoryBucket: Hashable, Sendable {
    let bucket: Int
    let categoryId: Uuid
    let name: String
    let isSystem: Bool
    let netExpense: Int64
    let count: Int
}

/// One line of the months x categories grid.
struct CategoryHeatRow: Identifiable, Hashable, Sendable {
    /// The category, `nil` on the "other" line.
    let categoryId: Uuid?
    let name: String
    let isSystem: Bool
    let isOther: Bool
    /// January first, always twelve; a month after the one on screen is
    /// zero, and drawn as a dash.
    let cells: [Int64]

    var id: String { categoryId ?? "other" }
}

/// The RIEPILOGO's months x categories grid: the same lines as the year's
/// category list, each month's net expense in a cell shaded against the
/// heaviest cell of the grid.
struct CategoryHeat: Hashable, Sendable {
    /// In the order of the category list.
    let rows: [CategoryHeatRow]
    /// The months up to the one on screen: the columns with a figure.
    let elapsedMonths: Int
    /// The heaviest cell, what every shade is measured on.
    let maximum: Int64

    /// 0...1 of `maximum`; 0 when the grid is empty.
    func intensity(_ value: Int64) -> Double {
        // A stub for now.
        _ = value
        return 0
    }

    /// `shares` are the lines of the year's category list, `buckets` the
    /// core's month x category rows over the same bounds as the year
    /// breakdown; a category past the list's own lines lands on "other".
    static func build(buckets: [CategoryBucket], shares: [CategoryShare], upTo: MonthKey) -> CategoryHeat {
        // A stub for now.
        _ = (buckets, shares)
        return CategoryHeat(rows: [], elapsedMonths: upTo.month, maximum: 0)
    }
}
