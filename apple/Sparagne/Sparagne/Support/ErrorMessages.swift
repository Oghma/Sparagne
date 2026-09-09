import Foundation

/// Headlines for the stable error codes in
/// `apple/SparagneCore/Sources/SparagneCore/ErrorCodes.swift` (team-lead
/// task 1): every `DomainError` and `QuickAddError` code, mapped to a short
/// user-facing sentence. The alert keeps the Rust `Display` text
/// (`AppError.message`) as its secondary detail; this is the headline.
///
/// `"domain_error"` is never produced by the core itself (`QuickAddError
/// .Domain` carries the wrapped `DomainError`'s own code, not that literal);
/// it exists here only as the catalog key behind `default`, so an
/// unrecognized code still gets a real sentence instead of falling back to
/// the raw code string.
enum ErrorMessages {
    static func summary(for code: String) -> String {
        switch code {
        case "insufficient_funds": String(localized: "Not enough money")
        case "max_balance_reached": String(localized: "That would go over the cap")
        case "not_found": String(localized: "Not found")
        case "already_exists": String(localized: "Already exists")
        case "invalid_amount": String(localized: "Invalid amount")
        case "invalid_name": String(localized: "Invalid name")
        case "invalid_flow": String(localized: "Invalid envelope")
        case "currency_mismatch": String(localized: "Currency mismatch")
        case "invalid_command": String(localized: "That action is not allowed")
        case "invalid_cursor": String(localized: "Could not load more")
        case "storage_error": String(localized: "A storage error occurred")
        case "empty_input": String(localized: "Type something first")
        case "missing_amount": String(localized: "An amount is required")
        case "duplicate_marker": String(localized: "That marker appears twice")
        case "marker_not_allowed": String(localized: "That marker is not allowed here")
        case "missing_transfer_target": String(localized: "A transfer needs two targets")
        case "invalid_date": String(localized: "Invalid date")
        case "duplicate_date": String(localized: "That date appears twice")
        case "ambiguous_name": String(localized: "Which one did you mean?")
        case "unknown_name": String(localized: "Unknown name")
        case "same_target": String(localized: "Source and destination are the same")
        default: String(localized: "Something went wrong")
        }
    }
}
