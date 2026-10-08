import Foundation

/// Headlines for the stable error codes in
/// `apple/SparagneCore/Sources/SparagneCore/ErrorCodes.swift`: every
/// `DomainError` and `QuickAddError` code, mapped to a short user-facing
/// sentence. The alert keeps the Rust `Display` text
/// (`AppError.message`) as its secondary detail; this is the headline.
///
/// `"domain_error"` is never produced by the core itself (`QuickAddError
/// .Domain` carries the wrapped `DomainError`'s own code, not that literal);
/// it exists here only as the catalog key behind `default`, so an
/// unrecognized code still gets a real sentence instead of falling back to
/// the raw code string.
nonisolated enum ErrorMessages {
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
        // Server-side codes plus the two the transport
        // itself raises, `offline` and `invalid_server_url`.
        case "offline": String(localized: "The server is unreachable")
        // A 401 on a request that carried a token: the token expired or was
        // revoked (a password change elsewhere). A 401 on the login itself
        // reads `wrongCredentials` instead.
        case "unauthorized": sessionExpired
        case "too_many_requests": String(localized: "Too many attempts. Try again later.")
        case "forbidden": String(localized: "You are not allowed to do that")
        case "author_mismatch": String(localized: "Those changes belong to another account")
        // One command of a push, not the push: it put a row or a template on
        // someone outside the vault. `summary(for:detail:)` names them.
        case "not_a_member": String(localized: "Not a member of this vault")
        case "registration_disabled": String(localized: "This server is not accepting new accounts")
        case "invalid_request": String(localized: "The server refused the request")
        case "invalid_response": String(localized: "The server answered something unexpected")
        case "invalid_server_url": String(localized: "That server address is not valid")
        case "server_error": String(localized: "The server had a problem")
        default: String(localized: "Something went wrong")
        }
    }

    /// The headline of a command the server refused, with what its `detail`
    /// adds (`RejectedCommand.detail`): for `not_a_member`, who was named.
    /// The message is English prose and never read: without a detail (a
    /// server older than the field, a row refused before it) the code's own
    /// headline stands.
    static func summary(for code: String, detail: String?) -> String {
        guard code == "not_a_member",
              let name = detail?.trimmingCharacters(in: .whitespacesAndNewlines), !name.isEmpty
        else { return summary(for: code) }
        return String(localized: "\(name) is not a member of this vault")
    }

    /// A person cell or a `!name` that matches nobody who may be named on a
    /// row (`AppStore.assignablePeople`).
    static func unknownPerson(_ name: String) -> String {
        String(localized: "Unknown person: \(name)")
    }

    /// An `ambiguous_name` that came from a `!name`: the candidates the alert
    /// offers are people, not categories or wallets.
    static var ambiguousPerson: String { String(localized: "Which person did you mean?") }

    /// The status once the server stopped accepting the session's token.
    static var sessionExpired: String { String(localized: "Session expired — log in again") }

    /// A `401` from `POST /auth/login`: the server answers the same for an
    /// unknown user and a wrong password, and so does the app.
    static var wrongCredentials: String { String(localized: "Wrong username or password") }

    /// A `429` from the login or register limits. `Retry-After` is in
    /// seconds; people think in minutes, rounded up so "try again" is never
    /// too early.
    static func tooManyAttempts(retryAfter seconds: Int?) -> String {
        guard let seconds, seconds > 0 else { return summary(for: "too_many_requests") }
        let minutes = (seconds + 59) / 60
        return String(localized: "Too many attempts. Try again in \(minutes) minutes.")
    }
}
