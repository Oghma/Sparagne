import SwiftUI
import SparagneCore

/// The words the import sheet puts on the core's statement values: every
/// stable code of `StatementRowStatus` translated (the English reason stays
/// as the detail), the kinds, the actions and the date formats.
enum StatementText {
    /// The status badge of a previewed row. The badges speak of one row and
    /// the counts of many, so they never share a key: Italian agrees the
    /// adjective in number ("importata", "importate").
    static func status(_ status: StatementRowStatus) -> String {
        switch status {
        case .new: String(localized: "New")
        case .alreadyImported: String(localized: "Imported before")
        case .possibleDuplicate: String(localized: "Possible duplicate")
        case .matchedTransfer: String(localized: "Transfer already there")
        case .skipped(let code, _): skipped(code)
        case .invalid(let code, _): invalid(code)
        }
    }

    /// The core's English sentence behind a status, for the tooltip.
    static func detail(_ status: StatementRowStatus) -> String? {
        switch status {
        case .new, .alreadyImported, .possibleDuplicate, .matchedTransfer: nil
        case .skipped(_, let reason): reason
        case .invalid(_, let message): message
        }
    }

    static func skipped(_ code: String) -> String {
        switch code {
        case "skipped_by_rule": String(localized: "Skipped by a rule")
        case "skipped_status": String(localized: "Skipped for its status")
        case "needs_wallet": String(localized: "Needs the other wallet")
        case "zero_amount": String(localized: "Zero amount")
        case "skipped_by_you": String(localized: "Skipped by you")
        case "before_start": String(localized: "Before the start day")
        default: String(localized: "Not imported")
        }
    }

    static func invalid(_ code: String) -> String {
        switch code {
        case "invalid_row": String(localized: "Malformed row")
        case "invalid_date": String(localized: "Unreadable date")
        case "invalid_amount": String(localized: "Unreadable amount")
        case "currency_mismatch": String(localized: "Another currency")
        default: String(localized: "Invalid row")
        }
    }

    /// The kind column: the same lowercase words the due-recurring sheet
    /// uses.
    static func kind(_ kind: TransactionKind?) -> String {
        switch kind {
        case .expense: String(localized: "expense")
        case .income: String(localized: "income")
        case .refund: String(localized: "refund")
        case .transferWallet, .transferFlow: String(localized: "transfer")
        case nil: TransactionRow.placeholder
        }
    }

    /// The review's KIND cell: what the row is taken for, in the kind
    /// column's lowercase words, a `BySign` as the kind it resolved to and a
    /// transfer with its direction, the other wallet being written beside it.
    static func rowKind(_ action: StatementAction, resolved: TransactionKind?) -> String {
        switch action {
        case .bySign: kind(resolved)
        case .expense: kind(.expense)
        case .income: kind(.income)
        case .refund: kind(.refund)
        case .transferIn: String(localized: "transfer from")
        case .transferOut: String(localized: "transfer to")
        case .skip: String(localized: "skip")
        }
    }

    /// What becomes of a row that matched a transaction already in the
    /// vault, as the review's menu offers it. `skip` keeps the transaction
    /// there as the bank's row, claimed so that no later statement takes it
    /// for one of its own.
    static func matchChoice(_ choice: StatementMatchChoice) -> String {
        switch choice {
        case .skip: String(localized: "Keep the one already there")
        case .moveExisting: String(localized: "Move it here")
        case .importAnyway: String(localized: "Import anyway")
        }
    }

    /// What each choice does, for the menu's tooltip and VoiceOver's hint.
    static func matchChoiceDetail(_ choice: StatementMatchChoice) -> String {
        switch choice {
        case .skip: String(localized: "The transaction already in the vault stands for this row from now on, and this row is not imported.")
        case .moveExisting: String(localized: "The transaction entered by hand moves into this statement’s wallet, and this row is not imported.")
        case .importAnyway: String(localized: "This row is imported as well, beside the transaction already in the vault.")
        }
    }

    /// The transaction a flagged row matched, on the dim line under it:
    /// `2026-09-14 · Bar Centrale · Bar · in Cash`, or for a transfer
    /// `2026-09-14 · Ricarica · from ING to Revolut`.
    static func existing(_ view: TransactionView, names: NameBook) -> String {
        let row = TransactionRow(view: view, names: names)
        var parts = [CoreDate.day(row.occurredAt)]
        if !row.note.isEmpty { parts.append(row.note) }
        if row.isTransfer {
            let from = names.wallet(view.fromId) ?? names.flow(view.fromId) ?? TransactionRow.placeholder
            let to = names.wallet(view.toId) ?? names.flow(view.toId) ?? TransactionRow.placeholder
            parts.append(String(localized: "from \(from) to \(to)"))
        } else {
            parts.append(row.category)
            parts.append(String(localized: "in \(row.walletDisplay)"))
        }
        return parts.joined(separator: " \u{00B7} ")
    }

    static func action(_ choice: StatementActionChoice) -> String {
        switch choice {
        case .expense: String(localized: "As expense")
        case .income: String(localized: "As income")
        case .refund: String(localized: "As refund")
        case .transferIn: String(localized: "Transfer in from…")
        case .transferOut: String(localized: "Transfer out to…")
        case .skip: String(localized: "Skip")
        case .bySign: String(localized: "By sign")
        }
    }

    /// `,` and `;` as themselves, a tab by name.
    static func delimiter(_ delimiter: String) -> String {
        switch delimiter {
        case "\t", "\\t", "tab", "TAB": String(localized: "Tab")
        default: delimiter
        }
    }
}

/// `StatementDateFormat` as one flat choice of a picker; the pattern of a
/// custom format is typed beside it.
enum StatementDateChoice: String, CaseIterable, Identifiable {
    case dateTimeUtc
    case isoDate
    case isoDateTime
    case dayMonthYear
    case monthDayYear
    case custom

    var id: String { rawValue }

    init(_ format: StatementDateFormat) {
        switch format {
        case .dateTimeUtc: self = .dateTimeUtc
        case .isoDate: self = .isoDate
        case .isoDateTime: self = .isoDateTime
        case .dayMonthYear: self = .dayMonthYear
        case .monthDayYear: self = .monthDayYear
        case .custom: self = .custom
        }
    }

    /// The format for this choice; a custom one keeps the pattern already
    /// typed, or starts from the ISO day.
    func format(keeping current: StatementDateFormat) -> StatementDateFormat {
        switch self {
        case .dateTimeUtc: .dateTimeUtc
        case .isoDate: .isoDate
        case .isoDateTime: .isoDateTime
        case .dayMonthYear: .dayMonthYear
        case .monthDayYear: .monthDayYear
        case .custom: .custom(pattern: Self.pattern(of: current) ?? "%Y-%m-%d")
        }
    }

    /// The chrono pattern of a custom format.
    static func pattern(of format: StatementDateFormat) -> String? {
        if case .custom(let pattern) = format { return pattern }
        return nil
    }

    /// An example of the format, which says more than its name.
    var label: String {
        switch self {
        case .dateTimeUtc: "2026-09-16 08:54:40 UTC"
        case .isoDate: "2026-09-16"
        case .isoDateTime: "2026-09-16T08:54:40"
        case .dayMonthYear: "16/09/2026"
        case .monthDayYear: "09/16/2026"
        case .custom: String(localized: "Custom pattern")
        }
    }
}
