import Foundation
import SparagneCore

/// The figures of the Mastro's status line (`docs/v2/UI.md` §2.1): average,
/// count and sum, the way a spreadsheet's status bar reads the cells it is
/// pointed at.
///
/// The scope is what the user is looking at: two or more picked rows when
/// there are, otherwise every row on screen. One picked row is not a range
/// worth summing (a plain click picks nothing, a ⌘-click picks one), so it
/// counts as no selection rather than as a sum of a single amount.
///
/// Pure, so the arithmetic is tested without a window
/// (`SparagneTests/SheetStatsTests.swift`).
struct SheetStats: Equatable, Sendable {
    enum Scope: Equatable, Sendable {
        /// Every row on screen.
        case visible
        /// The two or more rows picked with ⌘-click, ⇧-click or ⌘A.
        case selection
    }

    let scope: Scope
    /// The rows that went into the sum: voided rows and transfers excluded.
    let count: Int
    /// Minor units. A refund comes off, as it does in the month's net expense.
    let sum: Int64
    /// `sum / count`, rounded half away from zero to the cent; `nil` for an
    /// empty sheet, where there is nothing to average.
    let mean: Int64?

    static func make(rows: [TransactionRow], selection: RowSelection) -> SheetStats {
        let scope: Scope = selection.count >= 2 ? .selection : .visible
        let scoped = scope == .selection ? rows.filter { selection.contains($0.id) } : rows

        var count = 0
        var sum: Int64 = 0
        for row in scoped where !row.voided {
            switch row.kind {
            // A refund sits in the USCITE list and has to come off it, or the
            // sum reads higher than what was actually spent.
            case .refund:
                sum -= row.absoluteAmount
                count += 1
            case .income, .expense:
                sum += row.absoluteAmount
                count += 1
            // A transfer moves money without spending or earning it.
            case .transferWallet, .transferFlow:
                continue
            }
        }
        return SheetStats(scope: scope, count: count, sum: sum, mean: average(sum, count))
    }

    /// Integer division rounded half away from zero, so no float ever touches
    /// an amount (`LedgerMoney`).
    private static func average(_ sum: Int64, _ count: Int) -> Int64? {
        guard count > 0 else { return nil }
        let divisor = Int64(count)
        var quotient = sum / divisor
        let remainder = sum % divisor
        if remainder.magnitude * 2 >= divisor.magnitude {
            quotient += sum < 0 ? -1 : 1
        }
        return quotient
    }
}
