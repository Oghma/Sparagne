import SwiftUI
import SparagneCore

/// The words the import sheet puts on the core's statement values: every
/// stable code of `StatementRowStatus` translated (the English reason stays
/// as the detail), the kinds, the actions and the date formats.
enum StatementText {
    /// The status badge of a previewed row.
    static func status(_ status: StatementRowStatus) -> String {
        switch status {
        case .new: String(localized: "New")
        case .alreadyImported: String(localized: "Already imported")
        case .skipped(let code, _): skipped(code)
        case .invalid(let code, _): invalid(code)
        }
    }

    /// The core's English sentence behind a status, for the tooltip.
    static func detail(_ status: StatementRowStatus) -> String? {
        switch status {
        case .new, .alreadyImported: nil
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
        default: String(localized: "Skipped")
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
