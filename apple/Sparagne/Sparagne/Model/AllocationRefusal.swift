import Foundation

/// The refusals of the plan's decisions a household actually meets: two
/// people, or two Macs, deciding the same period. The core's own words name
/// the rule ("already exists", "not the latest period"); these say what
/// happened. Anything else keeps the core's message.
enum AllocationRefusal {
    /// Distribuisci or Salta on a period someone decided first.
    static func decision(_ error: AppError) -> AppError? {
        guard error.code == "already_exists" else { return nil }
        return AppError(
            code: error.code,
            message: String(localized: "Someone decided it already, on another Mac or before this window caught up. The tab now shows what was done."),
            headline: String(localized: "This period was already decided")
        )
    }

    /// Annulla on a period that is no longer the latest decision: someone
    /// undid it, or decided a later one, in the meantime.
    static func reopen(_ error: AppError) -> AppError? {
        guard error.code == "invalid_command" || error.code == "not_found" else { return nil }
        return AppError(
            code: error.code,
            message: String(localized: "In the meantime someone undid it or decided a later period. The history now shows where things stand."),
            headline: String(localized: "This period is no longer the latest decision")
        )
    }
}
