//
//  Hand-written companion to the generated bindings.
//
//  `DomainError` crosses the FFI as a flat error: Swift gets one case per Rust
//  variant carrying the `Display` message, but not the `code()` string.
//  `QuickAddError` crosses with its fields, so `.AmbiguousName` and
//  `.UnknownName` arrive with their data and only need their sentence
//  rebuilt here. These switches mirror the `code()` implementations in
//  `core/src/error.rs` and `core/src/quick_add.rs`, and the `#[error(...)]`
//  formats next to them. They are exhaustive on purpose: adding a variant in
//  Rust breaks this file at compile time instead of drifting silently.
//

extension DomainError {
    /// Stable snake_case code, identical to `DomainError::code()` in Rust.
    public var code: String {
        switch self {
        case .InsufficientFunds: "insufficient_funds"
        case .MaxBalanceReached: "max_balance_reached"
        case .NotFound: "not_found"
        case .AlreadyExists: "already_exists"
        case .InvalidAmount: "invalid_amount"
        case .InvalidName: "invalid_name"
        case .InvalidFlow: "invalid_flow"
        case .CurrencyMismatch: "currency_mismatch"
        case .InvalidCommand: "invalid_command"
        case .InvalidCursor: "invalid_cursor"
        case .Forbidden: "forbidden"
        case .Storage: "storage_error"
        }
    }

    /// The Rust `Display` message carried by every case.
    public var message: String {
        switch self {
        case .InsufficientFunds(let message),
            .MaxBalanceReached(let message),
            .NotFound(let message),
            .AlreadyExists(let message),
            .InvalidAmount(let message),
            .InvalidName(let message),
            .InvalidFlow(let message),
            .CurrencyMismatch(let message),
            .InvalidCommand(let message),
            .InvalidCursor(let message),
            .Forbidden(let message),
            .Storage(let message):
            message
        }
    }
}

extension QuickAddError {
    /// Stable snake_case code, identical to `QuickAddError::code()` in Rust,
    /// including the wrapped `DomainError`'s own code.
    public var code: String {
        switch self {
        case .EmptyInput: "empty_input"
        case .MissingAmount: "missing_amount"
        case .InvalidAmount: "invalid_amount"
        case .DuplicateMarker: "duplicate_marker"
        case .MarkerNotAllowed: "marker_not_allowed"
        case .MissingTransferTarget: "missing_transfer_target"
        case .InvalidDate: "invalid_date"
        case .DuplicateDate: "duplicate_date"
        case .AmbiguousName: "ambiguous_name"
        case .UnknownName: "unknown_name"
        case .SameTarget: "same_target"
        case .Domain(let code, _): code
        }
    }

    /// The sentence the Rust `Display` implementation produces.
    public var message: String {
        switch self {
        case .EmptyInput(let message),
            .MissingAmount(let message),
            .InvalidAmount(let message),
            .DuplicateMarker(let message),
            .MarkerNotAllowed(let message),
            .MissingTransferTarget(let message),
            .InvalidDate(let message),
            .DuplicateDate(let message),
            .SameTarget(let message),
            .Domain(_, let message):
            message
        case .AmbiguousName(let name, let candidates):
            "'\(name)' is ambiguous among \(candidates.joined(separator: ", "))"
        case .UnknownName(let kind, let name):
            "unknown \(kind) '\(name)'"
        }
    }

    /// The names a `#`, `@` or `>` marker could have meant, when the line was
    /// ambiguous; empty for every other case. The app offers them as a choice.
    public var candidates: [String] {
        switch self {
        case .AmbiguousName(_, let candidates): candidates
        default: []
        }
    }
}
