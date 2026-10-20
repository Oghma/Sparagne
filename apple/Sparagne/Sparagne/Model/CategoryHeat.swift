import Foundation
import SparagneCore

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
        guard maximum > 0 else { return 0 }
        return min(max(Double(value) / Double(maximum), 0), 1)
    }

    /// `shares` are the lines of the year's category list, `buckets` the
    /// core's month x category rows over the same bounds as the year
    /// breakdown; a category past the list's own lines lands on "other".
    ///
    /// Bucket 0 is everything before January, and is no column; neither is
    /// a month after the one on screen, whose cells stay zero. A category the
    /// list leaves out with no "other" line (it spent nothing over the year,
    /// a refund having made up for it) has no line here either.
    static func build(buckets: [BucketCategoryTotals], shares: [CategoryShare], upTo: MonthKey) -> CategoryHeat {
        let elapsedMonths = min(max(upTo.month, 0), 12)
        let named = Set(shares.compactMap(\.categoryId))
        var sums: [Uuid: [Int64]] = [:]
        var other = Array(repeating: Int64(0), count: 12)
        for bucket in buckets where (1...12).contains(bucket.bucket) && Int(bucket.bucket) <= elapsedMonths {
            let index = Int(bucket.bucket) - 1
            if named.contains(bucket.categoryId) {
                sums[bucket.categoryId, default: Array(repeating: 0, count: 12)][index] += bucket.netExpense
            } else {
                other[index] += bucket.netExpense
            }
        }

        let rows = shares.map { share in
            CategoryHeatRow(
                categoryId: share.categoryId,
                name: share.name,
                isSystem: share.isSystem,
                isOther: share.isOther,
                cells: share.categoryId.flatMap { sums[$0] }
                    ?? (share.isOther ? other : Array(repeating: 0, count: 12))
            )
        }
        let maximum = rows.lazy.flatMap(\.cells).max() ?? 0
        return CategoryHeat(rows: rows, elapsedMonths: elapsedMonths, maximum: max(maximum, 0))
    }
}
