import Foundation
import SparagneCore

/// The words of a template on the Ricorrenze tab:
/// where its money goes, and what kind it is.
enum DueRecurringText {
    /// What VoiceOver says after an amount, since the color that tells an
    /// income from an expense is not read.
    static func kind(_ kind: TransactionKind) -> String {
        switch kind {
        case .expense: String(localized: "expense")
        case .income: String(localized: "income")
        case .refund: String(localized: "refund")
        case .transferWallet, .transferFlow: String(localized: "transfer")
        }
    }

    /// The wallet's name, or "any wallet": a template without one takes the
    /// only active wallet when a period is recorded
    /// (`core/src/engine/recurring.rs`).
    static func wallet(_ template: RecurringView, names: NameBook) -> String {
        template.walletId.map { names.wallet($0) ?? TransactionRow.placeholder }
            ?? String(localized: "Any wallet")
    }

    /// The envelope's name, or Unallocated, where a template without one
    /// lands.
    static func envelope(_ template: RecurringView, names: NameBook) -> String {
        template.flowId.map { names.flow($0) ?? TransactionRow.placeholder } ?? NameBook.unallocatedLabel
    }

    /// The line under a due period's title: `"Casa · da Conto, busta Casa"`.
    /// The category leads unless it is the title already (a template with no
    /// note is called by its category, `RecurringTitle`).
    static func detail(_ template: RecurringView, names: NameBook) -> String {
        let envelope = envelope(template, names: names)
        let route = template.walletId == nil
            ? String(localized: "from any wallet, envelope \(envelope)")
            : String(localized: "from \(wallet(template, names: names)), envelope \(envelope)")
        let category = template.category?.trimmingCharacters(in: .whitespaces) ?? ""
        guard !category.isEmpty, category != RecurringTitle.of(template) else { return route }
        return "\(category) \u{00B7} \(route)"
    }
}
