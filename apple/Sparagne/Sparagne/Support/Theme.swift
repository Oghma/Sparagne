import SwiftUI
import AppKit
import SparagneCore

/// Semantic colors and icons, built on top of system colors/symbols so they
/// pick up light/dark, accessibility contrast and tint preferences for free.
enum AppTheme {
    static let income = Color(nsColor: .systemGreen)
    static let expense = Color(nsColor: .systemRed)
    static let refund = Color(nsColor: .secondaryLabelColor)
    static let transfer = Color(nsColor: .systemBlue)
    static let warning = Color(nsColor: .systemOrange)
    static let critical = Color(nsColor: .systemRed)

    /// SF Symbol for a transaction kind, used in the Type column.
    static func symbolName(for kind: TransactionKind) -> String {
        switch kind {
        case .income: "arrow.down.circle.fill"
        case .expense: "arrow.up.circle.fill"
        case .refund: "arrow.uturn.backward.circle.fill"
        case .transferWallet: "arrow.left.arrow.right.circle.fill"
        case .transferFlow: "arrow.left.arrow.right.circle"
        }
    }

    /// Amount/icon color for a transaction row (docs/v2/DISTILLATO_V1.md §3.3).
    static func amountColor(for kind: TransactionKind) -> Color {
        switch kind {
        case .income: income
        case .expense: expense
        case .refund: refund
        case .transferWallet, .transferFlow: transfer
        }
    }

    /// Traffic-light tint for a budget bar at a given fill fraction (§3.4:
    /// thresholds at 70% and 90%).
    static func progressTint(_ fraction: Double) -> Color {
        switch fraction {
        case ..<0.7: income
        case ..<0.9: warning
        default: critical
        }
    }
}
