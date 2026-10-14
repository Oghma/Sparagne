import Foundation
import SparagneCore

/// The usage column of the SETUP categories: how many
/// rows each category has had in the last 12 months, so an unused one is easy
/// to spot before archiving it. A year, not the completion list's 90 days: a
/// category that comes up once a year (a tax, an insurance, the holidays) is
/// alive, and a quarter would show it as dead for most of the year.
enum CategoryUsage {
    static let months = 12

    /// `[start, end)` as the core reads it: the 12 months before `now`, up to
    /// `now` itself. The same day of the month a year earlier, or the last
    /// day the earlier month has (29 February → 28 February).
    static func window(endingAt now: Date, calendar: Calendar = .current) -> (start: Date, end: Date) {
        (calendar.date(byAdding: .month, value: -months, to: now) ?? now, now)
    }

    /// Rows per category. The core leaves out a category with none, and the
    /// table reads a missing id as zero.
    static func counts(from totals: [CategoryTotals]) -> [Uuid: Int] {
        Dictionary(totals.map { ($0.categoryId, Int($0.count)) }, uniquingKeysWith: +)
    }
}

extension AppStore {
    /// Loads `categoryUsage` for the vault on screen. Silent on failure, like
    /// the completion lists: the column is a hint, and a stale one is better
    /// than an alert over a table the user is only reading.
    func loadCategoryUsage(now: Date = Date()) async {
        guard let vault = currentVault else {
            categoryUsage = [:]
            return
        }
        let window = CategoryUsage.window(endingAt: now)
        guard let totals = try? await core.categoryTotals(
            vaultId: vault.id,
            from: CoreDate.utcString(window.start),
            to: CoreDate.utcString(window.end)
        ), currentVault?.id == vault.id else { return }
        categoryUsage = CategoryUsage.counts(from: totals)
    }
}
