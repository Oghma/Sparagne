//
//  Hand-written companion to the generated bindings.
//
//  `DomainError` and `QuickAddError` cross the FFI as flat errors: Swift gets
//  one case per Rust variant carrying the `Display` message, but not the
//  associated data and not the `code()` string. These switches mirror the
//  `code()` implementations in `core/src/error.rs` and `core/src/quick_add.rs`.
//  They are exhaustive on purpose: adding a variant in Rust breaks this file
//  at compile time instead of drifting silently.
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
            .Storage(let message):
            message
        }
    }
}

extension QuickAddError {
    /// Stable snake_case code, identical to `QuickAddError::code()` in Rust.
    ///
    /// One difference: Rust's `Domain` variant reports the code of the wrapped
    /// `DomainError`, which the flat representation cannot carry across the
    /// FFI. Here it reports `domain_error`; read `message` for the detail.
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
        case .Domain: "domain_error"
        }
    }

    /// The Rust `Display` message carried by every case.
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
            .AmbiguousName(let message),
            .UnknownName(let message),
            .SameTarget(let message),
            .Domain(let message):
            message
        }
    }
}
