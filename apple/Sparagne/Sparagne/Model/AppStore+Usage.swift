import Foundation
import SparagneCore

/// The usage column of the SETUP categories (`docs/v2/UI.md` §2.3): how many
/// rows each category has had in the last 90 days, so an unused one is easy
/// to spot before archiving it.
enum CategoryUsage {
    static let days = 90

    /// `[start, end)` as the core reads it: the 90 days before `now`, up to
    /// `now` itself. The same span as the quick-add completion's.
    static func window(endingAt now: Date, calendar: Calendar = .current) -> (start: Date, end: Date) {
        (calendar.date(byAdding: .day, value: -days, to: now) ?? now, now)
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
